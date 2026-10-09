import Foundation

/// What someone typed after `canon-webcam`.
public enum CLICommand: Equatable, Sendable {
    case start(alwaysOn: Bool)
    case status
    case set(name: String, value: String)
    case focus(action: String, steps: Int)
    case doctor
    case install(alwaysOn: Bool)
    case uninstall
    case bench(seconds: Double)
    case help

    public static let usage = """
    canon-webcam: your Canon EOS as a webcam, no subscription required

      canon-webcam [start] [--always-on]   run it (the camera wakes when an app opens it)
      canon-webcam status                  what the camera is doing, plus its settings
      canon-webcam set <setting> <value>   iso, aperture, shutter, exposure, white-balance,
                                           color-temp, mirror (on/off), zoom (1 to 3)
      canon-webcam focus [auto|near|far] [1-3]
      canon-webcam doctor                  check everything is set up
      canon-webcam install [--always-on]   start at login and stay running
      canon-webcam uninstall               stop starting at login
      canon-webcam bench [seconds]         measure the camera's live view frame rate
    """

    public static func parse(_ arguments: [String]) -> Result<CLICommand, ControlError> {
        let flags = Set(arguments.filter { $0.hasPrefix("--") })
        let words = arguments.filter { !$0.hasPrefix("--") }
        let alwaysOn = flags.contains("--always-on")
        if flags.contains("--help") { return .success(.help) }

        switch words.first {
        case nil, "start":
            return .success(.start(alwaysOn: alwaysOn))
        case "status":
            return .success(.status)
        case "set":
            guard words.count == 3 else { return .failure(.unknown("Usage: canon-webcam set <setting> <value>, like: set iso 400")) }
            return .success(.set(name: words[1], value: words[2]))
        case "focus":
            let action = words.count > 1 ? words[1] : "auto"
            let steps = words.count > 2 ? Int(words[2]) ?? 1 : 1
            return .success(.focus(action: action, steps: steps))
        case "doctor":
            return .success(.doctor)
        case "install":
            return .success(.install(alwaysOn: alwaysOn))
        case "uninstall":
            return .success(.uninstall)
        case "bench":
            let seconds = words.count > 1 ? Double(words[1]) ?? 8 : 8
            return .success(.bench(seconds: seconds))
        case "help", "-h":
            return .success(.help)
        case let other?:
            return .failure(.unknown("I don't know \"\(other)\".\n\n\(usage)"))
        }
    }
}
