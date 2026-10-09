import Foundation

/// A camera setting a person would want to change mid call, with Canon's codes translated to words.
public enum CameraSetting: String, CaseIterable, Sendable {
    case iso
    case aperture
    case shutter
    case exposure
    case whiteBalance = "white-balance"
    case colorTemperature = "color-temp"

    public var property: CanonProperty {
        switch self {
        case .iso: return .iso
        case .aperture: return .aperture
        case .shutter: return .shutterSpeed
        case .exposure: return .exposureCompensation
        case .whiteBalance: return .whiteBalance
        case .colorTemperature: return .colorTemperature
        }
    }

    public func label(for code: UInt32) -> String {
        switch self {
        case .colorTemperature:
            return "\(code)K"
        case .aperture where code == 0:
            return "auto"
        case .aperture:
            return table.first { $0.code == code }.map { "f/\($0.label)" } ?? hex(code)
        default:
            return table.first { $0.code == code }?.label ?? hex(code)
        }
    }

    /// Accepts what a person would type: "400", "auto", "f/5.6", "1/60", "+1", "daylight", "5000K".
    public func code(for input: String, choices: [UInt32]) -> UInt32? {
        let typed = normalized(input)
        if self == .colorTemperature {
            return UInt32(typed.replacingOccurrences(of: "k", with: ""))
        }
        // Prefer codes the camera says it accepts, since tables repeat labels for 1/3 and 1/2 stops
        let candidates = table.filter { normalized($0.label) == typed }
        return candidates.first { choices.contains($0.code) }?.code ?? candidates.first?.code
    }

    private func normalized(_ text: String) -> String {
        var cleaned = text.lowercased().replacingOccurrences(of: " ", with: "")
        if cleaned.hasPrefix("f/") { cleaned.removeFirst(2) }
        if cleaned.hasPrefix("+") { cleaned.removeFirst() }
        if cleaned.hasPrefix("iso") { cleaned.removeFirst(3) }
        return cleaned
    }

    private func hex(_ code: UInt32) -> String {
        String(format: "0x%X", code)
    }

    private var table: [(label: String, code: UInt32)] {
        switch self {
        case .iso: return isoTable
        case .aperture: return apertureTable
        case .shutter: return shutterTable
        case .exposure: return exposureTable
        case .whiteBalance: return whiteBalanceTable
        case .colorTemperature: return []
        }
    }
}

// Tables lifted from libgphoto2's config.c, trimmed to what an EOS body reports

private let isoTable: [(label: String, code: UInt32)] = [
    ("auto", 0x00), ("100", 0x48), ("125", 0x4B), ("160", 0x4D), ("200", 0x50), ("250", 0x53),
    ("320", 0x55), ("400", 0x58), ("500", 0x5B), ("640", 0x5D), ("800", 0x60), ("1000", 0x63),
    ("1250", 0x65), ("1600", 0x68), ("2000", 0x6B), ("2500", 0x6D), ("3200", 0x70), ("4000", 0x73),
    ("5000", 0x75), ("6400", 0x78), ("8000", 0x7B), ("10000", 0x7D), ("12800", 0x80), ("25600", 0x88),
]

private let apertureTable: [(label: String, code: UInt32)] = [
    ("1", 0x08), ("1.1", 0x0B), ("1.2", 0x0C), ("1.2", 0x0D), ("1.4", 0x10), ("1.6", 0x13),
    ("1.8", 0x14), ("1.8", 0x15), ("2", 0x18), ("2.2", 0x1B), ("2.5", 0x1C), ("2.5", 0x1D),
    ("2.8", 0x20), ("3.2", 0x23), ("3.5", 0x24), ("3.5", 0x25), ("4", 0x28), ("4.5", 0x2B),
    ("4.5", 0x2C), ("5", 0x2D), ("5.6", 0x30), ("6.3", 0x33), ("6.7", 0x34), ("7.1", 0x35),
    ("8", 0x38), ("9", 0x3B), ("9.5", 0x3C), ("10", 0x3D), ("11", 0x40), ("13", 0x43),
    ("13", 0x44), ("14", 0x45), ("16", 0x48), ("18", 0x4B), ("19", 0x4C), ("20", 0x4D),
    ("22", 0x50), ("25", 0x53), ("27", 0x54), ("29", 0x55), ("32", 0x58), ("36", 0x5B),
]

private let shutterTable: [(label: String, code: UInt32)] = [
    ("auto", 0x00), ("30", 0x10), ("15", 0x18), ("8", 0x20), ("4", 0x28), ("2", 0x30), ("1", 0x38),
    ("0.5", 0x40), ("1/4", 0x48), ("1/5", 0x4B), ("1/6", 0x4C), ("1/8", 0x50), ("1/10", 0x53),
    ("1/10", 0x54), ("1/13", 0x55), ("1/15", 0x58), ("1/20", 0x5B), ("1/20", 0x5C), ("1/25", 0x5D),
    ("1/30", 0x60), ("1/40", 0x63), ("1/45", 0x64), ("1/50", 0x65), ("1/60", 0x68), ("1/80", 0x6B),
    ("1/90", 0x6C), ("1/100", 0x6D), ("1/125", 0x70), ("1/160", 0x73), ("1/180", 0x74),
    ("1/200", 0x75), ("1/250", 0x78), ("1/320", 0x7B), ("1/350", 0x7C), ("1/400", 0x7D),
    ("1/500", 0x80), ("1/640", 0x83), ("1/750", 0x84), ("1/800", 0x85), ("1/1000", 0x88),
    ("1/1250", 0x8B), ("1/1500", 0x8C), ("1/1600", 0x8D), ("1/2000", 0x90), ("1/2500", 0x93),
    ("1/3000", 0x94), ("1/3200", 0x95), ("1/4000", 0x98),
]

private let exposureTable: [(label: String, code: UInt32)] = [
    ("3", 0x18), ("2.6", 0x15), ("2.5", 0x14), ("2.3", 0x13), ("2", 0x10), ("1.6", 0x0D),
    ("1.5", 0x0C), ("1.3", 0x0B), ("1", 0x08), ("0.6", 0x05), ("0.5", 0x04), ("0.3", 0x03),
    ("0", 0x00), ("-0.3", 0xFD), ("-0.5", 0xFC), ("-0.6", 0xFB), ("-1", 0xF8), ("-1.3", 0xF5),
    ("-1.5", 0xF4), ("-1.6", 0xF3), ("-2", 0xF0), ("-2.3", 0xED), ("-2.5", 0xEC), ("-2.6", 0xEB),
    ("-3", 0xE8),
]

private let whiteBalanceTable: [(label: String, code: UInt32)] = [
    ("auto", 0), ("daylight", 1), ("cloudy", 2), ("tungsten", 3), ("fluorescent", 4),
    ("flash", 5), ("custom", 6), ("shade", 8), ("kelvin", 9), ("auto-white", 23),
]
