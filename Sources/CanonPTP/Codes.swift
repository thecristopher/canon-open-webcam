// The PTP and Canon EOS numbers we actually use. libgphoto2's ptp.h has the full zoo.

public enum Operation: UInt16, Sendable {
    case getDeviceInfo = 0x1001
    case openSession = 0x1002
    case closeSession = 0x1003
    case canonSetPropertyValue = 0x9110
    case canonSetRemoteMode = 0x9114
    case canonSetEventMode = 0x9115
    case canonGetEvent = 0x9116
    case canonKeepDeviceOn = 0x911D
    case canonGetViewFinderData = 0x9153
    case canonDoAutofocus = 0x9154
    case canonDriveLens = 0x9155
    case canonCancelAutofocus = 0x9160
}

public enum ResponseCode: UInt16, Sendable {
    case ok = 0x2001
    case generalError = 0x2002
    case sessionNotOpen = 0x2003
    case operationNotSupported = 0x2005
    case deviceBusy = 0x2019
    case sessionAlreadyOpen = 0x201E
    // Canon's way of saying the next live view frame isn't ready yet
    case canonNotReady = 0xA102
}

public enum ContainerType: UInt16, Sendable {
    case command = 1
    case data = 2
    case response = 3
    case event = 4
}

public enum CanonEvent: UInt32, Sendable {
    case propertyChanged = 0xC189
    case availableValuesChanged = 0xC18A
}

public enum CanonProperty: UInt32, Sendable, CaseIterable {
    case aperture = 0xD101
    case shutterSpeed = 0xD102
    case iso = 0xD103
    case exposureCompensation = 0xD104
    case exposureMode = 0xD105
    case whiteBalance = 0xD109
    case colorTemperature = 0xD10A
    case pictureStyle = 0xD110
    case battery = 0xD111
    case liveViewOutput = 0xD1B0
    case liveViewMode = 0xD1B1
    case liveViewSharpness = 0xD1B3
    case lensName = 0xD1D8
}

// Live view output is a bit mask, and PC is the bit that makes frames come over USB
public enum LiveViewOutput {
    public static let cameraScreen: UInt32 = 1
    public static let computer: UInt32 = 2
}

public enum LensDrive: UInt32, Sendable {
    case nearSmall = 0x0001
    case nearMedium = 0x0002
    case nearLarge = 0x0003
    case farSmall = 0x8001
    case farMedium = 0x8002
    case farLarge = 0x8003
}
