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
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "6bfdb24fb5bb77dda1474d68fbe31c2a7ffec5b4"),
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
