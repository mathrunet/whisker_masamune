// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-messaging-firebase`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-messaging-firebase",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerMessagingFirebase", targets: ["MasamuneWhiskerMessagingFirebase"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "3aec202955b4b1423437f822c63c5edaec405934"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", exact: "12.19.2"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerMessagingFirebase",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
                .product(name: "FirebaseCore", package: "firebase-ios-sdk"),
                .product(name: "FirebaseMessaging", package: "firebase-ios-sdk"),
            ],
            path: "ios/Sources/MasamuneWhiskerMessagingFirebase",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
