import AppKit
import Foundation
import WebcamEngine

/// Every side effect the installer, the uninstaller and the app perform, kept here so the models only orchestrate.
public enum SetupActions {
    public static let appName = "Canon Webcam.app"
    public static let appBundleID = "dev.canon-open-webcam.app"
    static let obsDownloadPage = URL(string: "https://obsproject.com/download")!
    static let extensionSettings = URL(string: "x-apple.systempreferences:com.apple.LoginItems-Settings.extension")!

    // MARK: Facts

    /// Reads everything the installer checklist needs. Cheap enough to call every second.
    public static func currentFacts() -> InstallerFacts {
        InstallerFacts(
            obsInstalled: SystemChecks.obsIsInstalled,
            driverReady: SystemChecks.virtualCameraIsReady,
            installed: installedApp != nil && SystemChecks.isInstalled,
            cameraConnected: SystemChecks.cameraIsPluggedIn,
            hasHomebrew: homebrew != nil
        )
    }

    /// True until the driver is approved and the engine is installed.
    public static func needsSetup() -> Bool {
        !(SystemChecks.virtualCameraIsReady && SystemChecks.isInstalled)
    }

    /// Wherever Canon Webcam.app lives, in the shared or the personal Applications folder.
    public static var installedApp: URL? {
        applicationsFolders.map { $0.appendingPathComponent(appName) }.first { FileManager.default.fileExists(atPath: $0.path) }
    }

    /// /Applications when this account may write there, otherwise the personal one that never needs an admin
    static var bestApplicationsFolder: URL {
        FileManager.default.isWritableFile(atPath: "/Applications") ? applicationsFolders[0] : applicationsFolders[1]
    }

    static var applicationsFolders: [URL] {
        [URL(fileURLWithPath: "/Applications"), FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Applications")]
            .map(\.standardizedFileURL)
    }

    /// Homebrew lives in different places on Apple Silicon and Intel
    static var homebrew: URL? {
        ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
            .first { FileManager.default.isExecutableFile(atPath: $0) }
            .map(URL.init(fileURLWithPath:))
    }

    // MARK: Camera driver

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

    // MARK: Install

    /// The app to install: the copy the installer carries, or this very app when it's running setup on itself.
    static var appToInstall: URL? {
        if let carried = Bundle.main.url(forResource: "Canon Webcam", withExtension: "app") { return carried }
        return Bundle.main.bundleIdentifier == appBundleID ? Bundle.main.bundleURL : nil
    }

    /// Copies the app into Applications, starts the engine, adds the login item and opens the app.
    public static func install() async throws {
        guard let source = appToInstall else {
            throw SetupError("This installer is missing its copy of Canon Webcam. Rebuild it with `make dist`.")
        }
        let installed = try await Task.detached { try copyIntoApplications(source) }.value
        let engine = installed.appendingPathComponent("Contents/Resources/canon-webcam")
        try await Task.detached { try LaunchAgent.install(from: engine, alwaysOn: false) }.value
        try AppLoginItem.install(appBundle: installed)
        if Bundle.main.bundleIdentifier != appBundleID {
            // Waits for the launch to go through, so a headless install can't exit out from under it
            _ = try await NSWorkspace.shared.openApplication(at: installed, configuration: NSWorkspace.OpenConfiguration())
        }
    }

    /// Leaves an app that already lives in Applications where it is, otherwise copies it into the best folder.
    static func copyIntoApplications(_ source: URL) throws -> URL {
        if applicationsFolders.contains(source.deletingLastPathComponent().standardizedFileURL) { return source }
        quitApp()
        let folder = bestApplicationsFolder
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let destination = folder.appendingPathComponent(appName)
        try? FileManager.default.removeItem(at: destination)
        try FileManager.default.copyItem(at: source, to: destination)
        return destination
    }

    public static func openApp(_ app: URL? = installedApp) {
        guard let app else { return }
        NSWorkspace.shared.openApplication(at: app, configuration: NSWorkspace.OpenConfiguration())
    }

    /// After you update the app, the background engine quietly gets the new version too.
    public static func syncInstalledEngine() async {
        guard SystemChecks.isInstalled, Bundle.main.bundleIdentifier == appBundleID,
              let bundled = Bundle.main.url(forResource: "canon-webcam", withExtension: nil),
              FileManager.default.contentsEqual(atPath: bundled.path, andPath: Paths.installedBinary.path) == false
        else { return }
        try? await Task.detached { try LaunchAgent.install(from: bundled, alwaysOn: false) }.value
    }

    // MARK: Uninstall

    /// What uninstalling would remove right now, in words a person would recognize.
    public static func installedPieces() -> [String] {
        var pieces: [String] = []
        if installedApp != nil { pieces.append("The Canon Webcam app") }
        if AppLoginItem.isInstalled { pieces.append("The login item that opens it in the menu bar") }
        return pieces + LaunchAgent.installedPieces()
    }

    public static func uninstall(restoreCanon: Bool) async throws {
        try await Task.detached {
            if Bundle.main.bundleIdentifier != appBundleID { quitApp() }
            AppLoginItem.uninstall()
            try LaunchAgent.uninstall(restoreCanonServices: restoreCanon)
            for folder in applicationsFolders {
                try? FileManager.default.removeItem(at: folder.appendingPathComponent(appName))
            }
        }.value
    }

    static func quitApp() {
        NSRunningApplication.runningApplications(withBundleIdentifier: appBundleID).forEach { $0.terminate() }
    }

    // MARK: Small conveniences

    public static func openLog() {
        NSWorkspace.shared.open(LaunchAgent.logFile)
    }

    /// Our preview just closed, so let the engine rest the camera now instead of waiting.
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
