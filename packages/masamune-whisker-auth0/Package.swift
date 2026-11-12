// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-auth0`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-auth0",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerAuth0", targets: ["MasamuneWhiskerAuth0"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "1fe19969ccd0f3e4d31da006327944b95adde9d6"),
        .package(url: "https://github.com/auth0/Auth0.swift.git", from: "2.24.1"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerAuth0",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
                .product(name: "Auth0", package: "Auth0.swift"),
            ],
            path: "ios/Sources/MasamuneWhiskerAuth0",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
