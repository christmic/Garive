// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "GariveIOS", platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.executable(name: "GariveIOS", targets: ["GariveIOS"])],
    targets: [.executableTarget(name: "GariveIOS"), .testTarget(name: "GariveIOSTests", dependencies: ["GariveIOS"])])
