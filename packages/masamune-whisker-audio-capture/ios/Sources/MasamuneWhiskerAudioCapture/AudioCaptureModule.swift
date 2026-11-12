// Microphone capture as 16 kHz (configurable) mono PCM16 little-endian chunks.
//
// AVAudioEngine taps the input node, AVAudioConverter resamples it, and every chunk is sent to Rust
// as an "audio" event ({ id, data: base64 }). The app must declare `NSMicrophoneUsageDescription`
// (for example with the masamune-whisker-ai-debugger CNG plugin); iOS terminates the app without it.

import AVFoundation
import Foundation
import WhiskerModule

@WhiskerModule
public final class AudioCaptureModule: Module {
    private var capture: Capture?

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("AudioCapture")
            Events("audio")

            // start({ id, sampleRate, chunkMs }) -> true. Rejects with "permission_denied" when the
            // user denies the microphone. A running capture is stopped first.
            AsyncFunction("start") { [weak self] args, promise in
                let json = Bridge.args(args)
                let id = json["id"] as? String ?? ""
                let sampleRate = (json["sampleRate"] as? NSNumber)?.doubleValue ?? 16000
                let chunkMs = (json["chunkMs"] as? NSNumber)?.intValue ?? 100
                Self.requestPermission { granted in
                    Bridge.onMain {
                        guard let self else {
                            Bridge.reject(promise, "AudioCapture module released")
                            return
                        }
                        guard granted else {
                            Bridge.reject(promise, "permission_denied")
                            return
                        }
                        self.begin(id: id, sampleRate: sampleRate, chunkMs: chunkMs, promise: promise)
                    }
                }
            }

            // stop({ id }) -> { data: base64 of the audio after the last chunk } | null (not running).
            AsyncFunction("stop") { [weak self] args, promise in
                let id = Bridge.args(args)["id"] as? String ?? ""
                Bridge.onMain {
                    guard let rest = self?.finish(id: id) else {
                        Bridge.resolve(promise, nil)
                        return
                    }
                    Bridge.resolve(promise, ["data": rest.base64EncodedString()])
                }
            }

            // cancel({ id }) — stops without returning the remaining audio.
            Function("cancel") { [weak self] args in
                let id = Bridge.args(args)["id"] as? String ?? ""
                Bridge.onMain { _ = self?.finish(id: id) }
                return Bridge.ok(true)
            }
        }
    }

    private static func requestPermission(_ handler: @escaping (Bool) -> Void) {
        if #available(iOS 17.0, *) {
            AVAudioApplication.requestRecordPermission(completionHandler: handler)
        } else {
            AVAudioSession.sharedInstance().requestRecordPermission(handler)
        }
    }

    private func begin(id: String, sampleRate: Double, chunkMs: Int, promise: WhiskerPromise) {
        capture?.stop()
        capture = nil
        let capture = Capture(
            id: id,
            sampleRate: sampleRate,
            chunkBytes: max(2, Int(sampleRate) * 2 * chunkMs / 1000),
            emit: { [weak self] id, data in
                self?.sendEvent("audio", Bridge.event(["id": id, "data": data.base64EncodedString()]))
            },
            fail: { [weak self] id, message in
                guard let self, self.capture?.id == id else { return }
                self.capture?.stop()
                self.capture = nil
                self.sendEvent("audio", Bridge.event(["id": id, "error": message]))
            }
        )
        do {
            try capture.start()
            self.capture = capture
            Bridge.resolve(promise, true)
        } catch {
            capture.stop()
            Bridge.reject(promise, Bridge.message(error))
        }
    }

    /// Stops the capture `id` and returns its remaining audio, or nil when it is not running.
    private func finish(id: String) -> Data? {
        guard let capture, capture.id == id else { return nil }
        self.capture = nil
        return capture.stop()
    }
}

private struct CaptureError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

