import Foundation
import WebcamEngine

/// What the menu bar panel should say about the engine, boiled down from its status.
public enum PanelState: Equatable, Sendable {
    case notRunning
    case resting
    case waitingForCamera
    case starting
    case live(model: String, detail: String)
    case trouble(String)

    public static func from(_ status: EngineStatus?) -> PanelState {
        guard let status else { return .notRunning }
        switch status.kind {
        case .idle: return .resting
        case .waitingForCamera: return .waitingForCamera
        case .starting: return .starting
        case .trouble: return .trouble(status.trouble ?? "Something went wrong")
        case .live:
            let fps = status.framesPerSecond > 0 ? String(format: "%.0f fps", status.framesPerSecond) : nil
            let detail = [status.lens, fps].compactMap { $0 }.joined(separator: " · ")
            return .live(model: status.model ?? "Canon camera", detail: detail)
        }
    }

    public var title: String {
        switch self {
        case .notRunning: return "Not running"
        case .resting: return "Resting"
        case .waitingForCamera: return "Waiting for your camera"
        case .starting: return "Waking up"
        case .live(let model, _): return model
        case .trouble: return "Can't reach the camera"
        }
    }

    public var subtitle: String {
        switch self {
        case .notRunning: return "Finish setup to start using your Canon"
        case .resting: return "Wakes up the moment an app opens OBS Virtual Camera"
        case .waitingForCamera: return "Plug in the Canon over USB and turn it on"
        case .starting: return "Live view takes a second or two"
        case .live(_, let detail): return detail.isEmpty ? "Live" : detail
        case .trouble(let reason): return reason
        }
    }

    /// The menu bar icon: outlined while resting, filled while live, a warning when stuck.
    public var menuBarSymbol: String {
        switch self {
        case .live: return "web.camera.fill"
        case .trouble, .notRunning: return "exclamationmark.triangle"
        default: return "web.camera"
        }
    }

    public var isLive: Bool {
        if case .live = self { return true }
        return false
    }
}

/// How one camera setting can be shown: a real picker, locked by the mode dial, or not known yet.
public enum SettingControl: Equatable, Sendable {
    case unavailable
    case lockedByMode(current: String)
    case choices([String], current: String)

    public static func from(_ setting: SettingStatus?) -> SettingControl {
        guard let setting else { return .unavailable }
        if setting.choices.isEmpty { return .lockedByMode(current: setting.value) }
        return .choices(setting.choices, current: setting.value)
    }
}
