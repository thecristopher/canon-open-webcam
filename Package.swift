// swift-tools-version: 6.0
import Foundation
import PackageDescription

// Homebrew keeps libusb in different places on Apple Silicon and Intel
let libusbLibs = ["/opt/homebrew/opt/libusb/lib", "/usr/local/opt/libusb/lib"].filter { FileManager.default.fileExists(atPath: $0) }

let package = Package(
    name: "canon-open-webcam",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "canon-webcam", targets: ["canon-webcam"]),
    ],
    targets: [
        .systemLibrary(name: "CLibUSB", path: "Sources/CLibUSB"),
        .target(name: "CanonPTP"),
        .target(
            name: "CanonUSB",
            dependencies: ["CLibUSB", "CanonPTP"],
            linkerSettings: [.unsafeFlags(libusbLibs.map { "-L\($0)" })]
        ),
        .target(name: "WebcamEngine", dependencies: ["CanonPTP", "CanonUSB"]),
        .executableTarget(name: "canon-webcam", dependencies: ["WebcamEngine"]),
        .testTarget(name: "CanonPTPTests", dependencies: ["CanonPTP"]),
        .testTarget(name: "WebcamEngineTests", dependencies: ["WebcamEngine"]),
    ],
    swiftLanguageModes: [.v5]
)
