import CanonPTP
import CanonUSB
import CoreVideo
import Foundation

/// Where the engine is at, and the card apps see when there's no live picture.
public enum Phase: Equatable, Sendable {
    case idle
    case waitingForCamera
    case starting
    case live
    case trouble(String)

    var name: String {
        switch self {
        case .idle: return "idle (no app is using the camera)"
        case .waitingForCamera: return "waiting for the camera"
        case .starting: return "starting live view"
        case .live: return "live"
        case .trouble(let reason): return "trouble: \(reason)"
        }
    }

    var card: (title: String, detail: String) {
        switch self {
        case .idle, .waitingForCamera: return ("Waiting for your camera", "Plug in the Canon over USB and turn it on")
        case .starting: return ("Starting camera", "Live view takes a second or two")
        case .live: return ("", "")
        case .trouble(let reason): return ("Can't reach the camera", reason)
        }
    }
}

public struct EngineConfig {
    /// Stream even when no app is watching. Costs camera battery and heat, saves the 2 second warm up.
    public var alwaysOn = false
    public var log: (String) -> Void = { print($0) }

    public init(alwaysOn: Bool = false, log: @escaping (String) -> Void = { print($0) }) {
        self.alwaysOn = alwaysOn
        self.log = log
    }
}

/// Camera in, virtual camera out. Everything that touches USB or CoreMediaIO runs on the thread that calls run().
public final class Engine {
    private let config: EngineConfig
    private let renderer = FrameRenderer()
    private let optionsStore: OptionsStore
    private var options: FrameOptions

    private var virtualCamera: VirtualCamera?
    private var usb: USBTransport?
    private var camera: CanonCamera?
    private var phase = Phase.idle
    private var appsWatching = false
    private var probes = ProbePolicy(startingAt: Date())
    private var meter = FrameRateMeter()
    private let pacer = FramePacer()
    private var stats = TimingStats(enabled: ProcessInfo.processInfo.environment["CANON_WEBCAM_DEBUG"] != nil)

    private var lastFrameAt = Date()
    private var lastLoggedTrouble: String?
    private var lastAcceptedSend = Date()
    private var situationCheck = Every(1)
    private var eventPoll = Every(0.5)
    private var keepAlive = Every(20)
    private var cardRefresh = Every(0.2)

    // Requests from the control socket wait here until the camera thread gets to them
    private let lock = NSLock()
    private var inbox: [(EngineCommand, (ControlResponse) -> Void)] = []
    private var snapshot = EngineStatus(state: Phase.idle.name)

    // No frames for this long means live view quietly died
    private let stallLimit: TimeInterval = 5
    // The extension stopped taking frames, so it probably restarted underneath us
    private let deadSinkLimit: TimeInterval = 3

    public init(config: EngineConfig = EngineConfig(), optionsStore: OptionsStore = OptionsStore()) {
        self.config = config
        self.optionsStore = optionsStore
        self.options = optionsStore.load()
    }

    public func run(until shouldStop: () -> Bool) {
        config.log(config.alwaysOn ? "Streaming all the time (--always-on)." : "Ready. The camera wakes up when an app opens \(VirtualCamera.deviceName).")
        publishStatus()
        while !shouldStop() {
            runPendingCommands()
            let now = Date()
            updateWatching(at: now)
            guard config.alwaysOn || appsWatching else {
                goIdle()
                // Short naps so an app that opens the camera sees OBS's logo for a blink at most
                Thread.sleep(forTimeInterval: 0.25)
                continue
            }
            guard ensureVirtualCamera() else {
                Thread.sleep(forTimeInterval: 5)
                continue
            }
            if camera == nil {
                connectCamera()
            } else {
                stepLiveView()
            }
        }
        goIdle()
        config.log("Stopped. The camera is back to normal.")
    }

    // MARK: Control socket side

    /// Called from the socket thread. Status answers right away, everything else waits its turn on the camera thread.
    public func handle(_ request: ControlRequest) -> ControlResponse {
        if request.command == "status" {
            return .success(status: lock.withLock { snapshot })
        }
        let command: EngineCommand
        switch EngineCommand.from(request) {
        case .success(let parsed): command = parsed
        case .failure(let error): return .failure(error.description)
        }

        let done = DispatchSemaphore(value: 0)
        var response = ControlResponse.failure("the engine didn't answer in time")
        lock.withLock {
            inbox.append((command, { response = $0; done.signal() }))
        }
        _ = done.wait(timeout: .now() + 10)
        return response
    }

