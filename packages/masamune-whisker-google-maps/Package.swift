// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-google-maps`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).
//
// Google Maps SDK for iOS 10.x is the last line that supports iOS 16 (11.0 requires iOS 17), so the
// dependency stays below 11.0 and the package raises the app's iOS floor to 16.

import PackageDescription

let package = Package(
    name: "masamune-whisker-google-maps",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "MasamuneWhiskerGoogleMaps", targets: ["MasamuneWhiskerGoogleMaps"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "72840203e02ebb8a48b9976ac6f6256a77e4cf8c"),
        .package(url: "https://github.com/googlemaps/ios-maps-sdk.git", "10.15.0"..<"11.0.0"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerGoogleMaps",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
                .product(name: "GoogleMaps", package: "ios-maps-sdk"),
            ],
            path: "ios/Sources/MasamuneWhiskerGoogleMaps",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
