// Output deadlines and output events of the native queue player, measured with real playback.

import AVFoundation
import XCTest
import WhiskerModule
@testable import MasamuneWhiskerAudioPlaylist

final class OutputBoundaryTests: XCTestCase {
    private static let moduleName = "masamune-whisker-audio-playlist:AudioPlaylist"
    private var module: AudioPlaylistModule!
    private var events: [[String: Any]] = []

    override func setUp() {
        super.setUp()
        module = AudioPlaylistModule()
        module.qualifiedName = Self.moduleName
        WhiskerModuleEventCenter.register(module)
        events = []
        WhiskerModuleEventCenter.installEventSink(owner: self) { [weak self] _, _, value in
            guard let raw = value.asString, let data = raw.data(using: .utf8),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
            Bridge.onMain { self?.events.append(json) }
        }
        WhiskerModuleEventCenter.setObserving(owner: self, module: Self.moduleName, event: "event", observing: true)
    }

    override func tearDown() {
        _ = call("stop")
        WhiskerModuleEventCenter.installEventSink(owner: self, nil)
        module = nil
        super.tearDown()
    }

    func testQueueItemStopsBeforeItsDeadline() throws {
        let url = try Self.tone(seconds: 4)
        let deadline = Self.now() + 1_500_000_000
        XCTAssertNotNil(call("setQueue", ["items": [["id": "a", "uri": url.path, "outputBeforeNs": deadline]]]))
        XCTAssertNotNil(call("play"))
        wait(seconds: 2.5)

        let started = try XCTUnwrap(event("outputStarted", id: "a"))
        let earliest = try XCTUnwrap(started["earliestNs"] as? NSNumber).uint64Value
        let first = try XCTUnwrap(started["firstFrameNs"] as? NSNumber).uint64Value
        let latest = try XCTUnwrap(started["latestNs"] as? NSNumber).uint64Value
        XCTAssertLessThanOrEqual(earliest, first)
        XCTAssertLessThanOrEqual(first, latest)
        XCTAssertLessThan(latest, deadline)
        XCTAssertNotNil(events.first { $0["type"] as? String == "itemEnded" && $0["reason"] as? String == "expired" })
        let ended = try XCTUnwrap(event("outputEnded", id: "a"))
        let last = try XCTUnwrap(ended["lastFrameNs"] as? NSNumber).uint64Value
        XCTAssertLessThan(last, deadline, "a frame may have been presented at or after the deadline")
        print("OUTPUT_BOUNDARY queue first=[\(earliest),\(first),\(latest)] last=\(last) deadline=\(deadline) margin_ms=\(Double(deadline - last) / 1e6)")
    }

    func testShortenedDeadlineAppliesToTheRunningItem() throws {
        let url = try Self.tone(seconds: 4)
        XCTAssertNotNil(call("setQueue", ["items": [["id": "a", "uri": url.path, "outputBeforeNs": Self.now() + 3_500_000_000]]]))
        XCTAssertNotNil(call("play"))
        wait(seconds: 0.6)
        let deadline = Self.now() + 500_000_000
        XCTAssertEqual(call("setOutputDeadline", ["id": "a", "outputBeforeNs": deadline]) as? Bool, true)
        // A later deadline does not extend it.
        XCTAssertEqual(call("setOutputDeadline", ["id": "a", "outputBeforeNs": deadline + 2_000_000_000]) as? Bool, true)
        wait(seconds: 1.2)
        let ended = try XCTUnwrap(event("outputEnded", id: "a"))
        let last = try XCTUnwrap(ended["lastFrameNs"] as? NSNumber).uint64Value
        XCTAssertLessThan(last, deadline)
        print("OUTPUT_BOUNDARY shortened last=\(last) deadline=\(deadline) margin_ms=\(Double(deadline - last) / 1e6)")
    }

