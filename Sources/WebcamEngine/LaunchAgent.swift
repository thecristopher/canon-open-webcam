import Foundation

/// Runs canon-webcam at login and brings it back if it ever dies.
public enum LaunchAgent {
    public static let label = "dev.canon-open-webcam"
    // Canon's own webcam services grab the camera at login and fight us for it
    public static let canonServices = ["com.canon.usa.EWCService", "com.canon.usa.EWCPairingService"]

    public static var plistFile: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/LaunchAgents/\(label).plist")
    }

    public static var logFile: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/canon-open-webcam.log")
    }

    public static var isInstalled: Bool {
        FileManager.default.fileExists(atPath: plistFile.path)
    }

    public static func plist(binary: String, alwaysOn: Bool, logPath: String) -> [String: Any] {
        [
            "Label": label,
            "ProgramArguments": [binary, "start"] + (alwaysOn ? ["--always-on"] : []),
            "RunAtLoad": true,
            "KeepAlive": true,
            // Keeps a crash loop from hammering the camera
            "ThrottleInterval": 5,
            "ProcessType": "Interactive",
            "StandardOutPath": logPath,
            "StandardErrorPath": logPath,
        ]
    }

    /// Copies the binary somewhere stable first, so rebuilding or moving the repo never breaks login startup.
    public static func install(from binary: URL, alwaysOn: Bool) throws {
        let fileManager = FileManager.default
        try fileManager.createDirectory(at: Paths.supportDirectory, withIntermediateDirectories: true)
        try fileManager.createDirectory(at: plistFile.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(at: logFile.deletingLastPathComponent(), withIntermediateDirectories: true)

        unloadAndWait()
        let installed = Paths.installedBinary
        if binary.standardizedFileURL != installed.standardizedFileURL {
            try? fileManager.removeItem(at: installed)
            try fileManager.copyItem(at: binary, to: installed)
        }

        let contents = plist(binary: installed.path, alwaysOn: alwaysOn, logPath: logFile.path)
        let data = try PropertyListSerialization.data(fromPropertyList: contents, format: .xml, options: 0)
        try data.write(to: plistFile, options: .atomic)

        try linkCommand(to: installed, from: Paths.commandLink)
        for service in canonServices { disable(service) }
        launchctl("enable", "\(guiDomain)/\(label)")
        let result = launchctl("bootstrap", guiDomain, plistFile.path)
        guard result.status == 0 else {
            throw ControlError.unknown("launchctl bootstrap failed: \(result.output.trimmingCharacters(in: .whitespacesAndNewlines))")
        }
    }

    public static func uninstall() throws {
        unloadAndWait()
        try? FileManager.default.removeItem(at: plistFile)
        unlinkCommand(at: Paths.commandLink, pointingTo: Paths.installedBinary)
    }

    /// Puts `canon-webcam` on the PATH without touching anything that isn't ours.
    static func linkCommand(to binary: URL, from link: URL) throws {
        let fileManager = FileManager.default
        try fileManager.createDirectory(at: link.deletingLastPathComponent(), withIntermediateDirectories: true)
        if let existing = try? fileManager.destinationOfSymbolicLink(atPath: link.path) {
            guard existing == binary.path else { return }
            try fileManager.removeItem(at: link)
        } else if fileManager.fileExists(atPath: link.path) {
            // A real file someone put there on purpose, leave it alone
            return
        }
        try fileManager.createSymbolicLink(at: link, withDestinationURL: binary)
    }

    static func unlinkCommand(at link: URL, pointingTo binary: URL) {
        guard (try? FileManager.default.destinationOfSymbolicLink(atPath: link.path)) == binary.path else { return }
        try? FileManager.default.removeItem(at: link)
    }

    /// Which of Canon's services are switched off for this user.
    public static func disabledServices(fromPrintDisabled output: String) -> Set<String> {
        var disabled = Set<String>()
        for line in output.split(separator: "\n") {
            let parts = line.components(separatedBy: "=>")
            guard parts.count == 2 else { continue }
            let name = parts[0].trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: "\""))
            let state = parts[1].trimmingCharacters(in: .whitespaces)
            // Older macOS says "true" for disabled, newer says "disabled"
            if state == "disabled" || state == "true" { disabled.insert(name) }
        }
        return disabled
    }

    public static func canonServicesDisabled() -> Bool {
        let disabled = disabledServices(fromPrintDisabled: launchctl("print-disabled", guiDomain).output)
        return canonServices.allSatisfy(disabled.contains)
    }

    static var guiDomain: String { "gui/\(getuid())" }

    private static func disable(_ service: String) {
        // disable survives reboots, bootout stops the copy running right now
        launchctl("disable", "\(guiDomain)/\(service)")
        launchctl("bootout", "\(guiDomain)/\(service)")
    }

    private static func unloadAndWait() {
        launchctl("bootout", "\(guiDomain)/\(label)")
        // bootout returns before the old agent exits, and bootstrapping over it fails with error 5
        let deadline = Date().addingTimeInterval(15)
        while launchctl("print", "\(guiDomain)/\(label)").status == 0, Date() < deadline {
            Thread.sleep(forTimeInterval: 0.5)
        }
    }

    @discardableResult
    static func launchctl(_ arguments: String...) -> (status: Int32, output: String) {
        run("/bin/launchctl", arguments)
    }
}

/// Runs a tool and hands back its exit code and combined output.
@discardableResult
func run(_ tool: String, _ arguments: [String]) -> (status: Int32, output: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: tool)
    process.arguments = arguments
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = pipe
    do {
        try process.run()
    } catch {
        return (-1, "\(error)")
    }
    let output = pipe.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    return (process.terminationStatus, String(decoding: output, as: UTF8.self))
}
