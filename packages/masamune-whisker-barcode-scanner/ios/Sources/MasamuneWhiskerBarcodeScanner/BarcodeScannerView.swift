// Scanner element of masamune-whisker-barcode-scanner (iOS), registered by BarcodeScannerModule.
//
// An AVCaptureSession feeds an AVCaptureVideoPreviewLayer and an AVCaptureMetadataOutput limited to QR codes. The
// session is configured, started and stopped on a private serial queue (startRunning blocks); everything else runs on
// the main thread. The camera runs while the element is in a window and `active` is true.

import AVFoundation
import UIKit
import WhiskerModule

/// The preview: a view backed by the capture preview layer, so the layer follows the view's size.
public final class ScannerPreviewView: UIView {
    public override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }

    var previewLayer: AVCaptureVideoPreviewLayer {
        // swiftlint:disable:next force_cast
        layer as! AVCaptureVideoPreviewLayer
    }

    public override func layoutSubviews() {
        super.layoutSubviews()
        updateOrientation()
    }

    public override func didMoveToWindow() {
        super.didMoveToWindow()
        updateOrientation()
    }

    /// Keeps the preview upright in the current interface orientation.
    func updateOrientation() {
        guard let connection = previewLayer.connection, connection.isVideoOrientationSupported else { return }
        switch window?.windowScene?.interfaceOrientation {
        case .landscapeLeft: connection.videoOrientation = .landscapeLeft
        case .landscapeRight: connection.videoOrientation = .landscapeRight
        case .portraitUpsideDown: connection.videoOrientation = .portraitUpsideDown
        default: connection.videoOrientation = .portrait
        }
    }
}

@objc(MasamuneWhiskerBarcodeScannerView)
public final class BarcodeScannerView: WhiskerUI<ScannerPreviewView>, AVCaptureMetadataOutputObjectsDelegate {
    static let defaultDebounceMs = 1500

    private let sessionQueue = DispatchQueue(label: "net.mathru.whisker.barcode_scanner.session")
    /// Session objects, used on `sessionQueue` only.
    private var session: AVCaptureSession?
    private var device: AVCaptureDevice?
    private var observers: [NSObjectProtocol] = []

    private var requestPermission = true
    private var front = false
    private var debounceMs = BarcodeScannerView.defaultDebounceMs
    private var torch = false
    private var active = false

    /// Whether the camera should run; `generation` invalidates callbacks of an earlier start.
    private var running = false
    private var generation = 0

    private var lastValue: String?
    private var lastSeen: TimeInterval = 0

    @objc public override func createView() -> ScannerPreviewView {
        let view = ScannerPreviewView()
        view.backgroundColor = .black
        view.previewLayer.videoGravity = .resizeAspectFill
        return view
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
        let session = session
        sessionQueue.async { session?.stopRunning() }
    }

    // MARK: - Props

    func setRequestPermission(_ request: Bool) {
        requestPermission = request
    }

    func setFacing(_ value: String) {
        let wantsFront = value == "front"
        guard wantsFront != front else { return }
        front = wantsFront
        // The session is rebuilt for the other camera.
        let wasRunning = running
        if wasRunning { stop() }
        sessionQueue.async { [weak self] in self?.tearDownSession() }
        if wasRunning { update() }
    }

    func setDebounceMs(_ value: Int) {
        debounceMs = max(0, value)
    }

    func setTorch(_ on: Bool) {
        torch = on
        applyTorch()
    }

    func setActive(_ value: Bool) {
        active = value
        update()
    }

    // MARK: - Lifecycle

    public override func didMoveToWindow() {
        super.didMoveToWindow()
        update()
    }

    private func update() {
        let shouldRun = active && window != nil
        if shouldRun && !running {
            start()
        } else if !shouldRun && running {
            stop()
        }
    }

    private func start() {
        running = true
        lastValue = nil
        generation += 1
        let gen = generation
        let status = CameraAccess.status()
        if status == "notDetermined" && requestPermission {
            CameraAccess.request { [weak self] result in
                guard let self, gen == self.generation, self.running else { return }
                self.emit("permission", ["status": result])
                if result == "authorized" { self.run(gen) }
            }
            return
        }
        emit("permission", ["status": status])
        if status == "authorized" { run(gen) }
    }

    private func stop() {
        running = false
        generation += 1
        sessionQueue.async { [weak self] in
            guard let self else { return }
            self.setTorchMode(false)
            self.session?.stopRunning()
        }
    }

    // MARK: - Session (sessionQueue)

