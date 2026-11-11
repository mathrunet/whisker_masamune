// Container that renders the screen, including its children, as a PNG image. The capture({ fileName, heightDp? })
// command saves it to the caches directory and emits a captured event ({ path } or { error }).

import UIKit
import WhiskerModule

@WhiskerModule
public final class ViewCaptureModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("ViewCapture")

            View("masamune-whisker-view-capture:Capture", CaptureView.self) {
                Command("capture") { (view: CaptureView, value: WhiskerValue) in
                    view.capture(value)
                }
                Events("captured")
            }
        }
    }
}

public final class CaptureView: WhiskerUI<UIView> {
    public override func createView() -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        view.isUserInteractionEnabled = false
        return view
    }

    public required init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        // Only the children (in the Host wrapper) are interactive.
        isUserInteractionEnabled = false
    }

    public required init?(coder: NSCoder) { nil }

    func capture(_ params: WhiskerValue) {
        let map: [String: WhiskerValue]
        if case .map(let value) = params { map = value } else { map = [:] }
        let fileName = map["fileName"]?.asString ?? "capture.png"
        let requestedHeightDp = map["heightDp"]?.asDouble
        // Capture on the next main loop turn so pending layout is applied first.
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            do {
                let path = try self.render(fileName: fileName, heightDp: requestedHeightDp)
                self.emitWhiskerEvent("captured", detail: .map(["path": .string(path)]))
            } catch {
                self.emitWhiskerEvent("captured", detail: .map(["error": .string(Bridge.message(error))]))
            }
        }
    }

    private func render(fileName: String, heightDp: Double?) throws -> String {
        // Whisker places the background and the children in the Host wrapper view around this
        // element (its superview); rendering this view alone would capture an empty view.
        guard let target = superview, let window = target.window else {
            throw CaptureError("capture host is not attached")
        }
        target.layoutIfNeeded()
        let bounds = target.bounds
        guard bounds.width > 0, bounds.height > 0 else { throw CaptureError("capture host is not laid out") }

        let scale = window.screen.scale
        let fullHeight = Int((bounds.height * scale).rounded())
        let cropHeight = heightDp.map { min(max(Int($0 * Double(scale)), 1), fullHeight) } ?? fullHeight
        let format = UIGraphicsImageRendererFormat.preferred()
        format.scale = scale
        format.opaque = false
        let size = CGSize(width: bounds.width, height: CGFloat(cropHeight) / scale)
        let renderer = UIGraphicsImageRenderer(size: size, format: format)
        // On-screen content is snapshotted as composited (blur, video). Content placed outside the
        // window (for example an off-screen share card) is not part of the snapshot, so its layer tree
        // is rendered directly instead.
        let visible = window.bounds.contains(target.convert(bounds, to: window))
        let data = renderer.pngData { context in
            if !visible || !target.drawHierarchy(in: bounds, afterScreenUpdates: true) {
                Self.displayLayers(target.layer)
                target.layer.render(in: context.cgContext)
            }
        }

        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let file = try captureFileURL(cache: caches, fileName: fileName)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        do {
            try data.write(to: file, options: .atomic)
        } catch {
            throw CaptureError("capture PNG could not be saved")
        }
        return file.path
    }

    /// Draws layers that have not been displayed yet (`render(in:)` uses their current contents).
    private static func displayLayers(_ layer: CALayer) {
        layer.displayIfNeeded()
        layer.sublayers?.forEach(displayLayers)
    }
}

private struct CaptureError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}
