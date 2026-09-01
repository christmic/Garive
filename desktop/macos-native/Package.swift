// swift-tools-version: 6.3

import PackageDescription

let package = Package(
    name: "GariveMacOSNative",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "GariveComputerUse", targets: ["GariveComputerUse"]),
        .library(name: "GariveProcessIsolation", targets: ["GariveProcessIsolation"]),
    ],
    dependencies: [
        .package(name: "GariveProtocolSchema", path: "../.."),
    ],
    targets: [
        .target(name: "GariveComputerUse"),
        .target(
            name: "GariveProcessIsolation",
            dependencies: [
                .product(name: "GariveProcessProtocol", package: "GariveProtocolSchema"),
            ]
        ),
        .testTarget(name: "GariveComputerUseTests", dependencies: ["GariveComputerUse"]),
        .testTarget(
            name: "GariveProcessIsolationTests",
            dependencies: ["GariveProcessIsolation"]
        ),
    ]
)
