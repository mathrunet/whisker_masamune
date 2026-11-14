// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-firebase-remote-config`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-firebase-remote-config",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerFirebaseRemoteConfig", targets: ["MasamuneWhiskerFirebaseRemoteConfig"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "72840203e02ebb8a48b9976ac6f6256a77e4cf8c"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", exact: "12.19.2"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerFirebaseRemoteConfig",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
                .product(name: "FirebaseCore", package: "firebase-ios-sdk"),
                .product(name: "FirebaseRemoteConfig", package: "firebase-ios-sdk"),
            ],
            path: "ios/Sources/MasamuneWhiskerFirebaseRemoteConfig",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
