import CanonUSB
import Foundation

/// The facts setup cares about, shared by `doctor` and the installer app.
public enum SystemChecks {
    public static let obsAppPath = "/Applications/OBS.app"

    public static var obsIsInstalled: Bool {
        FileManager.default.fileExists(atPath: obsAppPath)
    }

    public static var virtualCameraIsReady: Bool {
        CMIO.revealCameraExtensions()
        return CMIO.device(named: VirtualCamera.deviceName) != nil
    }

    public static var obsIsRunning: Bool {
        run("/usr/bin/pgrep", ["-x", "OBS"]).status == 0
    }

    public static var cameraIsPluggedIn: Bool {
        USB.canonIsPluggedIn()
    }

    public static var isInstalled: Bool {
        LaunchAgent.isInstalled && FileManager.default.fileExists(atPath: Paths.installedBinary.path)
    }
}
