import CoreVideo
import Foundation

/// Sends frames to the virtual camera on a steady beat (30 or 60 per second) on its own thread,
/// so the camera loop being busy with USB or rendering never makes the beat stumble.
final class SteadyOutput: @unchecked Sendable {
    private let renderer: FrameRenderer
    private let lock = NSLock()
    private var sink: VirtualCamera?
    private var framesPerSecond = 0
    private var latest: CVPixelBuffer?
    private var latestIsFresh = false

    init(renderer: FrameRenderer) {
        self.renderer = renderer
        let thread = Thread { [weak self] in self?.tickForever() }
        thread.name = "steady output"
        thread.qualityOfService = .userInteractive
        thread.start()
    }

    /// Zero turns the beat off, and the camera loop sends every frame itself.
    func configure(sink: VirtualCamera?, framesPerSecond: Int) {
        lock.withLock {
            self.sink = sink
            self.framesPerSecond = framesPerSecond
        }
    }

    func offer(_ frame: CVPixelBuffer) {
        lock.withLock {
            latest = frame
            latestIsFresh = true
        }
    }

    /// Nothing live anymore, so stop repeating the last picture.
    func clear() {
        lock.withLock {
            latest = nil
            latestIsFresh = false
        }
    }

    private func tickForever() {
        var nextTick = Date()
        while true {
            let rate = lock.withLock { framesPerSecond }
            guard rate > 0 else {
                Thread.sleep(forTimeInterval: 0.1)
                nextTick = Date()
                continue
            }
            nextTick = max(nextTick.addingTimeInterval(1 / Double(rate)), Date().addingTimeInterval(-0.05))
            Thread.sleep(until: nextTick)
            sendOneTick()
        }
    }

    private func sendOneTick() {
        let (sink, frame, fresh) = lock.withLock { () -> (VirtualCamera?, CVPixelBuffer?, Bool) in
            defer { latestIsFresh = false }
            return (self.sink, latest, latestIsFresh)
        }
        guard let sink, let frame else { return }
        // The virtual camera drops a buffer it was just handed, so repeats need their own copy
        guard let outgoing = fresh ? frame : renderer.copy(frame) else { return }
        sink.send(outgoing)
    }
}
