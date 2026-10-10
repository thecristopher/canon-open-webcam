import Foundation
import WebcamEngine

func printError(_ message: String) {
    FileHandle.standardError.write(Data("Error: \(message)\n".utf8))
}

func timestamped(_ message: String) {
    let formatter = DateFormatter()
    formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
    print("\(formatter.string(from: Date())) \(message)")
}

func startEngine(alwaysOn: Bool) -> Int32 {
    let engine = Engine(config: EngineConfig(alwaysOn: alwaysOn, log: timestamped))
    let server = ControlServer(handle: engine.handle)
    do {
        try server.start()
    } catch {
        printError("\(error). Is another canon-webcam already running?")
        return 1
    }

    let stopFlag = StopFlag()
    // Ctrl+C from a terminal and SIGTERM from launchd both mean "hand the camera back and leave"
    let signalSources = [SIGINT, SIGTERM].map { signalNumber -> DispatchSourceSignal in
        signal(signalNumber, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: signalNumber, queue: .global())
        source.setEventHandler { stopFlag.raise() }
        source.resume()
        return source
    }
    engine.run(until: stopFlag.isRaised)
    signalSources.forEach { $0.cancel() }
    return 0
}

func showStatus() -> Int32 {
    do {
        let response = try ControlClient.send(.status)
        guard let status = response.status else {
            printError(response.message ?? "no status")
            return 1
        }
        print(describe(status))
        return 0
    } catch {
        printError("\(error)")
        return 1
    }
}

func describe(_ status: EngineStatus) -> String {
    var lines = ["State:   \(status.state)"]
    if let model = status.model { lines.append("Camera:  \(model)") }
    if let lens = status.lens { lines.append("Lens:    \(lens)") }
    if status.framesPerSecond > 0 { lines.append(String(format: "Speed:   %.1f fps", status.framesPerSecond)) }
    lines.append("Mirror:  \(status.mirror ? "on" : "off")")
    lines.append(String(format: "Zoom:    %.1fx", status.zoom))
    for name in status.settings.keys.sorted() {
        guard let setting = status.settings[name] else { continue }
        let choices = setting.choices.isEmpty ? "locked by the camera's mode" : setting.choices.joined(separator: " ")
        lines.append("\(name): \(setting.value)  (\(choices))")
    }
    return lines.joined(separator: "\n")
}

func sendToEngine(_ request: ControlRequest) -> Int32 {
    do {
        let response = try ControlClient.send(request)
        if response.ok {
            print(response.message ?? "Done.")
            return 0
        }
        printError(response.message ?? "the engine said no")
        return 1
    } catch {
        printError("\(error)")
        return 1
    }
}

func runDoctor() -> Int32 {
    let checks = Doctor.runChecks()
    for check in checks {
        let label = check.verdict == .ok ? "ok" : check.verdict.rawValue.uppercased()
        print("\(label.padding(toLength: 8, withPad: " ", startingAt: 0)) \(check.message)")
    }
    return Doctor.isHealthy(checks) ? 0 : 1
}

func installAgent(alwaysOn: Bool) -> Int32 {
    let binary = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
    do {
        try LaunchAgent.install(from: binary, alwaysOn: alwaysOn)
        print("Installed. canon-webcam now starts at login and restarts itself if anything goes wrong.")
        print("Switched off Canon's EOS Webcam Utility services so they stop grabbing the camera.")
        print("Log: \(LaunchAgent.logFile.path)")
        print("`canon-webcam` is linked into \(Paths.commandLink.deletingLastPathComponent().path), so it works from any terminal.")
        return 0
    } catch {
        printError("\(error)")
        return 1
    }
}

func uninstallAgent(restoreCanon: Bool) -> Int32 {
    do {
        try LaunchAgent.uninstall(restoreCanonServices: restoreCanon)
        print("Uninstalled. Everything canon-webcam put on this Mac is gone.")
        if restoreCanon {
            print("Canon's EOS Webcam Utility comes back the next time you log in.")
        } else if LaunchAgent.canonUtilityIsPresent {
            print("Canon's EOS Webcam Utility stays off. Run with --restore-canon to bring it back.")
        }
        print("OBS stays installed. Drag it to the Trash if you don't need it anymore.")
        return 0
    } catch {
        printError("\(error)")
        return 1
    }
}

func runBenchmark(seconds: Double) -> Int32 {
    do {
        try benchmarkLiveView(seconds: seconds) { print($0) }
        return 0
    } catch {
        printError("\(error)")
        return 1
    }
}

/// Set from a signal handler queue, read by the engine loop.
final class StopFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var raised = false

    func raise() {
        lock.withLock { raised = true }
    }

    func isRaised() -> Bool {
        lock.withLock { raised }
    }
}
