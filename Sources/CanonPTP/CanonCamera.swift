import Foundation

/// What the camera has told us so far through its event stream.
public struct CameraState: Equatable, Sendable {
    public var values: [UInt32: UInt32] = [:]
    public var choices: [UInt32: [UInt32]] = [:]
    public var lensName: String?

    public init() {}

    public func value(of property: CanonProperty) -> UInt32? {
        values[property.rawValue]
    }

    public func choices(for property: CanonProperty) -> [UInt32] {
        choices[property.rawValue] ?? []
    }

    public mutating func apply(_ events: [CameraEvent]) {
        for event in events {
            switch event {
            case .propertyChanged(let property, let value):
                values[property] = value
            case .textPropertyChanged(let property, let text):
                if property == CanonProperty.lensName.rawValue { lensName = text }
            case .choicesChanged(let property, let newChoices):
                choices[property] = newChoices
            }
        }
    }
}

/// A Canon EOS body in remote mode, speaking just enough PTP to stream live view and take orders.
public final class CanonCamera {
    private let session: PTPSession
    public private(set) var state = CameraState()
    public private(set) var model = "Canon camera"
    private var liveViewOutputBeforeUs: UInt32?

    public init(transport: Transport) {
        session = PTPSession(transport: transport)
    }

    public func connect() throws {
        let opened = try session.run(.openSession, [1])
        if opened.code == ResponseCode.sessionAlreadyOpen.rawValue {
            // Someone (probably a crashed us) left a session open, so start clean
            try session.run(.closeSession)
            try session.require(.openSession, [1])
        } else if !opened.isOK {
            throw PTPError.response(opened.code, during: Operation.openSession.rawValue)
        }
        if let info = try? session.require(.getDeviceInfo), let name = modelName(fromDeviceInfo: info.data) {
            model = name
        }
        try session.require(.canonSetRemoteMode, [1])
        try session.require(.canonSetEventMode, [1])
        // The first GetEvent dumps every property and its allowed values
        try pollEvents()
    }

    public func startLiveView() throws {
        liveViewOutputBeforeUs = state.value(of: .liveViewOutput)
        if state.value(of: .liveViewMode) != 1 {
            // Movie mode answers busy here but streams anyway, so busy is fine
            let reply = try session.run(.canonSetPropertyValue, sending: Self.propertyPayload(.liveViewMode, 1))
            guard reply.isOK || reply.code == ResponseCode.deviceBusy.rawValue else {
                throw PTPError.response(reply.code, during: Operation.canonSetPropertyValue.rawValue)
            }
        }
        // Computer only, so the rear screen goes dark and the sensor runs a bit cooler
        try set(.liveViewOutput, to: LiveViewOutput.computer)
    }

    /// One attempt at a frame. Nil means the camera hasn't produced a new one yet, so ask again shortly.
    public func nextFrame() throws -> Data? {
        let reply = try session.run(.canonGetViewFinderData, [0x0020_0000, 0, 0])
        let notReady: Set<UInt16> = [ResponseCode.canonNotReady.rawValue, ResponseCode.deviceBusy.rawValue]
        if notReady.contains(reply.code) { return nil }
        guard reply.isOK else { throw PTPError.response(reply.code, during: Operation.canonGetViewFinderData.rawValue) }
        return jpegFromViewFinder(reply.data)
    }

    @discardableResult
    public func pollEvents() throws -> [CameraEvent] {
        let reply = try session.require(.canonGetEvent)
        let events = parseCanonEvents(reply.data)
        state.apply(events)
        return events
    }

    public func set(_ property: CanonProperty, to value: UInt32) throws {
        try session.require(.canonSetPropertyValue, sending: Self.propertyPayload(property, value))
        state.values[property.rawValue] = value
    }

    /// Stops the camera from powering itself off mid meeting.
    public func keepAwake() throws {
        try session.run(.canonKeepDeviceOn)
    }

    public func autofocus() throws {
        try session.require(.canonDoAutofocus)
        try session.run(.canonCancelAutofocus)
    }

    public func driveFocus(_ step: LensDrive) throws {
        try session.require(.canonDriveLens, [step.rawValue])
    }

    /// Hands the camera back the way we found it. Errors are ignored since the cable may already be gone.
    public func disconnect() {
        try? set(.liveViewOutput, to: liveViewOutputBeforeUs ?? LiveViewOutput.cameraScreen)
        _ = try? session.run(.canonSetEventMode, [0])
        _ = try? session.run(.canonSetRemoteMode, [0])
        _ = try? session.run(.closeSession)
    }

    static func propertyPayload(_ property: CanonProperty, _ value: UInt32) -> Data {
        Container.packed([12, property.rawValue, value])
    }
}
