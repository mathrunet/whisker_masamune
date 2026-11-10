// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-billing`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).
//
// The implementation uses StoreKit 2 (iOS 15+), so there are no third-party dependencies.

import PackageDescription

let package = Package(
    name: "masamune-whisker-billing",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerBilling", targets: ["MasamuneWhiskerBilling"]),
    ],
    dependencies: [
        .package(url: "https://github.com/whiskerrs/whisker.git", exact: "0.1.19"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerBilling",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerBilling",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
