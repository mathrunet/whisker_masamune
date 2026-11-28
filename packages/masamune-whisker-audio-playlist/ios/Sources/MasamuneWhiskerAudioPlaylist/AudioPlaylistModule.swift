// Gapless queue playback (iOS) with announcement interrupts.
//
// - The queue is an AVQueuePlayer. `entries` mirrors its items, current item first; AVQueuePlayer
//   advances by itself at the end of an item (`actionAtItemEnd = .advance`), which keeps the
//   handover gapless, and the end notification then updates `entries` and reports the events.
// - Announcements play once on a separate AVPlayer. In pause mode the queue pauses (keeping its
//   position) and resumes afterwards when the playlist is still playing; in duck mode its volume
//   is lowered meanwhile.
// - The audio session category is set to `.playback` before playing only when it is neither
//   `.playback` nor `.playAndRecord`, so a configuration made by masamune-whisker-audio-session
//   (mixWithOthers, duckOthers) is kept. Background playback needs the `audio` background mode.
// - Output deadlines and output events use `mach_continuous_time` (keeps counting while the
//   device sleeps). While something plays, a monitor runs every `monitorInterval`: it ends items
//   and announcements early enough that no frame is presented at or after their deadline (the
//   session's output latency and two monitor periods are kept as a margin) and reports when audio
//   starts reaching the output (the playback position advances) and when it stops.
// - Everything runs on the main thread; KVO callbacks are moved there.

import AVFoundation
import Foundation
import WhiskerModule

@WhiskerModule
public final class AudioPlaylistModule: Module {
    private struct Entry {
        let id: String
        let item: AVPlayerItem
        var deadline: UInt64?
    }

    private struct Announcement {
        let id: String
        let duck: Bool
        let duckVolume: Float
        let player: AVPlayer
        let item: AVPlayerItem
        var observation: NSKeyValueObservation?
        var deadline: UInt64?
    }

    /// Output of the queue or the announcement: whether `id` reaches the output.
    private struct Output {
        let id: String
        let item: AVPlayerItem
        var audible = false
        /// Last monitor time at which no new frame had been presented (earliest first frame).
        var silentAt: UInt64
        var silentPosition: Double
    }

    private static let monitorInterval: UInt64 = 40_000_000
    private var monitor: DispatchSourceTimer?
    private var queueOutput: Output?
    private var announcementOutput: Output?

    private let queue: AVQueuePlayer = {
        let player = AVQueuePlayer()
        player.actionAtItemEnd = .advance
        return player
    }()

    private var entries: [Entry] = []
    private var itemObservations: [ObjectIdentifier: NSKeyValueObservation] = [:]
    private var playing = false
    private var volume: Float = 1
    private var announcement: Announcement?
    private var observers: [NSObjectProtocol] = []

