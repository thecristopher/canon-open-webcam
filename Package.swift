// swift-tools-version: 6.0
import Foundation
import PackageDescription

// libusb is linked in statically so the apps run on Macs without Homebrew.
// Homebrew is only needed to build, and keeps it in different places on Apple Silicon and Intel.
let staticLibusb = ["/opt/homebrew/opt/libusb/lib/libusb-1.0.a", "/usr/local/opt/libusb/lib/libusb-1.0.a"]
    .first { FileManager.default.fileExists(atPath: $0) } ?? "libusb-1.0.a"

let package = Package(
    name: "canon-open-webcam",
    platforms: [.macOS("26.0")],
    products: [
        .executable(name: "canon-webcam", targets: ["canon-webcam"]),
        .executable(name: "CanonWebcamInstaller", targets: ["CanonWebcamInstaller"]),
        .executable(name: "CanonWebcamUninstaller", targets: ["CanonWebcamUninstaller"]),
    ],
    targets: [
        .systemLibrary(name: "CLibUSB", path: "Sources/CLibUSB"),
        .target(name: "CanonPTP"),
        .target(
            name: "CanonUSB",
            dependencies: ["CLibUSB", "CanonPTP"],
            linkerSettings: [
                .unsafeFlags([staticLibusb]),
                .linkedFramework("IOKit"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Security"),
            ]
        ),
        .target(name: "WebcamEngine", dependencies: ["CanonPTP", "CanonUSB"]),
        .target(name: "SetupKit", dependencies: ["WebcamEngine"]),
        .executableTarget(name: "canon-webcam", dependencies: ["WebcamEngine"]),
        .executableTarget(name: "CanonWebcamInstaller", dependencies: ["SetupKit"]),
        .executableTarget(name: "CanonWebcamUninstaller", dependencies: ["SetupKit"]),
        .testTarget(name: "CanonPTPTests", dependencies: ["CanonPTP"]),
        .testTarget(name: "WebcamEngineTests", dependencies: ["WebcamEngine"]),
        .testTarget(name: "SetupKitTests", dependencies: ["SetupKit"]),
    ],
    swiftLanguageModes: [.v5]
)