    private func runPendingCommands() {
        let pending = lock.withLock { () -> [(EngineCommand, (ControlResponse) -> Void)] in
            defer { inbox.removeAll() }
            return inbox
        }
        for (command, reply) in pending {
            do {
                let message = try execute(command)
                config.log(message)
                publishStatus()
                reply(.success(message, status: lock.withLock { snapshot }))
            } catch {
                reply(.failure("\(error)"))
            }
        }
    }

    private func execute(_ command: EngineCommand) throws -> String {
        switch command {
        case .setMirror(let on):
            options.mirror = on
            try optionsStore.save(options)
            return "Mirror is \(on ? "on" : "off")."
        case .setZoom(let zoom):
            options.zoom = zoom
            try optionsStore.save(options)
            return String(format: "Zoom is %.1fx.", zoom)
        case .setCamera(let setting, let input):
            let camera = try liveCamera(for: setting.rawValue)
            let choices = camera.state.choices(for: setting.property)
            guard !choices.isEmpty else {
                // Movie mode with auto exposure, for example, owns ISO and shutter itself
                throw ControlError.unknown("The camera's current mode controls \(setting.rawValue) itself. Switch the dial to manual (M) to set it.")
            }
            guard let code = setting.code(for: input, choices: choices), choices.contains(code) else {
                let choiceLabels = choices.map(setting.label(for:)).joined(separator: ", ")
                throw ControlError.unknown("\(input) isn't available for \(setting.rawValue) right now. Pick from: \(choiceLabels)")
            }
            try camera.set(setting.property, to: code)
            return "\(setting.rawValue) is now \(setting.label(for: code))."
        case .autofocus:
            try liveCamera(for: "focus").autofocus()
            return "Focused."
        case .focus(let step):
            try liveCamera(for: "focus").driveFocus(step)
            return "Nudged focus."
        }
    }

    private func liveCamera(for what: String) throws -> CanonCamera {
        guard let camera, phase == .live else {
            throw ControlError.unknown("The camera isn't live right now, so \(what) can't change. Open the camera in an app first.")
        }
        return camera
    }

    // MARK: Who's watching

    private func updateWatching(at now: Date) {
        guard let virtualCamera else {
            // With our sink off, the flag only lights up for real apps, so a plain read is honest
            appsWatching = VirtualCamera.appsAreWatchingWhileIdle()
            return
        }
        guard situationCheck.isDue(at: now), probes.shouldProbe(UserPresence.currentSituation(), at: now) else { return }
        appsWatching = virtualCamera.probeAppsWatching()
        if appsWatching { probes.probeFoundSomeoneWatching(at: now) }
        if stats.enabled { config.log(appsWatching ? "probe: still watched" : "probe: nobody is watching") }
    }

    private func ensureVirtualCamera() -> Bool {
        if virtualCamera != nil { return true }
        guard let opened = VirtualCamera.open() else {
            setPhase(.trouble("OBS Virtual Camera isn't installed"))
            config.log("Can't find \(VirtualCamera.deviceName). Run `canon-webcam doctor` for the fix.")
            return false
        }
        virtualCamera = opened
        lastAcceptedSend = Date()
        probes = ProbePolicy(startingAt: Date())
        config.log("An app opened the camera.")
        return true
    }

    // MARK: Camera

    private func connectCamera() {
        guard USB.canonIsPluggedIn() else {
            setPhase(.waitingForCamera)
            holdCard(for: 0.5)
            return
        }
        setPhase(.starting)
        showCardIfDue(force: true)
        do {
            freeCameraFromMacOS()
            guard let usb = try USB.openCanon() else {
                setPhase(.waitingForCamera)
                return
            }
            let camera = CanonCamera(transport: usb)
            try camera.connect()
            try camera.startLiveView()
            self.usb = usb
            self.camera = camera
            lastFrameAt = Date()
            meter.reset()
            eventPoll.reset()
            keepAlive.reset()
            lastLoggedTrouble = nil
            config.log("Live from \(camera.model) with \(camera.state.lensName ?? "an unknown lens").")
        } catch {
            releaseCamera()
            let reason = "\(error)"
            // Retrying every couple of seconds shouldn't write the same line to the log forever
            if reason != lastLoggedTrouble { config.log("Couldn't start the camera: \(reason) Retrying until it works.") }
            lastLoggedTrouble = reason
            setPhase(.trouble(reason))
            holdCard(for: 2)
        }
    }

