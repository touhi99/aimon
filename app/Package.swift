// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "AIMon",
    platforms: [.macOS(.v14)],
    targets: [.executableTarget(name: "AIMon", path: "Sources/AIMon", swiftSettings: [.swiftLanguageMode(.v5)])]
)
