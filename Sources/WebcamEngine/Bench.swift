import CanonPTP
import CanonUSB
import Foundation

/// Measures how fast the camera really hands out live view frames.
public func benchmarkLiveView(seconds: Double, log: (String) -> Void) throws {
    freeCameraFromMacOS()
    guard let usb = try USB.openCanon() else { throw EngineError.noCamera }
    let camera = CanonCamera(transport: usb)
    try camera.connect()
    defer { camera.disconnect() }
    log("Connected to \(camera.model) with \(camera.state.lensName ?? "an unknown lens")")
    try camera.startLiveView()

    let start = Date()
    var frames = 0
    var firstFrameAt: Date?
    var lastFrameAt = start
    var worstGap = 0.0
    var bytes = 0
    while Date().timeIntervalSince(start) < seconds {
        guard let jpeg = try camera.nextFrame() else {
            usleep(1000)
            continue
        }
        let now = Date()
        if firstFrameAt == nil { firstFrameAt = now } else if frames > 10 { worstGap = max(worstGap, now.timeIntervalSince(lastFrameAt)) }
        lastFrameAt = now
        frames += 1
        bytes += jpeg.count
    }
    guard let firstFrameAt, frames > 1 else { throw EngineError.noFrames }
    let fps = Double(frames - 1) / lastFrameAt.timeIntervalSince(firstFrameAt)
    log(String(format: "First frame after %.2fs, then %.1f fps, worst gap %.0fms, %dKB per frame",
               firstFrameAt.timeIntervalSince(start), fps, worstGap * 1000, bytes / frames / 1024))
}

public enum EngineError: Error, CustomStringConvertible {
    case noCamera
    case noFrames
    case stalled

    public var description: String {
        switch self {
        case .noCamera: return "No Canon camera found. Plug it in over USB and turn it on."
        case .noFrames: return "The camera connected but never sent a live view frame."
        case .stalled: return "live view stopped sending frames"
        }
    }
}