    private func run(_ gen: Int) {
        let front = front
        sessionQueue.async { [weak self] in
            guard let self else { return }
            let session: AVCaptureSession
            if let existing = self.session {
                session = existing
            } else {
                switch self.makeSession(front: front) {
                case .success(let made):
                    session = made
                case .failure(let failure):
                    DispatchQueue.main.async {
                        guard gen == self.generation else { return }
                        self.emit("error", ["code": failure.code, "message": failure.message])
                    }
                    return
                }
            }
            DispatchQueue.main.async {
                if self.view().previewLayer.session !== session {
                    self.view().previewLayer.session = session
                    self.view().updateOrientation()
                }
            }
            if !session.isRunning { session.startRunning() }
            DispatchQueue.main.async {
                guard gen == self.generation, self.running else {
                    // Stopped while starting.
                    self.sessionQueue.async { session.stopRunning() }
                    return
                }
                self.applyTorch()
            }
        }
    }

    private struct SessionFailure: Error {
        let code: String
        let message: String
    }

    private func makeSession(front: Bool) -> Result<AVCaptureSession, SessionFailure> {
        let position: AVCaptureDevice.Position = front ? .front : .back
        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: position)
            ?? AVCaptureDevice.default(for: .video)
        else {
            return .failure(SessionFailure(code: "unavailable", message: "no camera is available"))
        }
        let session = AVCaptureSession()
        session.beginConfiguration()
        defer { session.commitConfiguration() }
        do {
            let input = try AVCaptureDeviceInput(device: device)
            guard session.canAddInput(input) else {
                return .failure(SessionFailure(code: "configuration", message: "camera input rejected"))
            }
            session.addInput(input)
        } catch {
            return .failure(SessionFailure(code: "unavailable", message: Bridge.message(error)))
        }
        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else {
            return .failure(SessionFailure(code: "configuration", message: "metadata output rejected"))
        }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        guard output.availableMetadataObjectTypes.contains(.qr) else {
            return .failure(SessionFailure(code: "unavailable", message: "QR detection is not available"))
        }
        output.metadataObjectTypes = [.qr]
        self.session = session
        self.device = device
        observe(session)
        return .success(session)
    }

    private func tearDownSession() {
        session?.stopRunning()
        session = nil
        device = nil
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.observers.forEach(NotificationCenter.default.removeObserver)
            self.observers.removeAll()
            self.view().previewLayer.session = nil
        }
    }

    private func observe(_ session: AVCaptureSession) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let center = NotificationCenter.default
            self.observers.append(center.addObserver(forName: AVCaptureSession.runtimeErrorNotification, object: session, queue: .main) { [weak self] note in
                guard let self, self.running else { return }
                let error = note.userInfo?[AVCaptureSessionErrorKey] as? Error
                self.emit("error", ["code": "configuration", "message": error.map(Bridge.message) ?? "capture session error"])
            })
            self.observers.append(center.addObserver(forName: AVCaptureSession.wasInterruptedNotification, object: session, queue: .main) { [weak self] _ in
                guard let self, self.running else { return }
                self.emit("error", ["code": "interrupted", "message": "the camera is used by another app"])
            })
        }
    }

    /// Sets the torch on the session queue (the torch only lights while the session runs).
    private func setTorchMode(_ on: Bool) {
        guard let device, device.hasTorch, device.isTorchAvailable else { return }
        do {
            try device.lockForConfiguration()
            device.torchMode = on ? .on : .off
            device.unlockForConfiguration()
        } catch {
            NSLog("MasamuneWhiskerBarcodeScanner: torch unavailable: %@", Bridge.message(error))
        }
    }

    private func applyTorch() {
        let on = torch && running
        sessionQueue.async { [weak self] in self?.setTorchMode(on) }
    }

    // MARK: - AVCaptureMetadataOutputObjectsDelegate

    public func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard running else { return }
        for case let code as AVMetadataMachineReadableCodeObject in metadataObjects where code.type == .qr {
            guard let value = code.stringValue, !value.isEmpty else { continue }
            report(value)
        }
    }

    /// Emits a decoded code unless it repeats the last one within the window (every sighting restarts it).
    private func report(_ value: String) {
        let now = ProcessInfo.processInfo.systemUptime
        let repeated = value == lastValue && (now - lastSeen) * 1000 < Double(debounceMs)
        lastValue = value
        lastSeen = now
        guard !repeated else { return }
        emit("scan", ["value": value, "format": "qr"])
    }

    private func emit(_ name: String, _ params: [String: Any] = [:]) {
        WhiskerCustomEvent.dispatch(from: self, name: name, params: params)
    }
}
