// Share Extension of masamune-whisker-share-receive, staged into the iOS project by its CNG plugin.
//
// It shows no UI: the shared text, URLs and images are written to the App Group container
// (images to `shared/`, the description to `share.json`) and the request completes at once. The
// app reads and deletes them when it starts or becomes active (ShareReceiveModule.swift). The App
// Group id comes from the `MasamuneShareReceiveAppGroup` Info.plist key.

import UIKit
import UniformTypeIdentifiers

final class ShareViewController: UIViewController {
    private var started = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !started else { return }
        started = true
        collect { [weak self] texts, images in
            guard let self else { return }
            do {
                try Self.save(texts: texts, images: images)
                self.extensionContext?.completeRequest(returningItems: [], completionHandler: nil)
            } catch {
                self.extensionContext?.cancelRequest(withError: error)
            }
        }
    }

    // MARK: - Reading the shared items

    private enum Loaded {
        case text(String)
        case image(Data, String)
    }

    /// Loads every attachment, keeping their order. Calls `completion` on the main thread.
    private func collect(_ completion: @escaping ([String], [(Data, String)]) -> Void) {
        let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? [])
            .flatMap { $0.attachments ?? [] }
        var results = [Loaded?](repeating: nil, count: providers.count)
        let group = DispatchGroup()
        let lock = NSLock()
        for (index, provider) in providers.enumerated() {
            group.enter()
            Self.load(provider) { value in
                lock.lock()
                results[index] = value
                lock.unlock()
                group.leave()
            }
        }
        group.notify(queue: .main) {
            var texts: [String] = []
            var images: [(Data, String)] = []
            for case let value? in results {
                switch value {
                case .text(let text): texts.append(text)
                case .image(let data, let ext): images.append((data, ext))
                }
            }
            completion(texts, images)
        }
    }

    private static func load(_ provider: NSItemProvider, _ done: @escaping (Loaded?) -> Void) {
        if provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
            provider.loadItem(forTypeIdentifier: UTType.image.identifier, options: nil) { item, _ in
                done(image(item, provider))
            }
        } else if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
            provider.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { item, _ in
                switch item {
                case let url as URL: done(.text(url.absoluteString))
                case let text as String: done(.text(text))
                default: done(nil)
                }
            }
        } else if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) {
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier, options: nil) { item, _ in
                switch item {
                case let text as String: done(.text(text))
                case let data as Data: done(String(data: data, encoding: .utf8).map(Loaded.text))
                default: done(nil)
                }
            }
        } else {
            done(nil)
        }
    }

    /// Image bytes and a file extension from a file URL, raw data or a `UIImage`.
    private static func image(_ item: NSSecureCoding?, _ provider: NSItemProvider) -> Loaded? {
        let declared = provider.registeredTypeIdentifiers
            .compactMap { UTType($0) }
            .first { $0.conforms(to: .image) }
        switch item {
        case let url as URL:
            guard let data = try? Data(contentsOf: url) else { return nil }
            let ext = url.pathExtension.isEmpty ? (declared?.preferredFilenameExtension ?? "jpg") : url.pathExtension
            return .image(data, ext)
        case let data as Data:
            return .image(data, declared?.preferredFilenameExtension ?? "jpg")
        case let image as UIImage:
            guard let data = image.pngData() else { return nil }
            return .image(data, "png")
        default:
            return nil
        }
    }

    // MARK: - Writing to the App Group

    private static func save(texts: [String], images: [(Data, String)]) throws {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "MasamuneShareReceiveAppGroup") as? String,
              let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
        else { throw ShareExtensionError("App Group is not configured") }
        let fileManager = FileManager.default
        let meta = container.appendingPathComponent("share.json")
        let shared = container.appendingPathComponent("shared", isDirectory: true)
        // A share the app has not read yet is replaced by this one.
        if let data = try? Data(contentsOf: meta),
           let old = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            for path in old["images"] as? [String] ?? [] {
                try? fileManager.removeItem(at: container.appendingPathComponent(path))
            }
            try? fileManager.removeItem(at: meta)
        }
        try fileManager.createDirectory(at: shared, withIntermediateDirectories: true)
        var paths: [String] = []
        for (index, image) in images.enumerated() {
            let name = "\(UUID().uuidString)_\(index).\(image.1.lowercased())"
            try image.0.write(to: shared.appendingPathComponent(name), options: .atomic)
            paths.append("shared/\(name)")
        }
        let text = texts.filter { !$0.isEmpty }.joined(separator: "\n")
        guard !text.isEmpty || !paths.isEmpty else { return }
        var object: [String: Any] = [
            "images": paths,
            "createdAt": Int64(Date().timeIntervalSince1970 * 1000),
        ]
        if !text.isEmpty { object["text"] = text }
        // Written last and atomically: the app only sees complete shares.
        let data = try JSONSerialization.data(withJSONObject: object)
        try data.write(to: meta, options: .atomic)
    }
}

private struct ShareExtensionError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}
