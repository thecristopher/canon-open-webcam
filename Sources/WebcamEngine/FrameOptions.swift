import Foundation

/// Everything we do to the picture on the Mac side, as opposed to settings the camera keeps itself.
public struct FrameOptions: Codable, Equatable, Sendable {
    /// Apps already mirror your self view, so mirroring here flips what everyone else sees.
    public var mirror = false
    /// Digital zoom, 1 means the full live view.
    public var zoom = 1.0
    /// Where the zoomed frame sits, from -1 (left or bottom edge) to 1 (right or top edge).
    public var panX = 0.0
    public var panY = 0.0

    // Picture adjustments all run from -1 to 1 with 0 meaning untouched
    public var brightness = 0.0
    public var contrast = 0.0
    public var saturation = 0.0
    public var warmth = 0.0
    public var tint = 0.0
    /// How much crispness to add back after upscaling, from 0 to 1.
    public var sharpness = 0.4

    /// Frames per second sent to apps. 0 means every camera frame as it arrives (24 on the T7).
    public var frameRate = 0

    public static let zoomRange = 1.0...3.0
    public static let adjustmentRange = -1.0...1.0
    public static let frameRates = [0, 30, 60]

    public init(mirror: Bool = false, zoom: Double = 1.0) {
        self.mirror = mirror
        self.zoom = zoom
    }

    /// True when the colors are untouched, so the renderer can skip the filters entirely.
    public var hasColorAdjustments: Bool {
        brightness != 0 || contrast != 0 || saturation != 0 || warmth != 0 || tint != 0
    }

    // Older options.json files only had mirror and zoom, so every field falls back to its default
    public init(from decoder: Decoder) throws {
        let saved = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = FrameOptions()
        mirror = try saved.decodeIfPresent(Bool.self, forKey: .mirror) ?? defaults.mirror
        zoom = try saved.decodeIfPresent(Double.self, forKey: .zoom) ?? defaults.zoom
        panX = try saved.decodeIfPresent(Double.self, forKey: .panX) ?? defaults.panX
        panY = try saved.decodeIfPresent(Double.self, forKey: .panY) ?? defaults.panY
        brightness = try saved.decodeIfPresent(Double.self, forKey: .brightness) ?? defaults.brightness
        contrast = try saved.decodeIfPresent(Double.self, forKey: .contrast) ?? defaults.contrast
        saturation = try saved.decodeIfPresent(Double.self, forKey: .saturation) ?? defaults.saturation
        warmth = try saved.decodeIfPresent(Double.self, forKey: .warmth) ?? defaults.warmth
        tint = try saved.decodeIfPresent(Double.self, forKey: .tint) ?? defaults.tint
        sharpness = try saved.decodeIfPresent(Double.self, forKey: .sharpness) ?? defaults.sharpness
        frameRate = try saved.decodeIfPresent(Int.self, forKey: .frameRate) ?? defaults.frameRate
    }
}

// MARK: Changing options by name, the way the CLI and the app ask for it

extension FrameOptions {
    public static let adjustableNames = [
        "mirror", "zoom", "pan-x", "pan-y",
        "brightness", "contrast", "saturation", "warmth", "tint", "sharpness",
        "fps", "picture", "framing",
    ]

    /// Applies one change like ("brightness", "0.2") or ("picture", "reset"), or explains what's wrong with it.
    public func applying(_ name: String, _ value: String) -> Result<FrameOptions, ControlError> {
        var changed = self
        switch name {
        case "mirror":
            guard let on = Self.parseSwitch(value) else { return .failure(.unknown("mirror takes on or off")) }
            changed.mirror = on
        case "zoom":
            guard let zoom = Double(value.replacingOccurrences(of: "x", with: "")), Self.zoomRange.contains(zoom) else {
                return .failure(.unknown("zoom goes from 1 to 3, like 1.5"))
            }
            changed.zoom = zoom
        case "pan-x", "pan-y", "brightness", "contrast", "saturation", "warmth", "tint":
            guard let amount = Double(value), Self.adjustmentRange.contains(amount) else {
                return .failure(.unknown("\(name) goes from -1 to 1, like 0.2"))
            }
            changed.setAdjustment(name, to: amount)
        case "sharpness":
            guard let amount = Double(value), (0.0...1.0).contains(amount) else {
                return .failure(.unknown("sharpness goes from 0 to 1"))
            }
            changed.sharpness = amount
        case "fps":
            let rate = value == "camera" ? 0 : Int(value) ?? -1
            guard Self.frameRates.contains(rate) else { return .failure(.unknown("fps takes camera, 30 or 60")) }
            changed.frameRate = rate
        case "picture" where value == "reset":
            changed.resetPicture()
        case "framing" where value == "reset":
            changed.resetFraming()
        default:
            return .failure(.unknown("\"\(name)\" isn't something I can change"))
        }
        return .success(changed)
    }

    /// A short confirmation for the log and the CLI.
    public func describe(_ name: String) -> String {
        switch name {
        case "mirror": return "Mirror is \(mirror ? "on" : "off")."
        case "zoom": return String(format: "Zoom is %.1fx.", zoom)
        case "fps": return frameRate == 0 ? "Sending every camera frame as it arrives." : "Sending \(frameRate) frames per second."
        case "picture": return "Picture adjustments are back to normal."
        case "framing": return "Framing is back to the full picture."
        default: return "\(name) updated."
        }
    }

    public mutating func resetPicture() {
        let defaults = FrameOptions()
        brightness = defaults.brightness
        contrast = defaults.contrast
        saturation = defaults.saturation
        warmth = defaults.warmth
        tint = defaults.tint
        sharpness = defaults.sharpness
    }

    public mutating func resetFraming() {
        zoom = 1
        panX = 0
        panY = 0
    }

    private mutating func setAdjustment(_ name: String, to amount: Double) {
        switch name {
        case "pan-x": panX = amount
        case "pan-y": panY = amount
        case "brightness": brightness = amount
        case "contrast": contrast = amount
        case "saturation": saturation = amount
        case "warmth": warmth = amount
        case "tint": tint = amount
        default: break
        }
    }

    static func parseSwitch(_ value: String) -> Bool? {
        switch value.lowercased() {
        case "on", "true", "yes", "1": return true
        case "off", "false", "no", "0": return false
        default: return nil
        }
    }
}
