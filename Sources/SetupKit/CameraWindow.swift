import AppKit
import SwiftUI
import WebcamEngine

/// The main window: the picture front and center, every control in a tidy sidebar.
public struct CameraWindow: View {
    @ObservedObject var model: CameraModel

    public init(model: CameraModel) {
        self.model = model
    }

    public var body: some View {
        HStack(spacing: 0) {
            Stage(model: model)
            Divider()
            Inspector(model: model).frame(width: 340)
        }
        .frame(minWidth: 980, minHeight: 600)
        // A Dock icon while the window is open, just the menu bar icon once it closes
        .onAppear {
            NSApplication.shared.setActivationPolicy(.regular)
            model.startPolling()
        }
        .onDisappear {
            NSApplication.shared.setActivationPolicy(.accessory)
        }
    }
}

// MARK: Stage

/// The live picture, exactly what everyone on the call sees, with framing gestures and a floating toolbar.
struct Stage: View {
    @ObservedObject var model: CameraModel
    @State private var lastDrag = CGSize.zero
    @State private var zoomAtPinchStart: Double?

    var body: some View {
        GeometryReader { space in
            let picture = Self.fitted16by9(in: space.size, padding: 28)
            ZStack {
                LinearGradient(colors: [Color(white: 0.13), Color(white: 0.07)], startPoint: .top, endPoint: .bottom)
                CameraPreview(onClose: model.previewClosed)
                    .frame(width: picture.width, height: picture.height)
                    .overlay { if !model.state.isLive { StageMessage(state: model.state) } }
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(alignment: .topLeading) { StatusPill(state: model.state).padding(14) }
                    .overlay(alignment: .bottom) { FloatingToolbar(model: model).padding(.bottom, 16) }
                    .shadow(color: .black.opacity(0.45), radius: 24, y: 10)
                    .gesture(reframe(pictureSize: picture))
                    .gesture(pinchToZoom)
            }
        }
        .frame(minWidth: 600)
    }

    /// Dragging moves the picture under your finger, like moving a photo inside a frame.
    private func reframe(pictureSize: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 2)
            .onChanged { drag in
                let step = CGSize(width: drag.translation.width - lastDrag.width, height: drag.translation.height - lastDrag.height)
                lastDrag = drag.translation
                let overflow = model.options.zoom - 1
                guard overflow > 0.01 else { return }
                let mirrored = model.options.mirror ? -1.0 : 1.0
                model.pan(
                    byX: -step.width / (overflow * pictureSize.width / 2) * mirrored,
                    y: step.height / (overflow * pictureSize.height / 2)
                )
            }
            .onEnded { _ in lastDrag = .zero }
    }

    private var pinchToZoom: some Gesture {
        MagnifyGesture()
            .onChanged { pinch in
                let start = zoomAtPinchStart ?? model.options.zoom
                zoomAtPinchStart = start
                model.zoom(to: start * pinch.magnification)
            }
            .onEnded { _ in zoomAtPinchStart = nil }
    }

    static func fitted16by9(in space: CGSize, padding: CGFloat) -> CGSize {
        let width = max(space.width - padding * 2, 100)
        let height = max(space.height - padding * 2, 56)
        return width / height > 16 / 9
            ? CGSize(width: height * 16 / 9, height: height)
            : CGSize(width: width, height: width * 9 / 16)
    }
}

/// A frosted pill in the corner: a colored dot and what the camera is up to.
struct StatusPill: View {
    let state: PanelState

    var body: some View {
        HStack(spacing: 7) {
            Circle().fill(color).frame(width: 8, height: 8)
            Text(text).font(.callout.weight(.medium))
        }
        .padding(.horizontal, 11)
        .padding(.vertical, 6)
        .background(.ultraThinMaterial, in: Capsule())
        .environment(\.colorScheme, .dark)
    }

    private var text: String {
        if case .live(let model, let detail) = state { return detail.isEmpty ? model : "\(model) · \(detail.components(separatedBy: " · ").last ?? "")" }
        return state.title
    }

    private var color: Color {
        switch state {
        case .live: return .green
        case .starting: return .blue
        case .trouble, .notRunning: return .orange
        case .resting, .waitingForCamera: return .gray
        }
    }
}

/// Shown over the picture while there's nothing live to look at yet.
struct StageMessage: View {
    let state: PanelState

    var body: some View {
        VStack(spacing: 10) {
            if state == .resting || state == .starting { ProgressView().controlSize(.large).tint(.white) }
            // This window is itself an app watching, so resting here really means waking up
            Text(state == .resting ? "Waking up your camera" : state.title).font(.title3.weight(.semibold))
            Text(state == .resting ? PanelState.starting.subtitle : state.subtitle)
                .font(.callout)
                .foregroundStyle(.white.opacity(0.7))
                .multilineTextAlignment(.center)
        }
        .foregroundStyle(.white)
        .padding(30)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.black.opacity(0.55))
    }
}

/// The handful of things you reach for mid call, floating over the bottom of the picture.
struct FloatingToolbar: View {
    @ObservedObject var model: CameraModel

