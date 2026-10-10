import Foundation
import SwiftUI
import WebcamEngine

/// The app's view of the engine: polls its status, and turns slider drags into a calm stream of changes.
@MainActor
public final class CameraModel: ObservableObject {
    @Published public private(set) var status: EngineStatus?
    /// What the controls show. Changes land here first, so dragging feels instant even before the engine answers.
    @Published public private(set) var options = FrameOptions()
    @Published public var problem: String?

    private var poller: Timer?
    private var asking = false
    private var lastLocalEdit = Date.distantPast
    private var lastSent: [String: Date] = [:]
    private var pendingSend: [String: Task<Void, Never>] = [:]
    // Fast enough to look live while dragging, slow enough not to swamp the engine
    private let sendInterval: TimeInterval = 0.08

    public init() {}

    /// Starts from a known status, for previews and tests.
    init(showing status: EngineStatus) {
        self.status = status
        self.options = status.options
    }

    public var state: PanelState { PanelState.from(status) }

    public func control(for setting: String) -> SettingControl {
        SettingControl.from(status?.settings[setting])
    }

    public func startPolling() {
        guard poller == nil else { return }
        refresh()
        poller = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    // MARK: Picture, framing and output

    /// Binds a slider or toggle to one option. Reading is instant, writing is throttled to the engine.
    public func binding<Value>(_ keyPath: WritableKeyPath<FrameOptions, Value>, name: String, format: @escaping (Value) -> String) -> Binding<Value> {
        Binding(
            get: { self.options[keyPath: keyPath] },
            set: { value in
                self.options[keyPath: keyPath] = value
                self.sendSoon(name, format(value))
            }
        )
    }

    public func setOption(_ name: String, _ value: String) {
        guard let changed = try? options.applying(name, value).get() else { return }
        options = changed
        lastLocalEdit = Date()
        send(.set(name, to: value))
    }

    /// Drag on the preview to slide the zoomed frame around.
    public func pan(byX deltaX: Double, y deltaY: Double) {
        options.panX = min(max(options.panX + deltaX, -1), 1)
        options.panY = min(max(options.panY + deltaY, -1), 1)
        sendSoon("pan-x", Self.number(options.panX))
        sendSoon("pan-y", Self.number(options.panY))
    }

    public func zoom(to value: Double) {
        options.zoom = min(max(value, FrameOptions.zoomRange.lowerBound), FrameOptions.zoomRange.upperBound)
        sendSoon("zoom", Self.number(options.zoom))
    }

    // MARK: Camera settings and focus

    public func setCamera(_ name: String, to value: String) {
        send(.set(name, to: value))
    }

    public func focus(_ action: String, steps: Int = 1) {
        send(.focus(action, steps: steps))
    }

    /// The preview just closed, so let the engine rest the camera if nobody else is watching.
    public func previewClosed() {
        Task.detached { _ = try? ControlClient.send(.recheck) }
    }

    nonisolated static func number(_ value: Double) -> String {
        String(format: "%.3f", value)
    }

    // MARK: Talking to the engine

    /// Sends right away if we haven't lately, otherwise once the interval passes, always with the latest value.
    private func sendSoon(_ name: String, _ value: String) {
        lastLocalEdit = Date()
        pendingSend[name]?.cancel()
        let since = Date().timeIntervalSince(lastSent[name] ?? .distantPast)
        let delay = max(sendInterval - since, 0)
        pendingSend[name] = Task {
            if delay > 0 { try? await Task.sleep(for: .seconds(delay)) }
            guard !Task.isCancelled else { return }
            lastSent[name] = Date()
            send(.set(name, to: value))
        }
    }

    private func send(_ request: ControlRequest) {
        problem = nil
        Task {
            let response = await Task.detached { try? ControlClient.send(request) }.value
            if let response, !response.ok { problem = response.message }
            if let fresh = response?.status { accept(fresh) }
        }
    }

    private func refresh() {
        guard !asking else { return }
        asking = true
        Task {
            let response = await Task.detached { try? ControlClient.send(.status) }.value
            asking = false
            if let fresh = response?.status { accept(fresh) } else { status = nil }
        }
    }

    /// Takes the engine's word for everything, except options you're in the middle of dragging.
    private func accept(_ fresh: EngineStatus) {
        status = fresh
        if Date().timeIntervalSince(lastLocalEdit) > 1.5 { options = fresh.options }
    }
}
