import CanonUSB
import Foundation

public struct Check: Equatable, Sendable {
    public enum Verdict: String, Sendable {
        case ok
        case info
        case warning
        case missing
    }

    public var verdict: Verdict
    public var message: String

    public init(_ verdict: Verdict, _ message: String) {
        self.verdict = verdict
        self.message = message
    }
}

/// Everything that has to line up for the webcam to work, checked in the order you'd fix it.
public enum Doctor {
    static let obsApp = "/Applications/OBS.app"

    public static func runChecks() -> [Check] {
        [
            obsInstalled(),
            virtualCameraInstalled(),
            obsNotRunning(),
            cameraPluggedIn(),
            canonServicesOff(),
            loginAgent(),
            engineRunning(),
        ]
    }

    public static func isHealthy(_ checks: [Check]) -> Bool {
        !checks.contains { $0.verdict == .missing }
    }

    static func obsInstalled() -> Check {
        FileManager.default.fileExists(atPath: obsApp)
            ? Check(.ok, "OBS is installed (we only borrow its camera driver, OBS itself never has to run)")
            : Check(.missing, "OBS isn't installed. Get it with: brew install --cask obs")
    }

    static func virtualCameraInstalled() -> Check {
        CMIO.revealCameraExtensions()
        return CMIO.device(named: VirtualCamera.deviceName) != nil
            ? Check(.ok, "\(VirtualCamera.deviceName) is installed and approved")
            : Check(.missing, "\(VirtualCamera.deviceName) isn't set up yet. Open OBS, click Start Virtual Camera once, approve it in System Settings > General > Login Items & Extensions, then quit OBS.")
    }

    static func obsNotRunning() -> Check {
        run("/usr/bin/pgrep", ["-x", "OBS"]).status == 0
            ? Check(.warning, "OBS is open. If its own virtual camera is on, it fights us for the same camera. Quit it, we don't need it.")
            : Check(.ok, "OBS isn't running, good")
    }

    static func cameraPluggedIn() -> Check {
        USB.canonIsPluggedIn()
            ? Check(.ok, "A Canon camera is plugged in")
            : Check(.missing, "No Canon camera on USB. Plug it in and turn it on.")
    }

    static func canonServicesOff() -> Check {
        LaunchAgent.canonServicesDisabled()
            ? Check(.ok, "Canon's EOS Webcam Utility services are switched off")
            : Check(.warning, "Canon's EOS Webcam Utility services may grab the camera at login. `canon-webcam install` switches them off.")
    }

    static func loginAgent() -> Check {
        LaunchAgent.isInstalled
            ? Check(.info, "Starts at login (log: \(LaunchAgent.logFile.path))")
            : Check(.info, "Doesn't start at login yet. Run `canon-webcam install` for that.")
    }

    static func engineRunning() -> Check {
        guard let response = try? ControlClient.send(.status), let status = response.status else {
            return Check(.info, "The engine isn't running right now")
        }
        return Check(.info, "The engine is running and \(status.state)")
    }
}
