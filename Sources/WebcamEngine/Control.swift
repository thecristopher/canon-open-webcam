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
}

/// A snapshot of the engine for `canon-webcam status`.
public struct EngineStatus: Codable, Equatable, Sendable {
    public var state: String
    public var model: String?
    public var lens: String?
    public var framesPerSecond: Double
    public var appsWatching: Bool
    public var mirror: Bool
    public var zoom: Double
    public var settings: [String: SettingStatus]

    public init(
        state: String, model: String? = nil, lens: String? = nil, framesPerSecond: Double = 0,
        appsWatching: Bool = false, options: FrameOptions = FrameOptions(), settings: [String: SettingStatus] = [:]
    ) {
        self.state = state
        self.model = model
        self.lens = lens
        self.framesPerSecond = framesPerSecond
        self.appsWatching = appsWatching
        self.mirror = options.mirror
        self.zoom = options.zoom
        self.settings = settings
    }
}

/// The work a control request turns into, run on the camera thread between frames.
public enum EngineCommand: Equatable, Sendable {
    case setCamera(CameraSetting, String)
    case setMirror(Bool)
    case setZoom(Double)
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
        switch name {
        case "mirror":
            guard let on = parseSwitch(value) else { return .failure(.unknown("mirror takes on or off")) }
            return .success(.setMirror(on))
        case "zoom":
            guard let zoom = Double(value.replacingOccurrences(of: "x", with: "")), FrameOptions.zoomRange.contains(zoom) else {
                return .failure(.unknown("zoom goes from 1 to 3, like 1.5"))
            }
            return .success(.setZoom(zoom))
        default:
            guard let setting = CameraSetting(rawValue: name) else {
                let names = (CameraSetting.allCases.map(\.rawValue) + ["mirror", "zoom"]).joined(separator: ", ")
                return .failure(.unknown("\"\(name)\" isn't a setting. Try one of: \(names)"))
            }
            return .success(.setCamera(setting, value))
        }
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

    private static func parseSwitch(_ value: String) -> Bool? {
        switch value.lowercased() {
        case "on", "true", "yes", "1": return true
        case "off", "false", "no", "0": return false
        default: return nil
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
