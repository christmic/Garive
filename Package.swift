// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "GariveProtocolSchema",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "GariveProcessProtocol", targets: ["GariveProcessProtocol"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", exact: "1.38.1"),
    ],
    targets: [
        .target(
            name: "GariveProcessProtocol",
            dependencies: [
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            resources: [.copy("swift-protobuf-config.json")],
            plugins: [
                .plugin(name: "SwiftProtobufPlugin", package: "swift-protobuf"),
            ]
        ),
        .testTarget(
            name: "GariveProcessProtocolTests",
            dependencies: ["GariveProcessProtocol"]
        ),
    ]
)
