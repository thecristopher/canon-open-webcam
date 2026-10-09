import CoreGraphics
import Foundation
import IOKit.pwr_mgt

/// What the person at the Mac is up to, as far as deciding whether a logo blink could be seen.
public enum UserPresence {
    public static func secondsSinceInput() -> TimeInterval {
        // ~0 is kCGAnyInputEventType: keyboard, mouse, trackpad, anything
        CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: CGEventType(rawValue: ~0)!)
    }

    public static func screenIsLocked() -> Bool {
        guard let session = CGSessionCopyCurrentDictionary() as? [String: Any] else { return false }
        return session["CGSSessionScreenIsLocked"] as? Bool == true
    }

    public static func displayIsAsleep() -> Bool {
        CGDisplayIsAsleep(CGMainDisplayID()) != 0
    }

    /// Zoom, Meet, Teams and FaceTime hold one of these for the whole call.
    public static func anAppKeepsTheDisplayAwake() -> Bool {
        var assertions: Unmanaged<CFDictionary>?
        guard IOPMCopyAssertionsByProcess(&assertions) == kIOReturnSuccess,
              let byProcess = assertions?.takeRetainedValue() as? [NSNumber: [[String: Any]]]
        else { return false }
        let ourPID = ProcessInfo.processInfo.processIdentifier
        let displayTypes: Set<String> = ["PreventUserIdleDisplaySleep", "NoDisplaySleepAssertion"]
        return byProcess.contains { pid, list in
            pid.int32Value != ourPID && list.contains { displayTypes.contains($0["AssertType"] as? String ?? "") }
        }
    }

    public static func currentSituation() -> Situation {
        Situation(
            onACall: MicMonitor.anyMicInUse() || anAppKeepsTheDisplayAwake(),
            nobodyLooking: screenIsLocked() || displayIsAsleep() || secondsSinceInput() >= Situation.awayAfter
        )
    }
}

public struct Situation: Equatable, Sendable {
    /// No keyboard or mouse for this long means nobody is looking at a preview.
    public static let awayAfter: TimeInterval = 300

    public var onACall: Bool
    public var nobodyLooking: Bool

    public init(onACall: Bool, nobodyLooking: Bool) {
        self.onACall = onACall
        self.nobodyLooking = nobodyLooking
    }
}
