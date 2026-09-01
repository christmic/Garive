// swift-tools-version: 6.3

import PackageDescription

let package = Package(
    name: "GariveMacOSNative",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "GariveNativeXPC", targets: ["GariveNativeXPC"]),
        .library(name: "GariveComputerUse", targets: ["GariveComputerUse"]),
        .library(name: "GariveProcessIsolation", targets: ["GariveProcessIsolation"]),
        .library(name: "GariveProcessService", targets: ["GariveProcessService"]),
        .executable(
            name: "GariveProcessIsolationService",
            targets: ["GariveProcessIsolationService"]
        ),
    ],
    dependencies: [
        .package(name: "GariveProtocolSchema", path: "../.."),
    ],
    targets: [
        .target(name: "GariveNativeXPC"),
        .target(name: "GariveComputerUse"),
        .target(
            name: "GariveProcessIsolation",
            dependencies: [
                .product(name: "GariveProcessProtocol", package: "GariveProtocolSchema"),
            ]
        ),
        .target(
            name: "GariveProcessService",
            dependencies: [
                "GariveNativeXPC",
                .product(name: "GariveProcessProtocol", package: "GariveProtocolSchema"),
            ]
        ),
        .executableTarget(
            name: "GariveProcessIsolationService",
            dependencies: ["GariveProcessService"]
        ),
        .testTarget(
            name: "GariveComputerUseTests",
            dependencies: ["GariveComputerUse", "GariveNativeXPC"]
        ),
        .testTarget(
            name: "GariveProcessIsolationTests",
            dependencies: ["GariveProcessIsolation"]
        ),
        .testTarget(
            name: "GariveProcessServiceTests",
            dependencies: [
                "GariveNativeXPC",
                "GariveProcessService",
                .product(name: "GariveProcessProtocol", package: "GariveProtocolSchema"),
            ]
        ),
    ]
)
