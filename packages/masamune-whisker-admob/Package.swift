// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-admob`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).
//
// The Google Mobile Ads SDK only depends on Google's User Messaging Platform, so it resolves
// alongside firebase-ios-sdk (pinned to an exact version by whisker-firebase) without conflicts.

import PackageDescription

let package = Package(
    name: "masamune-whisker-admob",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerAdmob", targets: ["MasamuneWhiskerAdmob"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "72840203e02ebb8a48b9976ac6f6256a77e4cf8c"),
        .package(url: "https://github.com/googleads/swift-package-manager-google-mobile-ads.git", from: "13.0.0"),
    ],
    targets: [
        .target(
            name: "MasamuneWhiskerAdmob",
            dependencies: [
                .product(name: "WhiskerModule", package: "whisker"),
                .product(
                    name: "GoogleMobileAds",
                    package: "swift-package-manager-google-mobile-ads",
                    condition: .when(platforms: [.iOS])
                ),
            ],
            path: "ios/Sources/MasamuneWhiskerAdmob",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
