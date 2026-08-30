// swift-tools-version: 6.3

import PackageDescription

let package = Package(
    name: "GariveMacOSNative",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "GariveComputerUse", targets: ["GariveComputerUse"]),
    ],
    targets: [
        .target(name: "GariveComputerUse"),
        .testTarget(name: "GariveComputerUseTests", dependencies: ["GariveComputerUse"]),
    ]
)
