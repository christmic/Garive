// swift-tools-version: 6.3

import PackageDescription

let package = Package(
    name: "GariveMacOSNative",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "GariveComputerUse", targets: ["GariveComputerUse"]),
        .library(name: "GariveProcessIsolation", targets: ["GariveProcessIsolation"]),
    ],
    targets: [
        .target(name: "GariveComputerUse"),
        .target(name: "GariveProcessIsolation"),
        .testTarget(name: "GariveComputerUseTests", dependencies: ["GariveComputerUse"]),
        .testTarget(
            name: "GariveProcessIsolationTests",
            dependencies: ["GariveProcessIsolation"]
        ),
    ]
)
