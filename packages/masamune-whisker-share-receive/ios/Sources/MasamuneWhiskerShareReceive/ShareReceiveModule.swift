// Receives what the Share Extension (ios/ShareExtension/ShareViewController.swift) saved to the
// App Group: `share.json` and the images in `shared/`. The share is read and deleted when Rust asks
// for the initial share and whenever the app becomes active (sent as the "share" event). Images are
// moved into Library/Caches/masamune_whisker_share_receive so the App Group stays small.
//
// The App Group id comes from the `MasamuneShareReceiveAppGroup` Info.plist key set by the CNG
// plugin. Without it every read returns nothing.

import Foundation
import UIKit
import WhiskerModule

@WhiskerModule
public final class ShareReceiveModule: Module {
    /// Serializes reads so the initial read and an activation never return the same share twice.
    private static let queue = DispatchQueue(label: "net.mathru.whisker.share_receive")
    private var token: NSObjectProtocol?

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("ShareReceive")
            Events("share")

            OnStartObserving("share") { [weak self] in
                Bridge.onMain { self?.startObserving() }
            }
            OnStopObserving("share") { [weak self] in
                Bridge.onMain { self?.stopObserving() }
            }

            // getInitialShare() -> { text, imagePaths } | null. Consumes the pending share.
            AsyncFunction("getInitialShare") { _, promise in
                Self.queue.async {
                    Bridge.resolve(promise, Self.consume())
                }
            }
        }
    }

    private func startObserving() {
        guard token == nil else { return }
        token = NotificationCenter.default.addObserver(
            forName: UIApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.deliverPending()
        }
        // The app may already be active (the observer is added after launch).
        if UIApplication.shared.applicationState == .active {
            deliverPending()
        }
    }

    private func stopObserving() {
        if let token { NotificationCenter.default.removeObserver(token) }
        token = nil
    }

    private func deliverPending() {
        Self.queue.async { [weak self] in
            guard let payload = Self.consume() else { return }
            Bridge.onMain { self?.sendEvent("share", Bridge.event(payload)) }
        }
    }

    /// Reads and deletes the pending share. Runs on `queue`.
    private static func consume() -> [String: Any?]? {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "MasamuneShareReceiveAppGroup") as? String,
              let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
        else { return nil }
        let fileManager = FileManager.default
        let meta = container.appendingPathComponent("share.json")
        guard let data = try? Data(contentsOf: meta) else { return nil }
        try? fileManager.removeItem(at: meta)
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }

        var imagePaths: [String] = []
        if let cache = try? cacheDirectory() {
            for relative in object["images"] as? [String] ?? [] {
                // Only files inside the App Group's shared/ folder are accepted.
                guard relative.hasPrefix("shared/"), !relative.contains("..") else { continue }
                let source = container.appendingPathComponent(relative)
                let target = cache.appendingPathComponent(source.lastPathComponent)
                try? fileManager.removeItem(at: target)
                do {
                    try fileManager.moveItem(at: source, to: target)
                    imagePaths.append(target.path)
                } catch {
                    try? fileManager.removeItem(at: source)
                }
            }
        }
        let text = (object["text"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        guard text != nil || !imagePaths.isEmpty else { return nil }
        return ["text": text, "imagePaths": imagePaths]
    }

    private static func cacheDirectory() throws -> URL {
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let directory = caches.appendingPathComponent("masamune_whisker_share_receive", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}
