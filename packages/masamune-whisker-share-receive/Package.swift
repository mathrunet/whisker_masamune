// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-share-receive`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-share-receive",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerShareReceive", targets: ["MasamuneWhiskerShareReceive"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "3aec202955b4b1423437f822c63c5edaec405934"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerShareReceive",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerShareReceive",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
