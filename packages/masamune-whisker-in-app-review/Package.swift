// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-in-app-review`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-in-app-review",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerInAppReview", targets: ["MasamuneWhiskerInAppReview"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "72840203e02ebb8a48b9976ac6f6256a77e4cf8c"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerInAppReview",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerInAppReview",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
