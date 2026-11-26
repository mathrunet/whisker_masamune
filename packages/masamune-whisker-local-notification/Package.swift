// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-local-notification`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-local-notification",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerLocalNotification", targets: ["MasamuneWhiskerLocalNotification"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "3aec202955b4b1423437f822c63c5edaec405934"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerLocalNotification",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerLocalNotification",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