/// One capture. Every method runs on the main thread except the tap, which hands its converted
/// samples to the main thread.
private final class Capture {
    let id: String
    private let sampleRate: Double
    private let chunkBytes: Int
    private let emit: (String, Data) -> Void
    private let fail: (String, String) -> Void
    private var engine: AVAudioEngine?
    private var pending = Data()
    private var running = false
    private var observer: NSObjectProtocol?
    private var previousCategory: AVAudioSession.Category?
    private var previousMode: AVAudioSession.Mode?
    private var previousOptions: AVAudioSession.CategoryOptions?

    init(
        id: String,
        sampleRate: Double,
        chunkBytes: Int,
        emit: @escaping (String, Data) -> Void,
        fail: @escaping (String, String) -> Void
    ) {
        self.id = id
        self.sampleRate = sampleRate
        self.chunkBytes = chunkBytes
        self.emit = emit
        self.fail = fail
    }

    func start() throws {
        let session = AVAudioSession.sharedInstance()
        previousCategory = session.category
        previousMode = session.mode
        previousOptions = session.categoryOptions
        try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
        try session.setActive(true)

        let engine = AVAudioEngine()
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard
            inputFormat.sampleRate > 0, inputFormat.channelCount > 0,
            let outputFormat = AVAudioFormat(
                commonFormat: .pcmFormatInt16,
                sampleRate: sampleRate,
                channels: 1,
                interleaved: true
            ),
            let converter = AVAudioConverter(from: inputFormat, to: outputFormat)
        else {
            throw CaptureError("start_failed: the microphone input format is not supported")
        }
        let bufferSize = AVAudioFrameCount(max(256, inputFormat.sampleRate / 10))
        input.installTap(onBus: 0, bufferSize: bufferSize, format: inputFormat) { [weak self] buffer, _ in
            guard let data = Self.convert(buffer, converter: converter, outputFormat: outputFormat) else {
                return
            }
            DispatchQueue.main.async { self?.append(data) }
        }
        engine.prepare()
        try engine.start()
        self.engine = engine
        running = true
        observer = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: session,
            queue: .main
        ) { [weak self] notification in
            guard let self,
                  let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                  AVAudioSession.InterruptionType(rawValue: raw) == .began
            else { return }
            self.fail(self.id, "interrupted")
        }
    }

    /// Stops capturing and returns the samples that did not fill a chunk yet.
    @discardableResult
    func stop() -> Data {
        if let observer {
            NotificationCenter.default.removeObserver(observer)
        }
        observer = nil
        if let engine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
        engine = nil
        running = false
        let rest = pending
        pending = Data()
        restoreSession()
        return rest
    }

    private func append(_ data: Data) {
        guard running else { return }
        pending.append(data)
        while pending.count >= chunkBytes {
            emit(id, Data(pending.prefix(chunkBytes)))
            pending.removeFirst(chunkBytes)
        }
    }

    private static func convert(
        _ buffer: AVAudioPCMBuffer,
        converter: AVAudioConverter,
        outputFormat: AVAudioFormat
    ) -> Data? {
        let ratio = outputFormat.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
        guard let output = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity) else {
            return nil
        }
        var supplied = false
        var error: NSError?
        let status = converter.convert(to: output, error: &error) { _, inputStatus in
            if supplied {
                inputStatus.pointee = .noDataNow
                return nil
            }
            supplied = true
            inputStatus.pointee = .haveData
            return buffer
        }
        guard status != .error, error == nil, output.frameLength > 0,
              let channel = output.int16ChannelData
        else { return nil }
        return Data(bytes: channel[0], count: Int(output.frameLength) * MemoryLayout<Int16>.size)
    }

    private func restoreSession() {
        guard let category = previousCategory else { return }
        let session = AVAudioSession.sharedInstance()
        try? session.setActive(false, options: .notifyOthersOnDeactivation)
        try? session.setCategory(category, mode: previousMode ?? .default, options: previousOptions ?? [])
        previousCategory = nil
        previousMode = nil
        previousOptions = nil
    }
}
