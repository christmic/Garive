// swift-tools-version: 6.0
import Foundation
import PackageDescription

let frameworkPath = "../shared/build/XCFrameworks/debug/GariveShared.xcframework"
let hasShared = FileManager.default.fileExists(atPath: frameworkPath)
var targets: [Target] = [
    .executableTarget(name: "GariveIOS", dependencies: hasShared ? ["GariveShared"] : []),
    .testTarget(name: "GariveIOSTests", dependencies: ["GariveIOS"]),
]
if hasShared { targets.append(.binaryTarget(name: "GariveShared", path: frameworkPath)) }

let package = Package(
    name: "GariveIOS",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.executable(name: "GariveIOS", targets: ["GariveIOS"])],
    targets: targets
)
