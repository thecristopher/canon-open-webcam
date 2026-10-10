import Foundation

/// Lets the installer and uninstaller run without a window, for scripts and for testing the real thing.
public enum Headless {
    /// Runs the job, prints how it went and exits, before any window has a chance to appear.
    public static func run(_ what: String, _ job: @escaping @Sendable () async throws -> Void) -> Never {
        let done = DispatchSemaphore(value: 0)
        let failure = FailureBox()
        Task.detached {
            do { try await job() } catch { failure.message = "\(error)" }
            done.signal()
        }
        done.wait()
        if let message = failure.message {
            FileHandle.standardError.write(Data("\(what) failed: \(message)\n".utf8))
            exit(1)
        }
        print("\(what) finished.")
        exit(0)
    }

    private final class FailureBox: @unchecked Sendable {
        var message: String?
    }
}
