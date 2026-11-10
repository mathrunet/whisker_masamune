// Image picking (photo library or camera). Applies the EXIF orientation, scales the long side down to
// the given size and saves the image as a PNG in the caches directory, like the Android module.
//
// The library uses PHPickerViewController (no photo library permission needed); the camera uses
// UIImagePickerController and requires `NSCameraUsageDescription` (see the crate's CNG plugin).

import AVFoundation
import ImageIO
import PhotosUI
import UIKit
import UniformTypeIdentifiers
import WhiskerModule

@WhiskerModule
public final class ImagePickerModule: Module {
    /// Sessions waiting for the user (the pickers hold their delegates weakly).
    private var sessions: [ObjectIdentifier: PickSession] = [:]

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("ImagePicker")

            // pickImage({ source: "gallery" | "camera", maxSize }) -> { path, width, height } | null (cancelled)
            AsyncFunction("pickImage") { [weak self] args, promise in
                let json = Bridge.args(args)
                let maxSize = (json["maxSize"] as? NSNumber)?.intValue ?? 512
                let source = json["source"] as? String ?? "gallery"
                Bridge.onMain {
                    guard let self else {
                        Bridge.reject(promise, "ImagePicker module released")
                        return
                    }
                    self.start(camera: source == "camera", maxSize: maxSize, promise: promise)
                }
            }
        }
    }

    private func start(camera: Bool, maxSize: Int, promise: WhiskerPromise) {
        guard let presenter = Self.presenter() else {
            Bridge.reject(promise, "no window")
            return
        }
        let session = PickSession(maxSize: maxSize, promise: promise)
        let id = ObjectIdentifier(session)
        session.onFinish = { [weak self] in self?.sessions[id] = nil }
        if camera {
            guard UIImagePickerController.isSourceTypeAvailable(.camera) else {
                Bridge.reject(promise, "camera is not available")
                return
            }
            Self.requestCameraAccess { granted in
                guard granted else {
                    Bridge.reject(promise, "camera permission denied")
                    return
                }
                self.sessions[id] = session
                session.presentCamera(from: presenter)
            }
        } else {
            sessions[id] = session
            session.presentLibrary(from: presenter)
        }
    }

    private static func requestCameraAccess(_ completion: @escaping (Bool) -> Void) {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            completion(true)
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { granted in
                Bridge.onMain { completion(granted) }
            }
        default:
            completion(false)
        }
    }

    /// Top-most view controller of the key window.
    private static func presenter() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let ordered = scenes.filter { $0.activationState == .foregroundActive } + scenes
        let window = ordered.lazy.compactMap { $0.windows.first(where: \.isKeyWindow) ?? $0.windows.first }.first
        guard var top = window?.rootViewController else { return nil }
        while let presented = top.presentedViewController, !presented.isBeingDismissed {
            top = presented
        }
        return top
    }
}

/// One pick: presents the picker, then processes the chosen image off the main thread.
private final class PickSession: NSObject, PHPickerViewControllerDelegate, UIImagePickerControllerDelegate,
    UINavigationControllerDelegate, UIAdaptivePresentationControllerDelegate {
    private static let worker = DispatchQueue(label: "masamune-whisker-image-picker")

    private let maxSize: Int
    private let promise: WhiskerPromise
    var onFinish: (() -> Void)?

    init(maxSize: Int, promise: WhiskerPromise) {
        self.maxSize = maxSize
        self.promise = promise
    }

    func presentLibrary(from presenter: UIViewController) {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        configuration.selectionLimit = 1
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = self
        picker.presentationController?.delegate = self
        presenter.present(picker, animated: true)
    }

    func presentCamera(from presenter: UIViewController) {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.mediaTypes = [UTType.image.identifier]
        picker.delegate = self
        presenter.present(picker, animated: true)
    }

    // MARK: PHPickerViewControllerDelegate

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        picker.dismiss(animated: true)
        guard let provider = results.first?.itemProvider else {
            finish(nil)
            return
        }
        guard provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) else {
            finish(error: "File not found.")
            return
        }
        provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) { [self] data, error in
            guard let data else {
                finish(error: error.map(Bridge.message) ?? "File not found.")
                return
            }
            Self.worker.async { [self] in
                process { try ImageProcessing.downscale(data: data, maxSize: maxSize) }
            }
        }
    }

    // MARK: UIImagePickerControllerDelegate

    func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
        picker.dismiss(animated: true)
        guard let image = info[.originalImage] as? UIImage else {
            finish(error: "File not found.")
            return
        }
        Self.worker.async { [self] in
            process { try ImageProcessing.downscale(image: image, maxSize: maxSize) }
        }
    }

    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        picker.dismiss(animated: true)
        finish(nil)
    }

    // MARK: UIAdaptivePresentationControllerDelegate

    /// The library sheet was swiped away.
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        finish(nil)
    }

    // MARK: Results

    private func process(_ work: () throws -> CGImage) {
        do {
            let image = try work()
            let path = try ImageProcessing.savePNG(image)
            finish(["path": path, "width": image.width, "height": image.height])
        } catch {
            finish(error: Bridge.message(error))
        }
    }

    private func finish(_ value: Any?) {
        Bridge.resolve(promise, value)
        done()
    }

    private func finish(error message: String) {
        Bridge.reject(promise, message)
        done()
    }

    private func done() {
        Bridge.onMain { [self] in
            onFinish?()
            onFinish = nil
        }
    }
}

