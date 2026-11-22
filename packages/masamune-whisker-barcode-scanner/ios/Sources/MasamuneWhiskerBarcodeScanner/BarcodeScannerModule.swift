// Module definition of masamune-whisker-barcode-scanner (iOS): the camera permission functions and the
// `masamune-whisker-barcode-scanner:Scanner` element (BarcodeScannerView.swift).

import AVFoundation
import UIKit
import WhiskerModule

/// Camera authorization as `authorized`, `denied`, `restricted` or `notDetermined`.
enum CameraAccess {
    static func status() -> String {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: return "authorized"
        case .notDetermined: return "notDetermined"
        case .restricted: return "restricted"
        default: return "denied"
        }
    }

    /// Shows the system prompt while the status is `notDetermined`, then answers the resulting status on the main
    /// thread.
    static func request(_ completion: @escaping (String) -> Void) {
        guard AVCaptureDevice.authorizationStatus(for: .video) == .notDetermined else {
            return Bridge.onMain { completion(status()) }
        }
        AVCaptureDevice.requestAccess(for: .video) { _ in
            DispatchQueue.main.async { completion(status()) }
        }
    }
}

@WhiskerModule
public final class BarcodeScannerModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("BarcodeScanner")

            // permissionStatus() -> "authorized" | "denied" | "restricted" | "notDetermined"
            Function("permissionStatus") { _ in
                Bridge.ok(CameraAccess.status())
            }

            // requestPermission() -> the status after the prompt (no prompt once decided)
            AsyncFunction("requestPermission") { _, promise in
                CameraAccess.request { status in Bridge.resolve(promise, status) }
            }

            // openSettings() — opens this app's page in the Settings app.
            Function("openSettings") { _ in
                guard let url = URL(string: UIApplication.openSettingsURLString) else {
                    return Bridge.err("settings URL unavailable")
                }
                Bridge.onMain { UIApplication.shared.open(url) }
                return Bridge.ok(true)
            }

            View("masamune-whisker-barcode-scanner:Scanner", BarcodeScannerView.self) {
                // Configuration (declared before `torch` and `active` in the Rust schema).
                Prop("request-permission", clear: { (view: BarcodeScannerView) in view.setRequestPermission(true) }) { (view: BarcodeScannerView, value: WhiskerValue) in
                    view.setRequestPermission(Self.bool(value) ?? true)
                }
                Prop("facing", clear: { (view: BarcodeScannerView) in view.setFacing("back") }) { (view: BarcodeScannerView, value: WhiskerValue) in
                    view.setFacing(value.asString ?? "back")
                }
                Prop("debounce-ms", clear: { (view: BarcodeScannerView) in view.setDebounceMs(BarcodeScannerView.defaultDebounceMs) }) { (view: BarcodeScannerView, value: WhiskerValue) in
                    view.setDebounceMs(value.asString.flatMap { Int($0) } ?? BarcodeScannerView.defaultDebounceMs)
                }
                Prop("torch", clear: { (view: BarcodeScannerView) in view.setTorch(false) }) { (view: BarcodeScannerView, value: WhiskerValue) in
                    view.setTorch(Self.bool(value) ?? false)
                }
                Prop("active", clear: { (view: BarcodeScannerView) in view.setActive(false) }) { (view: BarcodeScannerView, value: WhiskerValue) in
                    view.setActive(Self.bool(value) ?? false)
                }

                Events("scan", "permission", "error")
            }
        }
    }

    /// A boolean attribute, sent as a bool or as `"true"` / `"false"`.
    private static func bool(_ value: WhiskerValue) -> Bool? {
        if let flag = value.asBool { return flag }
        switch value.asString {
        case "true": return true
        case "false": return false
        default: return nil
        }
    }
}
