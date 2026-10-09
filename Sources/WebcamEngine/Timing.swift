import Foundation

/// Decides how long to wait before asking the camera for the next frame.
/// Asking too early just earns a "not ready" and burns USB time, asking late adds lag.
public struct FramePacer {
    /// The T7 makes a frame every 41.7ms, but moving one over USB eats about 14ms of that,
    /// so counting from the end of the last transfer the next frame lands around 27ms later
    public var earliestAsk: TimeInterval = 0.020
    public var retryDelay: TimeInterval = 0.002

    public init() {}

    public func waitBeforeAsking(sinceLastFrame elapsed: TimeInterval) -> TimeInterval {
        max(earliestAsk - elapsed, 0)
    }
}

/// Smooths frame intervals into a frames per second number that doesn't jitter in `status`.
public struct FrameRateMeter {
    public private(set) var framesPerSecond: Double = 0
    private var lastFrame: Date?
    private let smoothing = 0.1

    public init() {}

    public mutating func frameArrived(at time: Date) {
        defer { lastFrame = time }
        guard let lastFrame else { return }
        let interval = time.timeIntervalSince(lastFrame)
        guard interval > 0 else { return }
        let instant = 1 / interval
        framesPerSecond = framesPerSecond == 0 ? instant : framesPerSecond + (instant - framesPerSecond) * smoothing
    }

    public mutating func reset() {
        self = FrameRateMeter()
    }
}

/// Decides when it's safe to check whether an app is still watching.
/// The check pauses our feed for a moment, and OBS's driver paints its logo into any pause an app can see,
/// so we only check when nobody could notice: right after a call ends, or while the person is away.
public struct ProbePolicy {
    public let afterCallDelay: TimeInterval
    public let shortestAwayInterval: TimeInterval
    public let longestAwayInterval: TimeInterval
    public private(set) var awayInterval: TimeInterval

    private var wasOnACall = false
    private var callEndedAt: Date?
    private var nextAwayProbe: Date

    public init(
        startingAt time: Date, afterCallDelay: TimeInterval = 30,
        shortestAwayInterval: TimeInterval = 60, longestAwayInterval: TimeInterval = 900
    ) {
        self.afterCallDelay = afterCallDelay
        self.shortestAwayInterval = shortestAwayInterval
        self.longestAwayInterval = longestAwayInterval
        awayInterval = shortestAwayInterval
        nextAwayProbe = time
    }

    public mutating func shouldProbe(_ situation: Situation, at time: Date) -> Bool {
        if situation.onACall {
            wasOnACall = true
            callEndedAt = nil
            return false
        }
        if wasOnACall {
            wasOnACall = false
            callEndedAt = time
        }
        if let callEndedAt, time.timeIntervalSince(callEndedAt) >= afterCallDelay {
            return true
        }
        return situation.nobodyLooking && time >= nextAwayProbe
    }

    /// Something is still watching, so wait longer before bothering it again.
    public mutating func probeFoundSomeoneWatching(at time: Date) {
        callEndedAt = nil
        nextAwayProbe = time.addingTimeInterval(awayInterval)
        awayInterval = min(awayInterval * 2, longestAwayInterval)
    }
}

/// Fires at most once per interval. Used for event polling, keep alive and usage checks.
public struct Every {
    public let interval: TimeInterval
    private var lastFired: Date?

    public init(_ interval: TimeInterval) {
        self.interval = interval
    }

    public mutating func isDue(at time: Date) -> Bool {
        if let lastFired, time.timeIntervalSince(lastFired) < interval { return false }
        lastFired = time
        return true
    }

    public mutating func reset() {
        lastFired = nil
    }
}

/// Opt in timing breakdown (CANON_WEBCAM_DEBUG=1) for when frames feel slow.
public struct TimingStats {
    public enum Bucket: String, CaseIterable {
        case ask
        case render
        case events
        case notReady = "not ready"
    }

    public let enabled: Bool
    private var totals: [Bucket: TimeInterval] = [:]
    private var counts: [Bucket: Int] = [:]
    private var report = Every(5)

    public init(enabled: Bool) {
        self.enabled = enabled
        _ = report.isDue(at: Date())
    }

    public mutating func record(_ bucket: Bucket, since start: Date) {
        guard enabled else { return }
        totals[bucket, default: 0] += Date().timeIntervalSince(start)
        counts[bucket, default: 0] += 1
    }

    public mutating func count(_ bucket: Bucket) {
        guard enabled else { return }
        counts[bucket, default: 0] += 1
    }

    public mutating func summaryIfDue(at time: Date) -> String? {
        guard enabled, report.isDue(at: time) else { return nil }
        defer { totals.removeAll(); counts.removeAll() }
        let parts = Bucket.allCases.map { bucket -> String in
            let count = counts[bucket] ?? 0
            guard let total = totals[bucket], count > 0 else { return "\(bucket.rawValue) x\(count)" }
            return String(format: "%@ x%d avg %.1fms", bucket.rawValue, count, total / Double(count) * 1000)
        }
        return "timing: " + parts.joined(separator: ", ")
    }
}
