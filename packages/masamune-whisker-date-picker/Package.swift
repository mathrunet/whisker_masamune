// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-date-picker`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-date-picker",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerDatePicker", targets: ["MasamuneWhiskerDatePicker"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "6bfdb24fb5bb77dda1474d68fbe31c2a7ffec5b4"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerDatePicker",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
            ],
            path: "ios/Sources/MasamuneWhiskerDatePicker",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
