import Foundation
import SwiftUI
import WebcamEngine

/// Drives the uninstaller window: shows what will go, then makes it go.
@MainActor
public final class UninstallerModel: ObservableObject {
    public enum Stage: Equatable {
        case ready
        case removing
        case done
        case nothingInstalled
    }

    @Published public private(set) var stage: Stage
    @Published public private(set) var pieces: [String]
    @Published public var restoreCanon = false
    @Published public var problem: String?
    public let canonUtilityIsPresent: Bool

    public init(pieces: [String] = SetupActions.installedPieces(), canonUtilityIsPresent: Bool = LaunchAgent.canonUtilityIsPresent) {
        self.pieces = pieces
        self.canonUtilityIsPresent = canonUtilityIsPresent
        stage = pieces.isEmpty ? .nothingInstalled : .ready
    }

    public func uninstall() {
        problem = nil
        stage = .removing
        let restoreCanon = restoreCanon
        Task {
            do {
                try await SetupActions.uninstall(restoreCanon: restoreCanon)
                stage = .done
            } catch {
                problem = "\(error)"
                stage = .ready
            }
        }
    }
}
