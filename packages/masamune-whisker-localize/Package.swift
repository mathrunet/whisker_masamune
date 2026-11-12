// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-localize`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-localize",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerLocalize", targets: ["MasamuneWhiskerLocalize"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "1fe19969ccd0f3e4d31da006327944b95adde9d6"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerLocalize",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerLocalize",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
