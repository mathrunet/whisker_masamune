// Audio session, Now Playing info and remote commands (iOS).
//
// - configure: AVAudioSession category `.playback` (mode `.default`), with `.mixWithOthers` or
//   `.duckOthers` when asked. Players such as whisker-audio set `.playback` again before playing,
//   which keeps the same category.
// - Events: `interruption` (AVAudioSession.interruptionNotification), `routeChange`
//   (AVAudioSession.routeChangeNotification) and `command` (MPRemoteCommandCenter). The
//   notification observers stay installed for the life of the module; events are only sent while
//   Rust subscribes. Remote command targets are added by `setControls` / `setNowPlaying` and
//   removed by `clearNowPlaying`.
// - Now Playing: MPNowPlayingInfoCenter. The elapsed time and rate are written together, so the
//   system extrapolates the progress between `setPlaybackState` calls.

import AVFoundation
import Foundation
import MediaPlayer
import UIKit
import WhiskerModule

@WhiskerModule
public final class AudioSessionModule: Module {
    private var observers: [NSObjectProtocol] = []
    private var commandTargets: [(MPRemoteCommand, Any)] = []
    private var controls: [String: Any] = [:]
    private var info: [String: Any] = [:]
    private var artworkPath: String?

    public override func definition() -> ModuleDefinition {
        installObservers()
        return ModuleDefinition {
            Name("AudioSession")
            Events("interruption", "routeChange", "command", "notificationTap")

            // configure({ mixWithOthers, duckOthers, ... }) — the Android channel keys are ignored.
            Function("configure") { args in
                let json = Bridge.args(args)
                var options: AVAudioSession.CategoryOptions = []
                if json["mixWithOthers"] as? Bool == true {
                    options.insert(.mixWithOthers)
                } else if json["duckOthers"] as? Bool == true {
                    options.insert(.duckOthers)
                }
                do {
                    try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default, options: options)
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // setActive({ active }) -> whether the session is active
            Function("setActive") { [weak self] args in
                let active = Bridge.args(args)["active"] as? Bool ?? false
                do {
                    try AVAudioSession.sharedInstance().setActive(
                        active,
                        options: active ? [] : [.notifyOthersOnDeactivation]
                    )
                    if active { self?.installCommands() }
                    return Bridge.ok(active)
                } catch {
                    return Bridge.err(error)
                }
            }

            // setNowPlaying({ id, title, artist, album, artworkPath, durationMs })
            Function("setNowPlaying") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                self.setNowPlaying(Bridge.args(args))
                return Bridge.ok(true)
            }

            Function("clearNowPlaying") { [weak self] _ in
                guard let self else { return Bridge.ok(false) }
                self.info = [:]
                self.artworkPath = nil
                MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
                self.removeCommands()
                return Bridge.ok(true)
            }

            // setPlaybackState({ playing, state, positionMs, speed })
            Function("setPlaybackState") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                let json = Bridge.args(args)
                let playing = json["playing"] as? Bool ?? false
                let position = ((json["positionMs"] as? NSNumber)?.doubleValue ?? 0) / 1000
                let speed = (json["speed"] as? NSNumber)?.doubleValue ?? 1
                self.info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = position
                self.info[MPNowPlayingInfoPropertyPlaybackRate] = playing ? speed : 0
                self.info[MPNowPlayingInfoPropertyDefaultPlaybackRate] = 1.0
                self.publish()
                MPNowPlayingInfoCenter.default().playbackState = playing ? .playing : .paused
                return Bridge.ok(true)
            }

            // setControls({ previous, next, seek, custom: [{ id, label, iosCommand, active }] })
            Function("setControls") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                self.controls = Bridge.args(args)
                self.installCommands()
                return Bridge.ok(true)
            }
        }
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    // MARK: - Session notifications

    private func installObservers() {
        let center = NotificationCenter.default
        let session = AVAudioSession.sharedInstance()
        observers.append(center.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: session,
            queue: .main
        ) { [weak self] note in
            self?.handleInterruption(note)
        })
        observers.append(center.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: session,
            queue: .main
        ) { [weak self] note in
            self?.handleRouteChange(note)
        })
    }

    private func handleInterruption(_ note: Notification) {
        guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: raw)
        else { return }
        switch type {
        case .began:
            // iOS 14.5+: an interruption that only marks a suspended app does not stop playback.
            if (note.userInfo?[AVAudioSessionInterruptionReasonKey] as? UInt)
                == AVAudioSession.InterruptionReason.appWasSuspended.rawValue {
                return
            }
            send("interruption", ["begin": true, "kind": "pause", "shouldResume": false])
        case .ended:
            let options = AVAudioSession.InterruptionOptions(
                rawValue: note.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0
            )
            send("interruption", ["begin": false, "kind": "pause", "shouldResume": options.contains(.shouldResume)])
        @unknown default:
            break
        }
    }

    private func handleRouteChange(_ note: Notification) {
        guard let raw = note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
              let reason = AVAudioSession.RouteChangeReason(rawValue: raw)
        else { return }
        let name: String
        switch reason {
        case .newDeviceAvailable: name = "newDeviceAvailable"
        case .oldDeviceUnavailable: name = "oldDeviceUnavailable"
        default: name = "other"
        }
        let outputs = AVAudioSession.sharedInstance().currentRoute.outputs.map { $0.portType.rawValue }
        send("routeChange", ["reason": name, "outputs": outputs])
    }

    private func send(_ event: String, _ payload: [String: Any?]) {
        Bridge.onMain { [weak self] in self?.sendEvent(event, Bridge.event(payload)) }
    }

    // MARK: - Now Playing

    private func setNowPlaying(_ json: [String: Any]) {
        var next: [String: Any] = [
            MPMediaItemPropertyTitle: json["title"] as? String ?? "",
            MPMediaItemPropertyArtist: json["artist"] as? String ?? "",
            MPMediaItemPropertyAlbumTitle: json["album"] as? String ?? "",
            MPNowPlayingInfoPropertyMediaType: MPNowPlayingInfoMediaType.audio.rawValue,
        ]
        if let id = json["id"] as? String, !id.isEmpty {
            next[MPNowPlayingInfoPropertyExternalContentIdentifier] = id
        }
        if let duration = (json["durationMs"] as? NSNumber)?.doubleValue {
            next[MPMediaItemPropertyPlaybackDuration] = duration / 1000
        }
        // Keep the progress of the current item across metadata updates.
        for key in [MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate,
                    MPNowPlayingInfoPropertyDefaultPlaybackRate] {
            if let value = info[key] { next[key] = value }
        }
        let path = (json["artworkPath"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        if path == artworkPath, let artwork = info[MPMediaItemPropertyArtwork] {
            next[MPMediaItemPropertyArtwork] = artwork
        } else if let path, let artwork = Self.artwork(path) {
            next[MPMediaItemPropertyArtwork] = artwork
        }
        artworkPath = path
        info = next
        installCommands()
        publish()
    }

    private func publish() {
        guard !info.isEmpty else { return }
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    private static func artwork(_ path: String) -> MPMediaItemArtwork? {
        guard let source = UIImage(contentsOfFile: path) else { return nil }
        let image = downscaled(source, maxSide: 512)
        return MPMediaItemArtwork(boundsSize: image.size) { _ in image }
    }

    private static func downscaled(_ image: UIImage, maxSide: CGFloat) -> UIImage {
        let longest = max(image.size.width, image.size.height)
        guard longest > maxSide else { return image }
        let scale = maxSide / longest
        let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }

    // MARK: - Remote commands

    private func installCommands() {
        removeCommands()
        let center = MPRemoteCommandCenter.shared()
        add(center.playCommand) { $0.command("play") }
        add(center.pauseCommand) { $0.command("pause") }
        add(center.togglePlayPauseCommand) { $0.command("togglePlayPause") }
        add(center.stopCommand) { $0.command("stop") }
        let previous = controls["previous"] as? Bool ?? true
        let next = controls["next"] as? Bool ?? true
        let seek = controls["seek"] as? Bool ?? true
        center.previousTrackCommand.isEnabled = previous
        center.nextTrackCommand.isEnabled = next
        center.changePlaybackPositionCommand.isEnabled = seek
        if previous { add(center.previousTrackCommand) { $0.command("previous") } }
        if next { add(center.nextTrackCommand) { $0.command("next") } }
        if seek {
            let target = center.changePlaybackPositionCommand.addTarget { [weak self] event in
                guard let self, let event = event as? MPChangePlaybackPositionCommandEvent else {
                    return .commandFailed
                }
                self.send("command", ["command": "seek", "positionMs": Int64(max(0, event.positionTime) * 1000)])
                return .success
            }
            commandTargets.append((center.changePlaybackPositionCommand, target))
        }
        for command in [center.likeCommand, center.dislikeCommand, center.bookmarkCommand] {
            command.isEnabled = false
        }
        for action in controls["custom"] as? [[String: Any]] ?? [] {
            guard let id = action["id"] as? String else { continue }
            let command: MPFeedbackCommand
            switch action["iosCommand"] as? String {
            case "like": command = center.likeCommand
            case "dislike": command = center.dislikeCommand
            case "bookmark": command = center.bookmarkCommand
            default: continue
            }
            command.isEnabled = true
            command.isActive = action["active"] as? Bool ?? false
            command.localizedTitle = action["label"] as? String ?? ""
            let target = command.addTarget { [weak self] _ in
                self?.send("command", ["command": "custom", "id": id])
                return .success
            }
            commandTargets.append((command, target))
        }
    }

    private func add(_ command: MPRemoteCommand, _ make: (AudioSessionModule) -> [String: Any?]) {
        let payload = make(self)
        command.isEnabled = true
        let target = command.addTarget { [weak self] _ in
            self?.send("command", payload)
            return .success
        }
        commandTargets.append((command, target))
    }

    private func command(_ name: String) -> [String: Any?] {
        ["command": name]
    }

    private func removeCommands() {
        for (command, target) in commandTargets {
            command.removeTarget(target)
        }
        commandTargets.removeAll()
    }
}