    public override func definition() -> ModuleDefinition {
        installObservers()
        return ModuleDefinition {
            Name("AudioPlaylist")
            Events("event")

            // setQueue({ items: [{ id, uri }] })
            Function("setQueue") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                do {
                    let items = try Self.items(Bridge.args(args))
                    try Self.checkUnique(items.map(\.id), existing: [])
                    self.setQueue(items)
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // append({ items: [{ id, uri }] })
            Function("append") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                do {
                    let items = try Self.items(Bridge.args(args))
                    try Self.checkUnique(items.map(\.id), existing: self.entries.map(\.id))
                    self.append(items)
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // remove({ id }) -> whether the item was queued
            Function("remove") { [weak self] args in
                guard let self, let id = Bridge.args(args)["id"] as? String else { return Bridge.ok(false) }
                return Bridge.ok(self.remove(id))
            }

            Function("play") { [weak self] _ in
                guard let self else { return Bridge.ok(false) }
                self.playing = true
                self.applyPlayback()
                return Bridge.ok(true)
            }

            Function("pause") { [weak self] _ in
                guard let self else { return Bridge.ok(false) }
                self.playing = false
                self.applyPlayback()
                return Bridge.ok(true)
            }

            Function("stop") { [weak self] _ in
                guard let self else { return Bridge.ok(false) }
                self.stop()
                return Bridge.ok(true)
            }

            // skipToNext() -> whether a next item exists
            Function("skipToNext") { [weak self] _ in
                guard let self, !self.entries.isEmpty else { return Bridge.ok(false) }
                self.endCurrent("skipped")
                return Bridge.ok(!self.entries.isEmpty)
            }

            // setVolume({ volume })
            Function("setVolume") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                guard let volume = (Bridge.args(args)["volume"] as? NSNumber)?.doubleValue, volume.isFinite else {
                    return Bridge.err("invalid volume")
                }
                self.volume = Float(min(max(volume, 0), 1))
                self.applyVolume()
                return Bridge.ok(true)
            }

            // status() -> { currentId, positionMs, durationMs, playing, interrupting, queue, volume }
            Function("status") { [weak self] _ in
                guard let self else { return Bridge.ok(nil) }
                return Bridge.ok(self.status())
            }

            // interrupt({ id, uri, mode, duckVolume, volume })
            Function("interrupt") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                let json = Bridge.args(args)
                guard let id = json["id"] as? String, let uri = json["uri"] as? String, let url = Self.url(uri) else {
                    return Bridge.err("invalid announcement")
                }
                let duck = json["mode"] as? String == "duck"
                let duckVolume = Float((json["duckVolume"] as? NSNumber)?.doubleValue ?? 0.2)
                let volume = Float((json["volume"] as? NSNumber)?.doubleValue ?? 1)
                let deadline = (json["outputBeforeNs"] as? NSNumber)?.uint64Value
                self.interrupt(id: id, url: url, duck: duck, duckVolume: duckVolume, volume: volume, deadline: deadline)
                return Bridge.ok(true)
            }

            // setOutputDeadline({ id, outputBeforeNs }) -> whether the id was queued or announcing
            Function("setOutputDeadline") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                let json = Bridge.args(args)
                guard let id = json["id"] as? String, let deadline = (json["outputBeforeNs"] as? NSNumber)?.uint64Value else {
                    return Bridge.err("invalid output deadline")
                }
                return Bridge.ok(self.setOutputDeadline(id: id, deadline: deadline))
            }

            // cancelInterrupt() -> whether an announcement was playing
            Function("cancelInterrupt") { [weak self] _ in
                guard let self else { return Bridge.ok(false) }
                return Bridge.ok(self.endAnnouncement(completed: false))
            }
        }
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    // MARK: - Queue

    private func setQueue(_ items: [(id: String, url: URL, deadline: UInt64?)]) {
        if let current = entries.first {
            emit(["type": "itemEnded", "id": current.id, "reason": "removed"])
        }
        entries.forEach { forget($0.item) }
        queue.removeAllItems()
        entries = []
        for (id, url, deadline) in items {
            enqueue(id: id, url: url, deadline: deadline)
        }
        if !entries.isEmpty {
            startCurrent()
        }
        applyPlayback()
    }

    private func append(_ items: [(id: String, url: URL, deadline: UInt64?)]) {
        let wasEmpty = entries.isEmpty
        for (id, url, deadline) in items {
            enqueue(id: id, url: url, deadline: deadline)
        }
        if wasEmpty && !entries.isEmpty {
            startCurrent()
        }
        applyPlayback()
    }

