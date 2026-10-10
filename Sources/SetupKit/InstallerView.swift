import AppKit
import SwiftUI

public struct InstallerView: View {
    @StateObject private var model: InstallerModel
    @State private var previewing = false

    public init() {
        _model = StateObject(wrappedValue: InstallerModel())
    }

    public init(model: InstallerModel) {
        _model = StateObject(wrappedValue: model)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            Header(title: "Canon Webcam", subtitle: "Your Canon EOS as a webcam. Free, no subscription.")
            if model.facts == nil {
                HStack { Spacer(); ProgressView("Looking around your Mac"); Spacer() }.frame(height: 220)
            } else {
                checklist
                if let problem = model.problem { ProblemNote(text: problem) }
                if model.isFinished { finish }
            }
            Text("Changed your mind later? Open Uninstall Canon Webcam and it cleans up everything.")
                .font(.footnote)
                .foregroundStyle(.tertiary)
        }
        .padding(28)
        .frame(width: 560)
        .onAppear { model.startWatching() }
        .animation(.easeInOut(duration: 0.25), value: model.isFinished)
        .animation(.easeInOut(duration: 0.25), value: previewing)
    }

    private var checklist: some View {
        Card {
            ForEach(Array(InstallerStep.allCases.enumerated()), id: \.offset) { index, step in
                if index > 0 { Divider().padding(.leading, 56) }
                if let status = model.status(of: step) {
                    StepRow(
                        number: index + 1,
                        title: step.title,
                        status: status,
                        action: { model.act(on: step) },
                        secondaryTitle: secondaryTitle(for: step, status: status),
                        secondaryAction: { secondaryAction(for: step) }
                    )
                }
            }
        }
    }

    private var finish: some View {
        Card {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    Image(systemName: "party.popper.fill").font(.title2).foregroundStyle(.orange)
                    Text("You're all set").font(.title3.weight(.semibold))
                }
                Text("Open Zoom, Meet, Teams or FaceTime and choose **OBS Virtual Camera** as your camera. Your Canon wakes up on its own whenever an app asks for it.")
                    .fixedSize(horizontal: false, vertical: true)
                if previewing {
                    CameraPreview(onClose: model.previewClosed)
                        .aspectRatio(16 / 9, contentMode: .fit)
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                    Text("Live from your camera. It takes a second or two to wake up.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                HStack {
                    Button(previewing ? "Hide Preview" : "Test Camera") { previewing.toggle() }
                        .controlSize(.large)
                    Spacer()
                    Button("Done") { NSApplication.shared.terminate(nil) }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .keyboardShortcut(.defaultAction)
                }
            }
            .padding(18)
        }
    }

    private func secondaryTitle(for step: InstallerStep, status: StepStatus) -> String? {
        switch (step, status) {
        case (.driver, .working) where model.activity == .waitingForApproval: return "Open Settings"
        case (.install, .done): return "Reinstall"
        default: return nil
        }
    }

    private func secondaryAction(for step: InstallerStep) {
        switch step {
        case .driver: model.reopenSettings()
        case .install: model.install()
        case .camera: break
        }
    }
}

/// A gentle red box for when something goes wrong, with the actual reason in it.
struct ProblemNote: View {
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text(text).font(.callout).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.orange.opacity(0.12)))
    }
}
