// Photo library access (iOS, Photos framework).
//
// - Assets are fetched with a creation-date predicate and paged with offset / limit. Locations come
//   from `PHAsset.location` (no extra cost).
// - Thumbnails and scaled images are requested from PHImageManager (network access allowed, so
//   iCloud photos download) and written as JPEG to Library/Caches/masamune_whisker_photo_library,
//   named after the asset, size and crop mode so repeated requests reuse the file.

import Foundation
import Photos
import PhotosUI
import UIKit
import WhiskerModule

@WhiskerModule
public final class PhotoLibraryModule: Module {
    private static let work = DispatchQueue(label: "net.mathru.whisker.photo_library", qos: .userInitiated)

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("PhotoLibrary")

            Function("status") { _ in
                Bridge.ok(Self.status(PHPhotoLibrary.authorizationStatus(for: .readWrite)))
            }

            AsyncFunction("requestPermission") { _, promise in
                PHPhotoLibrary.requestAuthorization(for: .readWrite) { status in
                    Bridge.resolve(promise, Self.status(status))
                }
            }

            AsyncFunction("presentLimitedPicker") { _, promise in
                Bridge.onMain {
                    guard PHPhotoLibrary.authorizationStatus(for: .readWrite) == .limited,
                          let controller = Self.topViewController()
                    else {
                        return Bridge.resolve(promise, Self.status(PHPhotoLibrary.authorizationStatus(for: .readWrite)))
                    }
                    PHPhotoLibrary.shared().presentLimitedLibraryPicker(from: controller) { _ in
                        Bridge.resolve(promise, Self.status(PHPhotoLibrary.authorizationStatus(for: .readWrite)))
                    }
                }
            }

            Function("openAppSettings") { _ in
                guard let url = URL(string: UIApplication.openSettingsURLString) else { return Bridge.ok(false) }
                Bridge.onMain { UIApplication.shared.open(url) }
                return Bridge.ok(true)
            }

            // openAsset({ id }) -> false. iOS has no public API to open a given asset in the Photos app.
            Function("openAsset") { _ in
                Bridge.ok(false)
            }

            // listAssets({ fromMs, toMs, kinds, newestFirst, offset, limit, includeLocation }) -> [asset]
            AsyncFunction("listAssets") { args, promise in
                let json = Bridge.args(args)
                Self.work.async {
                    Bridge.resolve(promise, Self.list(json))
                }
            }

            // thumbnail({ id, size, crop }) -> { path, width, height }
            AsyncFunction("thumbnail") { args, promise in
                let json = Bridge.args(args)
                guard let id = json["id"] as? String, let asset = Self.asset(id) else {
                    return Bridge.reject(promise, "asset not found")
                }
                let size = max(1, (json["size"] as? NSNumber)?.intValue ?? 256)
                let crop = json["crop"] as? Bool ?? true
                Self.render(asset, size: size, crop: crop, promise: promise)
            }

