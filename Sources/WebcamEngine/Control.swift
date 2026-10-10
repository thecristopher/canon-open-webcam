import CanonPTP
import Foundation

/// What the CLI can ask a running engine to do. One JSON object per line over the control socket.
public struct ControlRequest: Codable, Equatable, Sendable {
    public var command: String
    public var name: String?
    public var value: String?

    public init(command: String, name: String? = nil, value: String? = nil) {
        self.command = command
        self.name = name
        self.value = value
    }

    public static let status = ControlRequest(command: "status")
    public static let recheck = ControlRequest(command: "recheck")

    public static func set(_ name: String, to value: String) -> ControlRequest {
        ControlRequest(command: "set", name: name, value: value)
    }

    public static func focus(_ action: String, steps: Int = 1) -> ControlRequest {
        ControlRequest(command: "focus", name: action, value: String(steps))
    }
}

public struct ControlResponse: Codable, Equatable, Sendable {
    public var ok: Bool
    public var message: String?
    public var status: EngineStatus?

    public static func success(_ message: String? = nil, status: EngineStatus? = nil) -> ControlResponse {
        ControlResponse(ok: true, message: message, status: status)
    }

    public static func failure(_ message: String) -> ControlResponse {
        ControlResponse(ok: false, message: message, status: nil)
    }
}

public struct SettingStatus: Codable, Equatable, Sendable {
    public var value: String
    public var choices: [String]

    public init(value: String, choices: [String]) {
        self.value = value
        self.choices = choices
    }
}

/// Where the engine is at, in a form an app can switch on instead of parsing English.
public enum PhaseKind: String, Codable, Sendable {
    case idle
    case waitingForCamera
    case starting
    case live
    case trouble
}

/// A snapshot of the engine for `canon-webcam status` and the menu bar app.
public struct EngineStatus: Codable, Equatable, Sendable {
    public var state: String
    public var kind: PhaseKind
    public var trouble: String?
    public var model: String?
    public var lens: String?
    public var framesPerSecond: Double
    public var appsWatching: Bool
    public var options: FrameOptions
    public var settings: [String: SettingStatus]

    public init(
        state: String, kind: PhaseKind = .idle, trouble: String? = nil, model: String? = nil, lens: String? = nil,
        framesPerSecond: Double = 0, appsWatching: Bool = false, options: FrameOptions = FrameOptions(),
        settings: [String: SettingStatus] = [:]
    ) {
        self.state = state
        self.kind = kind
        self.trouble = trouble
        self.model = model
        self.lens = lens
        self.framesPerSecond = framesPerSecond
        self.appsWatching = appsWatching
        self.options = options
        self.settings = settings
    }
}

/// The work a control request turns into, run on the camera thread between frames.
public enum EngineCommand: Equatable, Sendable {
    case setCamera(CameraSetting, String)
    case setOption(String, String)
    case autofocus
    case focus(LensDrive)

    /// Turns a request into a command, or explains what's wrong with it.
    public static func from(_ request: ControlRequest) -> Result<EngineCommand, ControlError> {
        switch request.command {
        case "set":
            return setCommand(name: request.name ?? "", value: request.value ?? "")
        case "focus":
            return focusCommand(action: request.name ?? "auto", steps: Int(request.value ?? "1") ?? 1)
        default:
            return .failure(.unknown("I don't know the command \"\(request.command)\""))
        }
    }

    private static func setCommand(name: String, value: String) -> Result<EngineCommand, ControlError> {
        if FrameOptions.adjustableNames.contains(name) {
            // Check it against defaults now, so a typo fails here instead of on the camera thread
            return FrameOptions().applying(name, value).map { _ in .setOption(name, value) }
        }
        guard let setting = CameraSetting(rawValue: name) else {
            let names = (CameraSetting.allCases.map(\.rawValue) + FrameOptions.adjustableNames).joined(separator: ", ")
            return .failure(.unknown("\"\(name)\" isn't a setting. Try one of: \(names)"))
        }
        return .success(.setCamera(setting, value))
    }

    private static func focusCommand(action: String, steps: Int) -> Result<EngineCommand, ControlError> {
        // Canon only knows three step sizes, so 1, 2 and 3 pick small, medium and large
        let size = max(1, min(steps, 3)) - 1
        switch action {
        case "auto": return .success(.autofocus)
        case "near": return .success(.focus([.nearSmall, .nearMedium, .nearLarge][size]))
        case "far": return .success(.focus([.farSmall, .farMedium, .farLarge][size]))
        default: return .failure(.unknown("focus takes auto, near or far"))
        }
    }
}

public enum ControlError: Error, Equatable, CustomStringConvertible {
    case unknown(String)

    public var description: String {
        switch self {
        case .unknown(let message): return message
        }
    }
}