enum ImageProcessing {
    /// Decodes picked image data with its EXIF orientation applied and the long side at most `maxSize`.
    static func downscale(data: Data, maxSize: Int) throws -> CGImage {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let pixelWidth = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.intValue,
              let pixelHeight = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.intValue
        else { throw PickerError("File not found.") }
        // EXIF orientations 5-8 swap the width and the height.
        let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue ?? 1
        let (width, height) = orientation >= 5 ? (pixelHeight, pixelWidth) : (pixelWidth, pixelHeight)
        let target = targetSize(width: width, height: height, maxSize: maxSize)
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(target.width, target.height),
        ]
        guard let decoded = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else {
            throw PickerError("cannot decode the image")
        }
        if decoded.width == target.width && decoded.height == target.height { return decoded }
        return try redraw(target) { context in
            context.draw(decoded, in: CGRect(x: 0, y: 0, width: target.width, height: target.height))
        }
    }

    /// Renders a camera photo upright with the long side at most `maxSize`.
    static func downscale(image: UIImage, maxSize: Int) throws -> CGImage {
        let width = Int((image.size.width * image.scale).rounded())
        let height = Int((image.size.height * image.scale).rounded())
        let target = targetSize(width: width, height: height, maxSize: maxSize)
        return try redraw(target) { context in
            // UIKit drawing applies `imageOrientation`; flip the bitmap context to UIKit coordinates.
            context.translateBy(x: 0, y: CGFloat(target.height))
            context.scaleBy(x: 1, y: -1)
            UIGraphicsPushContext(context)
            image.draw(in: CGRect(x: 0, y: 0, width: target.width, height: target.height))
            UIGraphicsPopContext()
        }
    }

    /// Same arithmetic as Android: `ratio = maxSize / longest`, sides truncated, at least 1.
    private static func targetSize(width: Int, height: Int, maxSize: Int) -> (width: Int, height: Int) {
        let longest = max(width, height)
        guard longest > maxSize, longest > 0 else { return (max(width, 1), max(height, 1)) }
        let ratio = Float(maxSize) / Float(longest)
        return (max(Int(Float(width) * ratio), 1), max(Int(Float(height) * ratio), 1))
    }

    private static func redraw(_ size: (width: Int, height: Int), _ draw: (CGContext) -> Void) throws -> CGImage {
        guard let context = CGContext(
            data: nil,
            width: size.width,
            height: size.height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { throw PickerError("cannot allocate the image") }
        context.interpolationQuality = .high
        draw(context)
        guard let image = context.makeImage() else { throw PickerError("cannot render the image") }
        return image
    }

    /// Writes a PNG into `Caches/masamune_whisker_image_picker/` and returns its absolute path.
    static func savePNG(_ image: CGImage) throws -> String {
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let directory = caches.appendingPathComponent("masamune_whisker_image_picker", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let millis = Int64(Date().timeIntervalSince1970 * 1000)
        let file = directory.appendingPathComponent("image_\(millis).png")
        guard let destination = CGImageDestinationCreateWithURL(file as CFURL, UTType.png.identifier as CFString, 1, nil) else {
            throw PickerError("cannot write \(file.path)")
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { throw PickerError("cannot write \(file.path)") }
        return file.path
    }
}

private struct PickerError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}