            // assetLocation({ id }) -> { latitude, longitude } | null
            AsyncFunction("assetLocation") { args, promise in
                let id = Bridge.args(args)["id"] as? String ?? ""
                guard let location = Self.asset(id)?.location else { return Bridge.resolve(promise, nil) }
                Bridge.resolve(promise, ["latitude": location.coordinate.latitude, "longitude": location.coordinate.longitude])
            }
        }
    }

    private static func status(_ status: PHAuthorizationStatus) -> String {
        switch status {
        case .notDetermined: return "notDetermined"
        case .restricted: return "restricted"
        case .denied: return "denied"
        case .authorized: return "granted"
        case .limited: return "limited"
        @unknown default: return "denied"
        }
    }

    private static func asset(_ id: String) -> PHAsset? {
        guard !id.isEmpty else { return nil }
        return PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject
    }

    private static func list(_ json: [String: Any]) -> [[String: Any?]] {
        let status = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        guard status == .authorized || status == .limited else { return [] }
        var predicates: [NSPredicate] = []
        if let from = (json["fromMs"] as? NSNumber)?.doubleValue {
            predicates.append(NSPredicate(format: "creationDate >= %@", Date(timeIntervalSince1970: from / 1000) as NSDate))
        }
        if let to = (json["toMs"] as? NSNumber)?.doubleValue {
            predicates.append(NSPredicate(format: "creationDate < %@", Date(timeIntervalSince1970: to / 1000) as NSDate))
        }
        let kinds = json["kinds"] as? [String] ?? ["image", "video"]
        var types: [Int] = []
        if kinds.contains("image") { types.append(PHAssetMediaType.image.rawValue) }
        if kinds.contains("video") { types.append(PHAssetMediaType.video.rawValue) }
        guard !types.isEmpty else { return [] }
        predicates.append(NSPredicate(format: "mediaType IN %@", types))
        let options = PHFetchOptions()
        options.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
        options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: !(json["newestFirst"] as? Bool ?? true))]
        let result = PHAsset.fetchAssets(with: options)
        let offset = max(0, (json["offset"] as? NSNumber)?.intValue ?? 0)
        let limit = max(0, (json["limit"] as? NSNumber)?.intValue ?? 100)
        guard offset < result.count, limit > 0 else { return [] }
        let includeLocation = json["includeLocation"] as? Bool ?? false
        let end = min(result.count, offset + limit)
        var assets: [[String: Any?]] = []
        assets.reserveCapacity(end - offset)
        result.enumerateObjects(at: IndexSet(integersIn: offset..<end), options: []) { asset, _, _ in
            let created = asset.creationDate ?? asset.modificationDate ?? Date(timeIntervalSince1970: 0)
            var item: [String: Any?] = [
                "id": asset.localIdentifier,
                "kind": asset.mediaType == .video ? "video" : "image",
                "createdAtMs": Int64(created.timeIntervalSince1970 * 1000),
                "width": asset.pixelWidth,
                "height": asset.pixelHeight,
                "durationMs": Int64(asset.duration * 1000),
            ]
            if includeLocation, let location = asset.location {
                item["latitude"] = location.coordinate.latitude
                item["longitude"] = location.coordinate.longitude
            }
            assets.append(item)
        }
        return assets
    }

    private static func render(_ asset: PHAsset, size: Int, crop: Bool, promise: WhiskerPromise) {
        let directory: URL
        do {
            directory = try cacheDirectory()
        } catch {
            return Bridge.reject(promise, error)
        }
        let name = asset.localIdentifier.unicodeScalars
            .map { CharacterSet.alphanumerics.contains($0) ? String($0) : "_" }
            .joined()
        let file = directory.appendingPathComponent("\(name)_\(size)_\(crop ? "c" : "f").jpg")
        if let image = UIImage(contentsOfFile: file.path) {
            return Bridge.resolve(promise, ["path": file.path, "width": Int(image.size.width * image.scale), "height": Int(image.size.height * image.scale)])
        }
        let options = PHImageRequestOptions()
        options.isNetworkAccessAllowed = true
        options.deliveryMode = .highQualityFormat
        options.resizeMode = .exact
        options.isSynchronous = false
        let target = CGSize(width: size, height: size)
        PHImageManager.default().requestImage(
            for: asset,
            targetSize: target,
            contentMode: crop ? .aspectFill : .aspectFit,
            options: options
        ) { image, info in
            if (info?[PHImageResultIsDegradedKey] as? Bool) == true { return }
            guard let image else {
                let error = info?[PHImageErrorKey] as? Error
                return Bridge.reject(promise, error.map(Bridge.message) ?? "image not available")
            }
            work.async {
                let output = crop ? cropped(image, side: CGFloat(size)) : image
                guard let data = output.jpegData(compressionQuality: 0.85) else {
                    return Bridge.reject(promise, "cannot encode the image")
                }
                do {
                    try data.write(to: file, options: .atomic)
                    Bridge.resolve(promise, [
                        "path": file.path,
                        "width": Int(output.size.width * output.scale),
                        "height": Int(output.size.height * output.scale),
                    ])
                } catch {
                    Bridge.reject(promise, error)
                }
            }
        }
    }

    /// Center-crops `image` to a `side` × `side` square (pixels).
    private static func cropped(_ image: UIImage, side: CGFloat) -> UIImage {
        let pixelWidth = image.size.width * image.scale
        let pixelHeight = image.size.height * image.scale
        let scale = side / min(pixelWidth, pixelHeight)
        let drawSize = CGSize(width: pixelWidth * scale, height: pixelHeight * scale)
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: side, height: side), format: format).image { _ in
            image.draw(in: CGRect(
                x: (side - drawSize.width) / 2,
                y: (side - drawSize.height) / 2,
                width: drawSize.width,
                height: drawSize.height
            ))
        }
    }

    private static func cacheDirectory() throws -> URL {
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let directory = caches.appendingPathComponent("masamune_whisker_photo_library", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    private static func topViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let window = scenes.flatMap(\.windows).first { $0.isKeyWindow } ?? scenes.first?.windows.first
        var top = window?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
}
