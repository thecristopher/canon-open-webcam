// The PTP and Canon EOS numbers we actually use. libgphoto2's ptp.h has the full zoo.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Operation {
    GetDeviceInfo = 0x1001,
    OpenSession = 0x1002,
    CloseSession = 0x1003,
    CanonSetPropertyValue = 0x9110,
    CanonSetRemoteMode = 0x9114,
    CanonSetEventMode = 0x9115,
    CanonGetEvent = 0x9116,
    CanonKeepDeviceOn = 0x911D,
    CanonGetViewFinderData = 0x9153,
    CanonDoAutofocus = 0x9154,
    CanonDriveLens = 0x9155,
    CanonCancelAutofocus = 0x9160,
}

impl Operation {
    pub fn code(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ResponseCode {
    Ok = 0x2001,
    GeneralError = 0x2002,
    SessionNotOpen = 0x2003,
    OperationNotSupported = 0x2005,
    DeviceBusy = 0x2019,
    SessionAlreadyOpen = 0x201E,
    // Canon's way of saying the next live view frame isn't ready yet
    CanonNotReady = 0xA102,
}

impl ResponseCode {
    pub fn code(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ContainerType {
    Command = 1,
    Data = 2,
    Response = 3,
    Event = 4,
}

impl ContainerType {
    pub fn from_raw(raw: u16) -> Option<Self> {
        match raw {
            1 => Some(Self::Command),
            2 => Some(Self::Data),
            3 => Some(Self::Response),
            4 => Some(Self::Event),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CanonEvent {
    PropertyChanged = 0xC189,
    AvailableValuesChanged = 0xC18A,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum CanonProperty {
    Aperture = 0xD101,
    ShutterSpeed = 0xD102,
    Iso = 0xD103,
    ExposureCompensation = 0xD104,
    ExposureMode = 0xD105,
    WhiteBalance = 0xD109,
    ColorTemperature = 0xD10A,
    PictureStyle = 0xD110,
    Battery = 0xD111,
    LiveViewOutput = 0xD1B0,
    LiveViewMode = 0xD1B1,
    LiveViewSharpness = 0xD1B3,
    LensName = 0xD1D8,
}

impl CanonProperty {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// Live view output is a bit mask, and computer is the bit that makes frames come over USB.
pub struct LiveViewOutput;

impl LiveViewOutput {
    pub const CAMERA_SCREEN: u32 = 1;
    pub const COMPUTER: u32 = 2;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum LensDrive {
    NearSmall = 0x0001,
    NearMedium = 0x0002,
    NearLarge = 0x0003,
    FarSmall = 0x8001,
    FarMedium = 0x8002,
    FarLarge = 0x8003,
}
