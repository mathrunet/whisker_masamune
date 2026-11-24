// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-audio-session`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-audio-session",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerAudioSession", targets: ["MasamuneWhiskerAudioSession"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "e554539121cde591e129a4fc87aaaf305933abcd"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerAudioSession",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerAudioSession",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
