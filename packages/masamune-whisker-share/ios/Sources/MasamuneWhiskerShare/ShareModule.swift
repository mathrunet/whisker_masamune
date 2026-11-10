// Shares files and text via the OS share sheet (UIActivityViewController).
//
// The subject becomes the share sheet title and the e-mail subject. On iPad the sheet is a popover
// anchored to the center of the key window.

import LinkPresentation
import UIKit
import UniformTypeIdentifiers
import WhiskerModule

@WhiskerModule
public final class ShareModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Share")

            // shareFile({ path, mimeType, subject, text }) — opens the OS share sheet.
            Function("shareFile") { args in
                let json = Bridge.args(args)
                guard let path = json["path"] as? String else { return Bridge.err("No value for path") }
                let subject = Self.nonBlank(json["subject"])
                do {
                    let file = try Self.shareableFile(path, mimeType: json["mimeType"] as? String)
                    var items: [Any] = [ShareItem(file, subject: subject)]
                    if let text = Self.nonBlank(json["text"]) {
                        items.append(ShareItem(text, subject: subject))
                    }
                    try Self.present(items)
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // shareText({ text, subject }) — shares text only.
            Function("shareText") { args in
                let json = Bridge.args(args)
                let text = json["text"] as? String ?? ""
                do {
                    try Self.present([ShareItem(text, subject: Self.nonBlank(json["subject"]))])
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }
        }
    }

    // MARK: - Helpers

    private static func nonBlank(_ value: Any?) -> String? {
        guard let text = value as? String, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return text
    }

    /// The file to share. A file without an extension is copied into this module's temporary
    /// directory with the extension of `mimeType`, so receiving apps recognise its type.
    private static func shareableFile(_ path: String, mimeType: String?) throws -> URL {
        let raw = path.hasPrefix("file://") ? String(path.dropFirst("file://".count)) : path
        // Paths are passed as is; a percent-encoded `file://` URL is accepted too.
        let candidates = [raw, raw.removingPercentEncoding].compactMap { $0 }
        guard let found = candidates.first(where: { FileManager.default.fileExists(atPath: $0) }) else {
            throw ShareError("\(raw): no such file")
        }
        let source = URL(fileURLWithPath: found)
        guard source.pathExtension.isEmpty,
              let mimeType,
              let ext = UTType(mimeType: mimeType)?.preferredFilenameExtension
        else { return source }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("masamune_whisker_share", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let target = directory.appendingPathComponent(source.lastPathComponent).appendingPathExtension(ext)
        if FileManager.default.fileExists(atPath: target.path) {
            try FileManager.default.removeItem(at: target)
        }
        try FileManager.default.copyItem(at: source, to: target)
        return target
    }

    private static func present(_ items: [Any]) throws {
        guard let window = keyWindow(), var presenter = window.rootViewController else {
            throw ShareError("no window")
        }
        while let presented = presenter.presentedViewController, !presented.isBeingDismissed {
            presenter = presented
        }
        let controller = UIActivityViewController(activityItems: items, applicationActivities: nil)
        if let popover = controller.popoverPresentationController {
            popover.sourceView = window
            popover.sourceRect = CGRect(x: window.bounds.midX, y: window.bounds.midY, width: 0, height: 0)
            popover.permittedArrowDirections = []
        }
        presenter.present(controller, animated: true)
    }

    private static func keyWindow() -> UIWindow? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let ordered = scenes.filter { $0.activationState == .foregroundActive } + scenes
        for scene in ordered {
            if let window = scene.windows.first(where: \.isKeyWindow) ?? scene.windows.first {
                return window
            }
        }
        return nil
    }
}

/// A shared value (file URL or text) that also provides the subject and the share sheet title.
private final class ShareItem: NSObject, UIActivityItemSource {
    private let value: Any
    private let subject: String?

    init(_ value: Any, subject: String?) {
        self.value = value
        self.subject = subject
    }

    func activityViewControllerPlaceholderItem(_ activityViewController: UIActivityViewController) -> Any {
        value
    }

    func activityViewController(_ activityViewController: UIActivityViewController, itemForActivityType activityType: UIActivity.ActivityType?) -> Any? {
        value
    }

    func activityViewController(_ activityViewController: UIActivityViewController, subjectForActivityType activityType: UIActivity.ActivityType?) -> String {
        subject ?? ""
    }

    func activityViewControllerLinkMetadata(_ activityViewController: UIActivityViewController) -> LPLinkMetadata? {
        let metadata = LPLinkMetadata()
        if let url = value as? URL {
            metadata.originalURL = url
            metadata.url = url
            metadata.title = subject ?? url.lastPathComponent
        } else if let text = value as? String {
            metadata.title = subject ?? text
        }
        return metadata
    }
}

private struct ShareError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}
