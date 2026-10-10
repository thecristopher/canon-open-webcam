import Foundation
import SwiftUI

/// Drives the installer window: keeps the facts fresh and runs whatever button was clicked.
@MainActor
public final class InstallerModel: ObservableObject {
    @Published public private(set) var facts: InstallerFacts?
    @Published public private(set) var activity = Activity.idle
    @Published public var problem: String?

    private let readFacts: @Sendable () -> InstallerFacts
    private var refresher: Timer?
    private var reading = false

    public init(readFacts: @escaping @Sendable () -> InstallerFacts = SetupActions.currentFacts, startingWith facts: InstallerFacts? = nil) {
        self.readFacts = readFacts
        self.facts = facts
    }

    public func startWatching() {
        refresh()
        refresher = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    public func status(of step: InstallerStep) -> StepStatus? {
        facts.map { Checklist.status(of: step, facts: $0, activity: activity) }
    }

    public var isFinished: Bool {
        facts.map(Checklist.isFinished) ?? false
    }

    public func act(on step: InstallerStep) {
        problem = nil
        switch step {
        case .driver: setUpDriver()
        case .install: install()
        case .camera: break
        }
    }

    public func install() {
        run(.installing) { try await SetupActions.install() }
    }

    public func reopenSettings() {
        SetupActions.openExtensionSettings()
    }

    public func previewClosed() {
        SetupActions.askEngineToRecheck()
    }

    private func setUpDriver() {
        guard let facts else { return }
        if facts.obsInstalled {
            activity = .waitingForApproval
            SetupActions.startDriverApproval()
        } else {
            run(.installingOBS) { try await SetupActions.getOBS() }
        }
    }

    /// Runs a slow action with the matching spinner, and turns failures into a friendly message.
    private func run(_ busy: Activity, _ work: @escaping () async throws -> Void) {
        activity = busy
        Task {
            do {
                try await work()
            } catch {
                problem = "\(error)"
            }
            activity = .idle
            refresh()
        }
    }

    private func refresh() {
        // CoreMediaIO and USB lookups can take a moment, so keep them off the main thread
        guard !reading else { return }
        reading = true
        let readFacts = readFacts
        Task {
            let fresh = await Task.detached { readFacts() }.value
            reading = false
            facts = fresh
            if activity == .waitingForApproval, fresh.driverReady {
                SetupActions.quitOBS()
                activity = .idle
            }
        }
    }
}
