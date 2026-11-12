// swift-tools-version:5.9
//
// SwiftPM manifest for the iOS implementation of `masamune-whisker-firebase-firestore-db`.
// The package name must match the crate name, and the product / target name is the
// crate name in PascalCase (whisker-cng resolves modules by that convention).

import PackageDescription

let package = Package(
    name: "masamune-whisker-firebase-firestore-db",
    platforms: [.iOS(.v15), .macOS(.v13)],
    products: [
        .library(name: "MasamuneWhiskerFirebaseFirestoreDb", targets: ["MasamuneWhiskerFirebaseFirestoreDb"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mathrunet/whisker.git", revision: "b2de64ce70f0d412cb9dceeaeb3a1576cf27e821"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", exact: "12.19.2"),
    ],
    targets: [
        // Objective-C helper that converts the SDK's NSExceptions into Swift errors.
        .target(
            name: "MasamuneWhiskerFirebaseFirestoreDbObjC",
            path: "ios/Sources/MasamuneWhiskerFirebaseFirestoreDbObjC"
        ),
        .target(
            name: "MasamuneWhiskerFirebaseFirestoreDb",
            dependencies: [
                "MasamuneWhiskerFirebaseFirestoreDbObjC",
                .product(name: "WhiskerModule", package: "whisker"),
                .product(name: "FirebaseCore", package: "firebase-ios-sdk"),
                .product(name: "FirebaseFirestore", package: "firebase-ios-sdk"),
            ],
            path: "ios/Sources/MasamuneWhiskerFirebaseFirestoreDb",
            plugins: [
                .plugin(name: "WhiskerModuleCodegenPlugin", package: "whisker"),
            ]
        ),
    ]
)
