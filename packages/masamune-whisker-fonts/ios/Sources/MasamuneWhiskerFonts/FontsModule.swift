// Makes bundled fonts available by name from whisker's Text.
//
// whisker's iOS Host (`resolveWhiskerBaseFont`) resolves a family with `UIFont(name:size:)` (a
// PostScript or full font name), then `UIFont.fontNames(forFamilyName:)`, and applies the weight by
// adding a weight trait to the descriptor of that font before `UIFont(descriptor:size:)`. A font
// registered with CoreText is only known by the names stored inside the file, so the arbitrary family
// names the app registers (the Android contract, for example "MPLUSRounded1c" for a file whose family
// is "Rounded Mplus 1c Bold") would never resolve, and the weight trait has no effect on a font
// looked up by name. This module therefore
//
// 1. registers every file for this process with `CTFontManagerRegisterGraphicsFont`,
// 2. answers `+[UIFont fontWithName:size:]` for a registered family name with the face assigned to
//    weight 400 (its PostScript name), and
// 3. answers `+[UIFont fontWithDescriptor:size:]` for a descriptor that names one of the registered
//    faces and carries a weight trait with the face assigned to that weight,
//
// so `font_family("Name")` renders and measures with the same files and weights as on Android.
// Both methods are swizzled once; names that were not registered pass through unchanged.

import CoreText
import ObjectiveC
import UIKit
import WhiskerModule

@WhiskerModule
public final class FontsModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Fonts")

            // register({ families: [{ family, files: [{ path, weights }] }] }) -> array of registered family names
            Function("register") { args in
                Bridge.ok(FontRegistry.register(Bridge.args(args)))
            }
        }
    }
}

enum FontRegistry {
    /// Family name -> CSS weight -> PostScript name of the registered face.
    private static var families: [String: [Int: String]] = [:]
    /// PostScript name of a registered face -> its family name.
    private static var owners: [String: String] = [:]
    private static let lock = NSLock()

    static func register(_ json: [String: Any]) -> [String] {
        _ = installLookups
        let entries = json["families"] as? [[String: Any]] ?? []
        var done: [String] = []
        for entry in entries {
            guard let name = entry["family"] as? String else { continue }
            if locked({ families[name] != nil }) {
                done.append(name)
                continue
            }
            var faces: [Int: String] = [:]
            for file in entry["files"] as? [[String: Any]] ?? [] {
                guard let path = file["path"] as? String else { continue }
                let weights = (file["weights"] as? [NSNumber] ?? []).map(\.intValue)
                do {
                    let postScriptName = try registerFile(path)
                    for weight in weights { faces[weight] = postScriptName }
                } catch {
                    NSLog("MasamuneWhiskerFonts: failed to load %@ (%@): %@", name, path, Bridge.message(error))
                }
            }
            guard !faces.isEmpty else { continue }
            locked {
                families[name] = faces
                for postScriptName in faces.values { owners[postScriptName] = name }
            }
            done.append(name)
        }
        return done
    }

    /// Registers one font file and returns its PostScript name.
    private static func registerFile(_ path: String) throws -> String {
        let url = fileURL(path)
        guard let provider = CGDataProvider(url: url as CFURL), let font = CGFont(provider) else {
            throw FontError("cannot read \(url.path)")
        }
        guard let postScriptName = font.postScriptName as String? else {
            throw FontError("\(url.lastPathComponent) has no PostScript name")
        }
        var error: Unmanaged<CFError>?
        if !CTFontManagerRegisterGraphicsFont(font, &error), let failure = error?.takeRetainedValue() {
            // Registering the same file again (for example after the Rust side restarts) is fine.
            let code = CFErrorGetCode(failure)
            if code != CTFontManagerError.alreadyRegistered.rawValue && code != CTFontManagerError.duplicatedName.rawValue {
                throw failure as Error
            }
        }
        return postScriptName
    }

    /// Accepts whisker-asset URLs (`file://<bundle>/whisker_assets/…`), absolute paths and paths
    /// relative to the bundled `whisker_assets` directory.
    private static func fileURL(_ path: String) -> URL {
        if path.hasPrefix("file://") {
            let raw = String(path.dropFirst("file://".count))
            return URL(fileURLWithPath: raw.removingPercentEncoding ?? raw)
        }
        if path.hasPrefix("/") { return URL(fileURLWithPath: path) }
        return Bundle.main.bundleURL.appendingPathComponent("whisker_assets").appendingPathComponent(path)
    }

    /// The face of `family` for a CSS weight: the exact weight, otherwise the closest one.
    private static func face(family: String, weight: Int) -> String? {
        locked {
            guard let faces = families[family] else { return nil }
            if let exact = faces[weight] { return exact }
            return faces.min { lhs, rhs in
                let left = abs(lhs.key - weight), right = abs(rhs.key - weight)
                return left == right ? lhs.key < rhs.key : left < right
            }?.value
        }
    }

    private static func owner(of postScriptName: String) -> String? {
        locked { owners[postScriptName] }
    }

    private static func locked<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }

    // MARK: - UIFont lookups

    private typealias NamedFactory = @convention(c) (AnyObject, Selector, NSString?, CGFloat) -> UIFont?
    private typealias DescriptorFactory = @convention(c) (AnyObject, Selector, UIFontDescriptor, CGFloat) -> UIFont

    /// Swizzles the two `UIFont` factories once (see the file comment).
    private static let installLookups: Void = {
        let named = NSSelectorFromString("fontWithName:size:")
        if let method = class_getClassMethod(UIFont.self, named) {
            let original = unsafeBitCast(method_getImplementation(method), to: NamedFactory.self)
            let block: @convention(block) (AnyObject, NSString?, CGFloat) -> UIFont? = { type, name, size in
                if let name, let postScriptName = face(family: name as String, weight: 400) {
                    return original(type, named, postScriptName as NSString, size)
                }
                return original(type, named, name, size)
            }
            method_setImplementation(method, imp_implementationWithBlock(block))
        }

        let described = NSSelectorFromString("fontWithDescriptor:size:")
        if let method = class_getClassMethod(UIFont.self, described) {
            let original = unsafeBitCast(method_getImplementation(method), to: DescriptorFactory.self)
            let block: @convention(block) (AnyObject, UIFontDescriptor, CGFloat) -> UIFont = { type, descriptor, size in
                original(type, described, weighted(descriptor) ?? descriptor, size)
            }
            method_setImplementation(method, imp_implementationWithBlock(block))
        }
    }()

    /// A descriptor naming a registered face with a weight trait, redirected to the face of that weight.
    private static func weighted(_ descriptor: UIFontDescriptor) -> UIFontDescriptor? {
        // Cheap checks first: every `UIFont(descriptor:size:)` in the app passes through here.
        guard let name = descriptor.object(forKey: .name) as? String, let family = owner(of: name) else { return nil }
        var attributes = descriptor.fontAttributes
        guard let traits = attributes[.traits] as? [String: Any],
              let weight = traits[UIFontDescriptor.TraitKey.weight.rawValue] as? NSNumber
        else { return nil }
        // The Host maps CSS weights as (weight - 400) / 500.
        let css = Int(((weight.doubleValue * 500 + 400) / 100).rounded()) * 100
        guard let target = face(family: family, weight: min(max(css, 100), 900)), target != name else { return nil }
        attributes[.name] = target
        return UIFontDescriptor(fontAttributes: attributes)
    }
}

private struct FontError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}
