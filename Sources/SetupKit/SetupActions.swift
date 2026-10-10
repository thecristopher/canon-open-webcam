import AppKit
import Foundation
import WebcamEngine

/// Every side effect the installer and uninstaller perform, kept here so the models only orchestrate.
public enum SetupActions {
    static let obsDownloadPage = URL(string: "https://obsproject.com/download")!
    static let extensionSettings = URL(string: "x-apple.systempreferences:com.apple.LoginItems-Settings.extension")!

    /// Reads everything the checklist needs. Cheap enough to call every second.
    public static func currentFacts() -> InstallerFacts {
        InstallerFacts(
            obsInstalled: SystemChecks.obsIsInstalled,
            driverReady: SystemChecks.virtualCameraIsReady,
            installed: SystemChecks.isInstalled,
            cameraConnected: SystemChecks.cameraIsPluggedIn,
            hasHomebrew: homebrew != nil
        )
    }

    /// Homebrew lives in different places on Apple Silicon and Intel
    static var homebrew: URL? {
        ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
            .first { FileManager.default.isExecutableFile(atPath: $0) }
            .map(URL.init(fileURLWithPath:))
    }

    /// Installs OBS with Homebrew when it's there, otherwise sends you to the download page.
    public static func getOBS() async throws {
        guard let homebrew else {
            NSWorkspace.shared.open(obsDownloadPage)
            return
        }
        let result = await runInBackground(homebrew.path, ["install", "--cask", "obs"])
        guard result.status == 0 else {
            throw SetupError("Homebrew couldn't install OBS:\n\(result.output.suffix(400))")
        }
    }

    /// Starting OBS's virtual camera is what makes macOS ask to approve the driver.
    public static func startDriverApproval() {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.arguments = ["--startvirtualcam", "--minimize-to-tray", "--disable-shutdown-check"]
        configuration.activates = false
        NSWorkspace.shared.openApplication(at: URL(fileURLWithPath: SystemChecks.obsAppPath), configuration: configuration)
        openExtensionSettings()
    }

    public static func openExtensionSettings() {
        NSWorkspace.shared.open(extensionSettings)
    }

    /// OBS was only needed to install its driver, and leaving it open would fight us for the camera.
    public static func quitOBS() {
        NSRunningApplication.runningApplications(withBundleIdentifier: "com.obsproject.obs-studio").forEach { $0.terminate() }
    }

    /// The installer carries its own copy of the engine, so it works without the source code around.
    public static func install() async throws {
        guard let engine = Bundle.main.url(forResource: "canon-webcam", withExtension: nil) else {
            throw SetupError("This installer is missing its copy of canon-webcam. Rebuild it with `make dist`.")
        }
        try await Task.detached { try LaunchAgent.install(from: engine, alwaysOn: false) }.value
    }

    public static func uninstall(restoreCanon: Bool) async throws {
        try await Task.detached { try LaunchAgent.uninstall(restoreCanonServices: restoreCanon) }.value
    }

    /// Our test preview just closed, so let the engine rest the camera now instead of waiting.
    public static func askEngineToRecheck() {
        _ = try? ControlClient.send(.recheck)
    }

    private static func runInBackground(_ tool: String, _ arguments: [String]) async -> (status: Int32, output: String) {
        await Task.detached {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: tool)
            process.arguments = arguments
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = pipe
            do { try process.run() } catch { return (Int32(-1), "\(error)") }
            let output = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            return (process.terminationStatus, String(decoding: output, as: UTF8.self))
        }.value
    }
}

public struct SetupError: Error, CustomStringConvertible {
    public let description: String

    public init(_ description: String) {
        self.description = description
    }
}
