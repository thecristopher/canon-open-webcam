import Foundation
import WebcamEngine

/// Opens Canon Webcam quietly in the menu bar at login. A launch agent instead of a plain login item,
/// because it can pass --background so no window pops up every time you log in.
public enum AppLoginItem {
    public static let label = "dev.canon-open-webcam.app"
    public static let backgroundFlag = "--background"

    public static var plistFile: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/LaunchAgents/\(label).plist")
    }

    public static var isInstalled: Bool {
        FileManager.default.fileExists(atPath: plistFile.path)
    }

    public static func plist(appExecutable: String) -> [String: Any] {
        [
            "Label": label,
            "ProgramArguments": [appExecutable, backgroundFlag],
            "RunAtLoad": true,
            // Only in a real login session, never for ssh or background sessions
            "LimitLoadToSessionType": "Aqua",
            "ProcessType": "Interactive",
        ]
    }

    /// Written now, picked up by launchd at the next login. The installer opens the app itself this time.
    public static func install(appBundle: URL) throws {
        guard let executable = Bundle(url: appBundle)?.executableURL else {
            throw SetupError("Couldn't find the program inside \(appBundle.lastPathComponent).")
        }
        try FileManager.default.createDirectory(at: plistFile.deletingLastPathComponent(), withIntermediateDirectories: true)
        let data = try PropertyListSerialization.data(fromPropertyList: plist(appExecutable: executable.path), format: .xml, options: 0)
        try data.write(to: plistFile, options: .atomic)
        LaunchAgent.launchctl("enable", "\(LaunchAgent.guiDomain)/\(label)")
    }

    public static func uninstall() {
        LaunchAgent.launchctl("bootout", "\(LaunchAgent.guiDomain)/\(label)")
        try? FileManager.default.removeItem(at: plistFile)
    }
}