    var body: some View {
        HStack(spacing: 4) {
            toolbarButton("minus.magnifyingglass", help: "Zoom out") { model.zoom(to: model.options.zoom - 0.25) }
            Text(String(format: "%.1f×", model.options.zoom))
                .font(.callout.monospacedDigit())
                .frame(width: 40)
            toolbarButton("plus.magnifyingglass", help: "Zoom in") { model.zoom(to: model.options.zoom + 0.25) }
            Divider().frame(height: 18).padding(.horizontal, 4)
            toolbarButton("viewfinder", help: "Show the whole picture") { model.setOption("framing", "reset") }
            toolbarButton("arrow.left.and.right.righttriangle.left.righttriangle.right", help: "Mirror for everyone", active: model.options.mirror) {
                model.setOption("mirror", model.options.mirror ? "off" : "on")
            }
            toolbarButton("scope", help: "Autofocus") { model.focus("auto") }
                .disabled(!model.state.isLive)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.ultraThinMaterial, in: Capsule())
        .environment(\.colorScheme, .dark)
    }

    private func toolbarButton(_ symbol: String, help: String, active: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .medium))
                .frame(width: 30, height: 26)
                .foregroundStyle(active ? Color.accentColor : .primary)
        }
        .buttonStyle(.borderless)
        .help(help)
    }
}

// MARK: Inspector

/// Every knob, grouped the way System Settings groups things.
struct Inspector: View {
    @ObservedObject var model: CameraModel

    var body: some View {
        Form {
            Section {
                AdjustmentRow(title: "Brightness", symbol: "sun.max", value: adjustment(\.brightness, "brightness"))
                AdjustmentRow(title: "Contrast", symbol: "circle.lefthalf.filled", value: adjustment(\.contrast, "contrast"))
                AdjustmentRow(title: "Saturation", symbol: "drop", value: adjustment(\.saturation, "saturation"))
                AdjustmentRow(title: "Warmth", symbol: "thermometer.sun", value: adjustment(\.warmth, "warmth"), lowLabel: "Cool", highLabel: "Warm")
                AdjustmentRow(title: "Tint", symbol: "paintpalette", value: adjustment(\.tint, "tint"), lowLabel: "Green", highLabel: "Magenta")
                AdjustmentRow(title: "Sharpness", symbol: "sparkle", value: adjustment(\.sharpness, "sharpness"), range: 0...1, resetValue: 0.4)
            } header: {
                SectionHeader(title: "Adjust", actionTitle: "Reset") { model.setOption("picture", "reset") }
            }

            Section {
                AdjustmentRow(title: "Zoom", symbol: "plus.magnifyingglass", value: model.binding(\.zoom, name: "zoom", format: CameraModel.number),
                              range: FrameOptions.zoomRange, resetValue: 1, display: { String(format: "%.1f×", $0) })
                Toggle(isOn: model.binding(\.mirror, name: "mirror", format: { $0 ? "on" : "off" })) {
                    Label("Mirror for everyone", systemImage: "arrow.left.and.right.righttriangle.left.righttriangle.right")
                }
            } header: {
                SectionHeader(title: "Framing", actionTitle: "Center") { model.setOption("framing", "reset") }
            } footer: {
                Text("Drag the picture to reframe it, pinch to zoom.").font(.caption).foregroundStyle(.secondary)
            }

            Section {
                CameraSliderRow(title: "Exposure", symbol: "plusminus.circle", control: model.control(for: "exposure")) { model.setCamera("exposure", to: $0) }
                CameraPickerRow(title: "White balance", symbol: "lightbulb", control: model.control(for: "white-balance")) { model.setCamera("white-balance", to: $0) }
                CameraPickerRow(title: "ISO", symbol: "camera.aperture", control: model.control(for: "iso")) { model.setCamera("iso", to: $0) }
                CameraPickerRow(title: "Aperture", symbol: "circle.dashed", control: model.control(for: "aperture")) { model.setCamera("aperture", to: $0) }
                CameraPickerRow(title: "Shutter", symbol: "timer", control: model.control(for: "shutter")) { model.setCamera("shutter", to: $0) }
                FocusRow(model: model)
            } header: {
                Text("Camera")
            } footer: {
                Text(cameraFooter).font(.caption).foregroundStyle(.secondary)
            }

            Section("Output") {
                Picker(selection: model.binding(\.frameRate, name: "fps", format: { $0 == 0 ? "camera" : String($0) })) {
                    Text("Camera").tag(0)
                    Text("30 fps").tag(30)
                    Text("60 fps").tag(60)
                } label: {
                    Label("Frame rate", systemImage: "film.stack")
                }
                LabeledContent {
                    Text("1920 × 1080")
                } label: {
                    Label("Resolution", systemImage: "rectangle.on.rectangle")
                }
                LabeledContent {
                    Text(VirtualCamera.deviceName)
                } label: {
                    Label("Shows up as", systemImage: "video")
                }
            }

            if let problem = model.problem {
                Section { ProblemNote(text: problem) }
            }

            Section {
            } footer: {
                CreditFooter()
            }
        }
        .formStyle(.grouped)
    }

    private func adjustment(_ keyPath: WritableKeyPath<FrameOptions, Double>, _ name: String) -> Binding<Double> {
        model.binding(keyPath, name: name, format: CameraModel.number)
    }

    private var cameraFooter: String {
        guard model.state.isLive else { return "These come from the camera itself, so they unlock while it's live." }
        let manual = ["iso", "aperture", "shutter"].map(model.control(for:))
        let allLocked = manual.allSatisfy { if case .lockedByMode = $0 { return true } else { return false } }
        return allLocked ? "Turn the camera's dial to M to set ISO, aperture and shutter yourself." : "Your camera remembers these on its own."
    }
}
