// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-google-sign-in`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-google-sign-in",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerGoogleSignIn", targets: ["MasamuneWhiskerGoogleSignIn"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "d7cbd957dac30f0b30b7eb31bf2a31d1b22ad2d5"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", exact: "12.19.2"),
        .package(url: "https://github.com/google/GoogleSignIn-iOS.git", from: "10.0.0"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerGoogleSignIn",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
                .product(name: "FirebaseCore", package: "firebase-ios-sdk"),
                .product(name: "FirebaseAuth", package: "firebase-ios-sdk"),
                .product(name: "GoogleSignIn", package: "GoogleSignIn-iOS"),
            ],
            path: "ios/Sources/MasamuneWhiskerGoogleSignIn",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