    private func stepLiveView() {
        guard let camera else { return }
        do {
            let now = Date()
            if eventPoll.isDue(at: now) {
                let pollStart = Date()
                try camera.pollEvents()
                stats.record(.events, since: pollStart)
                publishStatus()
            }
            if keepAlive.isDue(at: now) {
                try camera.keepAwake()
            }

            let wait = pacer.waitBeforeAsking(sinceLastFrame: Date().timeIntervalSince(lastFrameAt))
            if wait > 0 { Thread.sleep(forTimeInterval: wait) }

            let askedAt = Date()
            let answer = try camera.nextFrame()
            stats.record(.ask, since: askedAt)
            guard let jpeg = answer else {
                stats.count(.notReady)
                if Date().timeIntervalSince(lastFrameAt) > stallLimit {
                    throw EngineError.stalled
                }
                Thread.sleep(forTimeInterval: pacer.retryDelay)
                return
            }
            if stats.enabled, Date().timeIntervalSince(lastFrameAt) > 0.06 {
                config.log(String(format: "slow frame: %.0fms since the last one", Date().timeIntervalSince(lastFrameAt) * 1000))
            }
            lastFrameAt = Date()
            meter.frameArrived(at: lastFrameAt)
            if phase != .live { setPhase(.live) }
            let renderStart = Date()
            if let frame = renderer.render(jpeg: jpeg, options: options) {
                stats.record(.render, since: renderStart)
                send(frame)
            }
            if let summary = stats.summaryIfDue(at: Date()) { config.log(summary) }
        } catch {
            config.log("Lost the camera: \(error). Reconnecting.")
            releaseCamera()
            setPhase(.trouble("Lost the camera, reconnecting"))
            holdCard(for: 1)
        }
    }

    private func releaseCamera() {
        camera?.disconnect()
        camera = nil
        usb = nil
        meter.reset()
    }

    private func goIdle() {
        guard virtualCamera != nil || camera != nil else { return }
        releaseCamera()
        virtualCamera = nil
        setPhase(.idle)
        config.log("No app is using the camera anymore, so it's resting.")
    }

    // MARK: Frames out

    private func send(_ frame: CVPixelBuffer) {
        guard let virtualCamera else { return }
        if virtualCamera.send(frame) {
            lastAcceptedSend = Date()
        } else if Date().timeIntervalSince(lastAcceptedSend) > deadSinkLimit {
            config.log("The virtual camera stopped taking frames, reopening it.")
            self.virtualCamera = nil
        }
    }

    private func showCardIfDue(force: Bool = false) {
        guard force || cardRefresh.isDue(at: Date()) else { return }
        let card = phase.card
        if let frame = renderer.placeholder(title: card.title, detail: card.detail) {
            send(frame)
        }
    }

    /// Keeps the card on screen while we wait, so apps don't freeze on an old frame.
    private func holdCard(for seconds: TimeInterval) {
        let until = Date().addingTimeInterval(seconds)
        while Date() < until {
            showCardIfDue()
            Thread.sleep(forTimeInterval: 0.05)
        }
    }

    // MARK: Status

    private func setPhase(_ newPhase: Phase) {
        guard newPhase != phase else { return }
        phase = newPhase
        publishStatus()
    }

    private func publishStatus() {
        var settings: [String: SettingStatus] = [:]
        if let state = camera?.state {
            for setting in CameraSetting.allCases {
                guard let value = state.value(of: setting.property) else { continue }
                settings[setting.rawValue] = SettingStatus(
                    value: setting.label(for: value),
                    choices: state.choices(for: setting.property).map(setting.label(for:))
                )
            }
        }
        let status = EngineStatus(
            state: phase.name, model: camera?.model, lens: camera?.state.lensName,
            framesPerSecond: meter.framesPerSecond, appsWatching: virtualCamera != nil,
            options: options, settings: settings
        )
        lock.withLock { snapshot = status }
    }
}