    func testItemTooCloseToItsDeadlineNeverStarts() throws {
        let url = try Self.tone(seconds: 2)
        XCTAssertNotNil(call("setQueue", ["items": [["id": "late", "uri": url.path, "outputBeforeNs": Self.now() + 30_000_000]]]))
        XCTAssertNotNil(call("play"))
        wait(seconds: 0.8)
        XCTAssertNil(event("outputStarted", id: "late"))
        XCTAssertNotNil(events.first { $0["type"] as? String == "itemEnded" && $0["id"] as? String == "late" && $0["reason"] as? String == "expired" })
    }

    func testAnnouncementReportsItsFirstFrameAndEndsAtItsDeadline() throws {
        let url = try Self.tone(seconds: 3)
        let deadline = Self.now() + 1_000_000_000
        XCTAssertNotNil(call("interrupt", ["id": "n1", "uri": url.path, "mode": "pause", "volume": 1.0, "outputBeforeNs": deadline]))
        wait(seconds: 1.8)
        let started = try XCTUnwrap(event("outputStarted", id: "n1"))
        let latest = try XCTUnwrap(started["latestNs"] as? NSNumber).uint64Value
        XCTAssertLessThan(latest, deadline)
        let ended = try XCTUnwrap(event("outputEnded", id: "n1"))
        XCTAssertLessThan(try XCTUnwrap(ended["lastFrameNs"] as? NSNumber).uint64Value, deadline)
        let interruptEnded = try XCTUnwrap(events.first { $0["type"] as? String == "interruptEnded" })
        XCTAssertEqual(interruptEnded["completed"] as? Bool, false)
    }

    func testCancelledAnnouncementHasNoOutput() throws {
        let url = try Self.tone(seconds: 3)
        XCTAssertNotNil(call("interrupt", ["id": "n2", "uri": url.path, "mode": "pause", "volume": 1.0]))
        XCTAssertEqual(call("cancelInterrupt") as? Bool, true)
        wait(seconds: 0.6)
        XCTAssertNil(event("outputStarted", id: "n2"))
    }

    // MARK: - Helpers

    private func call(_ name: String, _ args: [String: Any] = [:]) -> Any? {
        guard let function = module.definitionLazy.functions.first(where: { $0.name == name }) else {
            XCTFail("missing function \(name)")
            return nil
        }
        let data = try! JSONSerialization.data(withJSONObject: args)
        let result = function.handler([.string(String(data: data, encoding: .utf8)!)])
        guard let raw = result.asString, let json = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any] else {
            return nil
        }
        return json["value"]
    }

    private func event(_ type: String, id: String) -> [String: Any]? {
        events.first { $0["type"] as? String == type && $0["id"] as? String == id }
    }

    private func wait(seconds: TimeInterval) {
        RunLoop.main.run(until: Date().addingTimeInterval(seconds))
    }

    private static func now() -> UInt64 {
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        let product = mach_continuous_time().multipliedFullWidth(by: UInt64(info.numer))
        return UInt64(info.denom).dividingFullWidth(product).quotient
    }

    /// A 440 Hz tone as 16-bit mono PCM WAV.
    private static func tone(seconds: Double) throws -> URL {
        let rate = 44_100
        let count = Int(Double(rate) * seconds)
        var pcm = Data(capacity: count * 2)
        for index in 0..<count {
            let sample = Int16(sin(2 * Double.pi * 440 * Double(index) / Double(rate)) * 12_000)
            withUnsafeBytes(of: sample.littleEndian) { pcm.append(contentsOf: $0) }
        }
        var wav = Data()
        func append<T: FixedWidthInteger>(_ value: T) { withUnsafeBytes(of: value.littleEndian) { wav.append(contentsOf: $0) } }
        wav.append(contentsOf: Array("RIFF".utf8)); append(UInt32(36 + pcm.count))
        wav.append(contentsOf: Array("WAVEfmt ".utf8)); append(UInt32(16)); append(UInt16(1)); append(UInt16(1))
        append(UInt32(rate)); append(UInt32(rate * 2)); append(UInt16(2)); append(UInt16(16))
        wav.append(contentsOf: Array("data".utf8)); append(UInt32(pcm.count)); wav.append(pcm)
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("tone-\(UUID().uuidString).wav")
        try wav.write(to: url)
        return url
    }
}
