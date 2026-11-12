// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-audio-capture`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-audio-capture",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerAudioCapture", targets: ["MasamuneWhiskerAudioCapture"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "b2de64ce70f0d412cb9dceeaeb3a1576cf27e821"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerAudioCapture",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerAudioCapture",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