    private func enqueue(id: String, url: URL, deadline: UInt64?) {
        let item = AVPlayerItem(url: url)
        let key = ObjectIdentifier(item)
        itemObservations[key] = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard item.status == .failed else { return }
            let message = item.error.map(Bridge.message) ?? "failed to load the source"
            Bridge.onMain { self?.fail(item, message: message) }
        }
        queue.insert(item, after: nil)
        entries.append(Entry(id: id, item: item, deadline: deadline))
    }

    private func remove(_ id: String) -> Bool {
        guard let index = entries.firstIndex(where: { $0.id == id }) else { return false }
        if index == 0 {
            endCurrent("removed")
        } else {
            let entry = entries.remove(at: index)
            forget(entry.item)
            queue.remove(entry.item)
        }
        return true
    }

    private func stop() {
        endAnnouncement(completed: false, resume: false)
        if let current = entries.first {
            emit(["type": "itemEnded", "id": current.id, "reason": "stopped"])
        }
        playing = false
        queue.pause()
        entries.forEach { forget($0.item) }
        queue.removeAllItems()
        entries = []
        updateOutputs()
        updateMonitor()
    }

    /// Ends the current item with `reason`, drops it from the player and reports the next one.
    private func endCurrent(_ reason: String) {
        guard !entries.isEmpty else { return }
        let entry = entries.removeFirst()
        forget(entry.item)
        emit(["type": "itemEnded", "id": entry.id, "reason": reason])
        if queue.items().contains(entry.item) {
            queue.remove(entry.item)
        }
        updateOutputs()
        startNext()
    }

    /// Reports the new current item, or that the queue ran dry.
    private func startNext() {
        if entries.isEmpty {
            emit(["type": "queueDrained"])
        } else {
            startCurrent()
            applyPlayback()
        }
    }

    private func startCurrent() {
        guard let current = entries.first else { return }
        emit(["type": "itemStarted", "id": current.id, "remaining": entries.count - 1])
    }

    private func itemDidEnd(_ item: AVPlayerItem) {
        if let index = entries.firstIndex(where: { $0.item === item }) {
            // Items before it ended earlier without a notification (should not happen).
            for _ in 0..<index {
                let skipped = entries.removeFirst()
                forget(skipped.item)
                emit(["type": "itemEnded", "id": skipped.id, "reason": "completed"])
            }
            // AVQueuePlayer already moved on to the next item.
            let entry = entries.removeFirst()
            forget(entry.item)
            emit(["type": "itemEnded", "id": entry.id, "reason": "completed"])
            updateOutputs()
            startNext()
        } else if let announcement, announcement.item === item {
            endAnnouncement(completed: true)
        }
    }

    private func fail(_ item: AVPlayerItem, message: String) {
        if let index = entries.firstIndex(where: { $0.item === item }) {
            let id = entries[index].id
            emit(["type": "error", "id": id, "message": message])
            if index == 0 {
                endCurrent("failed")
            } else {
                // A queued item failed while loading ahead: it ends once it would start.
                let entry = entries.remove(at: index)
                forget(entry.item)
                queue.remove(entry.item)
                emit(["type": "itemEnded", "id": id, "reason": "failed"])
            }
        } else if let announcement, announcement.item === item {
            emit(["type": "error", "id": announcement.id, "message": message])
            endAnnouncement(completed: false)
        }
    }

    private func forget(_ item: AVPlayerItem) {
        itemObservations.removeValue(forKey: ObjectIdentifier(item))?.invalidate()
    }

    /// Plays the queue when the playlist is playing and no pausing announcement runs.
    private func applyPlayback() {
        enforceDeadlines()
        let pausedByAnnouncement = announcement.map { !$0.duck } ?? false
        if playing && !pausedByAnnouncement && !entries.isEmpty {
            Self.ensureCategory()
            queue.play()
        } else {
            queue.pause()
        }
        updateMonitor()
    }

    private func applyVolume() {
        let duck = announcement.flatMap { $0.duck ? $0.duckVolume : nil } ?? 1
        queue.volume = volume * duck
    }

    private func status() -> [String: Any?] {
        var position: Int64 = 0
        var duration: Int64?
        if let current = entries.first {
            let seconds = queue.currentItem === current.item ? CMTimeGetSeconds(queue.currentTime()) : 0
            position = seconds.isFinite ? Int64(max(seconds, 0) * 1000) : 0
            let total = CMTimeGetSeconds(current.item.duration)
            duration = total.isFinite ? Int64(total * 1000) : nil
        }
        return [
            "currentId": entries.first?.id,
            "positionMs": position,
            "durationMs": duration,
            "playing": playing,
            "interrupting": announcement != nil,
            "queue": entries.map(\.id),
            "volume": Double(volume),
        ]
    }

    // MARK: - Announcements

    private func interrupt(id: String, url: URL, duck: Bool, duckVolume: Float, volume: Float, deadline: UInt64?) {
        endAnnouncement(completed: false, resume: false)
        let item = AVPlayerItem(url: url)
        let player = AVPlayer(playerItem: item)
        player.volume = min(max(volume, 0), 1)
        var next = Announcement(id: id, duck: duck, duckVolume: min(max(duckVolume, 0), 1), player: player, item: item, deadline: deadline)
        next.observation = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard item.status == .failed else { return }
            let message = item.error.map(Bridge.message) ?? "failed to load the source"
            Bridge.onMain { self?.fail(item, message: message) }
        }
        announcement = next
        applyVolume()
        applyPlayback()
        emit(["type": "interruptStarted", "id": id])
        if let deadline, Self.now() &+ guardNs() >= deadline {
            // Too close to its deadline to be presented in full: it never starts.
            endAnnouncement(completed: false)
            return
        }
        Self.ensureCategory()
        player.play()
        updateMonitor()
    }

    /// Stops the announcement and, with `resume`, restores the queue. Returns whether one ran.
    @discardableResult
    private func endAnnouncement(completed: Bool, resume: Bool = true) -> Bool {
        guard let current = announcement else { return false }
        announcement = nil
        current.observation?.invalidate()
        current.player.pause()
        current.player.replaceCurrentItem(with: nil)
        applyVolume()
        if resume {
            applyPlayback()
        }
        updateOutputs()
        emit(["type": "interruptEnded", "id": current.id, "completed": completed])
        updateMonitor()
        return true
    }

    // MARK: - Output deadlines and output events

    private func setOutputDeadline(id: String, deadline: UInt64) -> Bool {
        let tightened: (UInt64?) -> UInt64 = { current in current.map { min($0, deadline) } ?? deadline }
        if var current = announcement, current.id == id {
            current.deadline = tightened(current.deadline)
            announcement = current
        } else if let index = entries.firstIndex(where: { $0.id == id }) {
            entries[index].deadline = tightened(entries[index].deadline)
        } else {
            return false
        }
        tick()
        return true
    }

    /// Output latency of the session plus two monitor periods and 20 ms: stopping at
    /// `deadline - guardNs()` keeps every presented frame before the deadline.
    private func guardNs() -> UInt64 {
        let session = AVAudioSession.sharedInstance()
        let latency = max(session.outputLatency, 0) + max(session.ioBufferDuration, 0)
        return UInt64(latency * 1_000_000_000) + 2 * Self.monitorInterval + 20_000_000
    }

    private func latencyNs() -> UInt64 {
        let session = AVAudioSession.sharedInstance()
        return UInt64((max(session.outputLatency, 0) + max(session.ioBufferDuration, 0)) * 1_000_000_000)
    }

    /// Runs the monitor while something plays or could start playing.
    private func updateMonitor() {
        let needed = announcement != nil || (playing && !entries.isEmpty)
            || queueOutput?.audible == true || announcementOutput?.audible == true
        if needed && monitor == nil {
            let timer = DispatchSource.makeTimerSource(queue: .main)
            timer.schedule(deadline: .now(), repeating: .nanoseconds(Int(Self.monitorInterval)), leeway: .milliseconds(5))
            timer.setEventHandler { [weak self] in self?.tick() }
            monitor = timer
            timer.resume()
        } else if !needed, let timer = monitor {
            monitor = nil
            timer.cancel()
        }
    }

    private func tick() {
        enforceDeadlines()
        updateOutputs()
        updateMonitor()
    }

    private func enforceDeadlines() {
        let limit = Self.now() &+ guardNs()
        if let deadline = announcement?.deadline, limit >= deadline {
            endAnnouncement(completed: false)
        }
        var index = 1
        while index < entries.count {
            if let deadline = entries[index].deadline, limit >= deadline {
                let entry = entries.remove(at: index)
                forget(entry.item)
                queue.remove(entry.item)
                emit(["type": "itemEnded", "id": entry.id, "reason": "expired"])
            } else {
                index += 1
            }
        }
        while let deadline = entries.first?.deadline, limit >= deadline {
            endCurrent("expired")
        }
    }

    /// Reports `outputStarted` once the position of an audible item advances and `outputEnded`
    /// as soon as it stops reaching the output.
    private func updateOutputs() {
        let now = Self.now()
        let latency = latencyNs()
        let pausedByAnnouncement = announcement.map { !$0.duck } ?? false
        let queueTarget = playing && !pausedByAnnouncement ? entries.first.map { ($0.id, $0.item) } : nil
        queueOutput = update(
            queueOutput, target: queueTarget, player: queue,
            running: queue.timeControlStatus == .playing && queue.currentItem === queueTarget?.1,
            now: now, latency: latency
        )
        let announcementTarget = announcement.map { ($0.id, $0.item) }
        announcementOutput = update(
            announcementOutput, target: announcementTarget, player: announcement?.player,
            running: announcement?.player.timeControlStatus == .playing,
            now: now, latency: latency
        )
    }

    private func update(
        _ output: Output?, target: (String, AVPlayerItem)?, player: AVPlayer?,
        running: Bool, now: UInt64, latency: UInt64
    ) -> Output? {
        var output = output
        let position = player.map { CMTimeGetSeconds($0.currentTime()) }.flatMap { $0.isFinite ? $0 : nil } ?? 0
        if let current = output, current.id != target?.0 || current.item !== target?.1 || (current.audible && !running) {
            if current.audible {
                emit(["type": "outputEnded", "id": current.id, "lastFrameNs": now &+ latency])
            }
            output = nil
        }
        guard let (id, item) = target else { return nil }
        var next = output ?? Output(id: id, item: item, silentAt: now, silentPosition: position)
        if !next.audible {
            if running && position > next.silentPosition + 0.001 {
                // Frames rendered after `silentAt` reach the output `latency` later.
                let earliest = next.silentAt
                let latest = now &+ latency
                let played = UInt64((position - next.silentPosition) * 1_000_000_000)
                let estimate = min(max(latest &- min(played, latest), earliest), latest)
                emit([
                    "type": "outputStarted", "id": id,
                    "firstFrameNs": estimate, "earliestNs": earliest, "latestNs": latest,
                ])
                next.audible = true
            } else {
                next.silentAt = now
                next.silentPosition = position
            }
        }
        return next
    }

    private static let timebase: mach_timebase_info_data_t = {
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        return info
    }()

    /// Nanoseconds of `mach_continuous_time` (the `output_clock_ns` clock in Rust).
    private static func now() -> UInt64 {
        let ticks = mach_continuous_time()
        let info = timebase
        guard info.denom != 0 else { return UInt64.max }
        let product = ticks.multipliedFullWidth(by: UInt64(info.numer))
        guard product.high < UInt64(info.denom) else { return UInt64.max }
        return UInt64(info.denom).dividingFullWidth(product).quotient
    }

    // MARK: - Helpers

    private func installObservers() {
        guard observers.isEmpty else { return }
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: .AVPlayerItemDidPlayToEndTime,
            object: nil,
            queue: .main
        ) { [weak self] note in
            guard let item = note.object as? AVPlayerItem else { return }
            self?.itemDidEnd(item)
        })
        observers.append(center.addObserver(
            forName: .AVPlayerItemFailedToPlayToEndTime,
            object: nil,
            queue: .main
        ) { [weak self] note in
            guard let item = note.object as? AVPlayerItem else { return }
            let error = note.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
            self?.fail(item, message: error.map(Bridge.message) ?? "failed to play the source")
        })
    }

    private func emit(_ payload: [String: Any?]) {
        Bridge.onMain { [weak self] in self?.sendEvent("event", Bridge.event(payload)) }
    }

    private static func ensureCategory() {
        let session = AVAudioSession.sharedInstance()
        if session.category != .playback && session.category != .playAndRecord {
            try? session.setCategory(.playback, mode: .default)
        }
    }

    /// `https://` URLs, `file://` URLs and absolute paths (validated in Rust as well).
    private static func url(_ uri: String) -> URL? {
        if uri.hasPrefix("/") { return URL(fileURLWithPath: uri) }
        guard let url = URL(string: uri), let scheme = url.scheme?.lowercased() else { return nil }
        return scheme == "https" || scheme == "file" ? url : nil
    }

    private struct InvalidArgument: LocalizedError {
        let errorDescription: String?
    }

    private static func items(_ json: [String: Any]) throws -> [(id: String, url: URL, deadline: UInt64?)] {
        guard let raw = json["items"] as? [[String: Any]] else {
            throw InvalidArgument(errorDescription: "items are missing")
        }
        return try raw.map { item in
            guard let id = item["id"] as? String, !id.isEmpty,
                  let uri = item["uri"] as? String, let url = url(uri)
            else { throw InvalidArgument(errorDescription: "invalid queue item") }
            return (id, url, (item["outputBeforeNs"] as? NSNumber)?.uint64Value)
        }
    }

    private static func checkUnique(_ ids: [String], existing: [String]) throws {
        var seen = Set(existing)
        for id in ids where !seen.insert(id).inserted {
            throw InvalidArgument(errorDescription: "duplicate queue item id: \(id)")
        }
    }
}
