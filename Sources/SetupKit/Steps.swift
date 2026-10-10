import Foundation

/// What we know about this Mac, refreshed every second while the installer is open.
public struct InstallerFacts: Equatable, Sendable {
    public var obsInstalled: Bool
    public var driverReady: Bool
    public var installed: Bool
    public var cameraConnected: Bool
    public var hasHomebrew: Bool

    public init(obsInstalled: Bool, driverReady: Bool, installed: Bool, cameraConnected: Bool, hasHomebrew: Bool) {
        self.obsInstalled = obsInstalled
        self.driverReady = driverReady
        self.installed = installed
        self.cameraConnected = cameraConnected
        self.hasHomebrew = hasHomebrew
    }
}

/// The one slow thing the installer can be busy with at a time.
public enum Activity: Equatable, Sendable {
    case idle
    case installingOBS
    case waitingForApproval
    case installing
}

public enum InstallerStep: CaseIterable, Sendable {
    case driver
    case install
    case camera

    public var title: String {
        switch self {
        case .driver: return "Camera driver"
        case .install: return "Canon Webcam"
        case .camera: return "Your camera"
        }
    }
}

/// How a row in the checklist looks and what its button says.
public enum StepStatus: Equatable, Sendable {
    case done(String)
    case needsAction(button: String, detail: String)
    case working(String)
    case waitingOn(String)
}

public enum Checklist {
    public static func status(of step: InstallerStep, facts: InstallerFacts, activity: Activity) -> StepStatus {
        switch step {
        case .driver: return driverStatus(facts, activity)
        case .install: return installStatus(facts, activity)
        case .camera: return cameraStatus(facts)
        }
    }

    /// Everything that matters is in place, time for the happy screen.
    public static func isFinished(_ facts: InstallerFacts) -> Bool {
        facts.driverReady && facts.installed
    }

    private static func driverStatus(_ facts: InstallerFacts, _ activity: Activity) -> StepStatus {
        if facts.driverReady { return .done("OBS Virtual Camera is ready") }
        switch activity {
        case .installingOBS:
            return .working("Installing OBS. This takes a minute or two.")
        case .waitingForApproval:
            return .working("In System Settings, switch on OBS Virtual Camera. This window notices on its own.")
        default:
            break
        }
        if !facts.obsInstalled {
            return .needsAction(
                button: facts.hasHomebrew ? "Install OBS" : "Download OBS",
                detail: "We borrow the camera driver that comes with OBS. OBS itself never has to run."
            )
        }
        return .needsAction(
            button: "Set Up Driver",
            detail: "OBS opens for a moment and macOS asks you to allow its camera. One time only."
        )
    }

    private static func installStatus(_ facts: InstallerFacts, _ activity: Activity) -> StepStatus {
        if activity == .installing { return .working("Installing") }
        if facts.installed { return .done("Installed, and starts by itself when you log in") }
        if !facts.driverReady { return .waitingOn("Set up the camera driver first") }
        return .needsAction(button: "Install", detail: "Runs quietly in the background and wakes the camera only when an app needs it.")
    }

    private static func cameraStatus(_ facts: InstallerFacts) -> StepStatus {
        facts.cameraConnected
            ? .done("Canon camera connected")
            : .waitingOn("Plug in your Canon over USB and turn it on. You can do this any time.")
    }
}
