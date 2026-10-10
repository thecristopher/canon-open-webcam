import AppKit
import SwiftUI

public struct UninstallerView: View {
    @StateObject private var model: UninstallerModel
    @Environment(\.dismiss) private var dismiss

    public init() {
        _model = StateObject(wrappedValue: UninstallerModel())
    }

    public init(model: UninstallerModel) {
        _model = StateObject(wrappedValue: model)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            Header(title: "Uninstall Canon Webcam", subtitle: "Takes everything back off your Mac.", tint: .gray)
            switch model.stage {
            case .ready, .removing: removalPlan
            case .done: allGone
            case .nothingInstalled: nothingHere
            }
            CreditFooter()
        }
        .padding(28)
        .frame(width: 520)
        .animation(.easeInOut(duration: 0.25), value: model.stage)
    }

    private var removalPlan: some View {
        VStack(alignment: .leading, spacing: 18) {
            Card {
                VStack(alignment: .leading, spacing: 10) {
                    Text("This removes").font(.headline)
                    ForEach(model.pieces, id: \.self) { piece in
                        Label(piece, systemImage: "minus.circle.fill")
                            .symbolRenderingMode(.hierarchical)
                            .foregroundStyle(.primary)
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            if model.canonUtilityIsPresent {
                Toggle(isOn: $model.restoreCanon) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Turn Canon's EOS Webcam Utility back on")
                        Text("It comes back the next time you log in.").font(.footnote).foregroundStyle(.secondary)
                    }
                }
                .toggleStyle(.checkbox)
            }
            Text("OBS stays installed. Drag it to the Trash if you don't need it anymore.")
                .font(.footnote)
                .foregroundStyle(.secondary)
            if let problem = model.problem { ProblemNote(text: problem) }
            HStack {
                Button("Cancel") { dismiss() }
                    .controlSize(.large)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                if model.stage == .removing {
                    ProgressView().controlSize(.small)
                    Text("Removing").foregroundStyle(.secondary)
                } else {
                    Button("Uninstall", role: .destructive) { model.uninstall() }
                        .buttonStyle(.borderedProminent)
                        .tint(.red)
                        .controlSize(.large)
                }
            }
        }
    }

    private var allGone: some View {
        finalNote(
            symbol: "checkmark.circle.fill",
            color: .green,
            title: "Canon Webcam is gone",
            detail: model.restoreCanon
                ? "Canon's EOS Webcam Utility comes back the next time you log in."
                : "The app, the background engine and every file they made are gone. Your camera is back to being just a camera."
        )
    }

    private var nothingHere: some View {
        finalNote(
            symbol: "sparkles",
            color: .blue,
            title: "Nothing to remove",
            detail: "Canon Webcam isn't installed on this Mac."
        )
    }

    private func finalNote(symbol: String, color: Color, title: String, detail: String) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            Card {
                HStack(spacing: 14) {
                    Image(systemName: symbol).font(.system(size: 30)).foregroundStyle(color)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(title).font(.title3.weight(.semibold))
                        Text(detail).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer()
                }
                .padding(18)
            }
            HStack {
                Spacer()
                Button("Quit") { NSApplication.shared.terminate(nil) }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .keyboardShortcut(.defaultAction)
            }
        }
    }
}
