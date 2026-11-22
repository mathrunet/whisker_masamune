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
// - Everything runs on the main thread; KVO callbacks are moved there.

import AVFoundation
import Foundation
import WhiskerModule

@WhiskerModule
public final class AudioPlaylistModule: Module {
    private struct Entry {
        let id: String
        let item: AVPlayerItem
    }

    private struct Announcement {
        let id: String
        let duck: Bool
        let duckVolume: Float
        let player: AVPlayer
        let item: AVPlayerItem
        var observation: NSKeyValueObservation?
    }

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
                self.interrupt(id: id, url: url, duck: duck, duckVolume: duckVolume, volume: volume)
                return Bridge.ok(true)
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

    private func setQueue(_ items: [(id: String, url: URL)]) {
        if let current = entries.first {
            emit(["type": "itemEnded", "id": current.id, "reason": "removed"])
        }
        entries.forEach { forget($0.item) }
        queue.removeAllItems()
        entries = []
        for (id, url) in items {
            enqueue(id: id, url: url)
        }
        if !entries.isEmpty {
            startCurrent()
        }
        applyPlayback()
    }

    private func append(_ items: [(id: String, url: URL)]) {
        let wasEmpty = entries.isEmpty
        for (id, url) in items {
            enqueue(id: id, url: url)
        }
        if wasEmpty && !entries.isEmpty {
            startCurrent()
        }
        applyPlayback()
    }

    private func enqueue(id: String, url: URL) {
        let item = AVPlayerItem(url: url)
        let key = ObjectIdentifier(item)
        itemObservations[key] = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard item.status == .failed else { return }
            let message = item.error.map(Bridge.message) ?? "failed to load the source"
            Bridge.onMain { self?.fail(item, message: message) }
        }
        queue.insert(item, after: nil)
        entries.append(Entry(id: id, item: item))
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
        let pausedByAnnouncement = announcement.map { !$0.duck } ?? false
        if playing && !pausedByAnnouncement && !entries.isEmpty {
            Self.ensureCategory()
            queue.play()
        } else {
            queue.pause()
        }
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

    private func interrupt(id: String, url: URL, duck: Bool, duckVolume: Float, volume: Float) {
        endAnnouncement(completed: false, resume: false)
        let item = AVPlayerItem(url: url)
        let player = AVPlayer(playerItem: item)
        player.volume = min(max(volume, 0), 1)
        var next = Announcement(id: id, duck: duck, duckVolume: min(max(duckVolume, 0), 1), player: player, item: item)
        next.observation = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard item.status == .failed else { return }
            let message = item.error.map(Bridge.message) ?? "failed to load the source"
            Bridge.onMain { self?.fail(item, message: message) }
        }
        announcement = next
        applyVolume()
        applyPlayback()
        emit(["type": "interruptStarted", "id": id])
        Self.ensureCategory()
        player.play()
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
        emit(["type": "interruptEnded", "id": current.id, "completed": completed])
        return true
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

    private static func items(_ json: [String: Any]) throws -> [(id: String, url: URL)] {
        guard let raw = json["items"] as? [[String: Any]] else {
            throw InvalidArgument(errorDescription: "items are missing")
        }
        return try raw.map { item in
            guard let id = item["id"] as? String, !id.isEmpty,
                  let uri = item["uri"] as? String, let url = url(uri)
            else { throw InvalidArgument(errorDescription: "invalid queue item") }
            return (id, url)
        }
    }

    private static func checkUnique(_ ids: [String], existing: [String]) throws {
        var seen = Set(existing)
        for id in ids where !seen.insert(id).inserted {
            throw InvalidArgument(errorDescription: "duplicate queue item id: \(id)")
        }
    }
}
