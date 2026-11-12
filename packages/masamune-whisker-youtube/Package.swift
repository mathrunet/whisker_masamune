// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-youtube`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-youtube",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerYoutube", targets: ["MasamuneWhiskerYoutube"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "1fe19969ccd0f3e4d31da006327944b95adde9d6"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerYoutube",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerYoutube",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
