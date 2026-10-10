import CLibUSB
import CanonPTP
import Foundation

let canonVendorID: UInt16 = 0x04A9
// Interface class 6 is "still image", which is what PTP cameras show up as
let stillImageClass: UInt8 = 6

/// Moves PTP containers over the camera's bulk endpoints with libusb.
public final class USBTransport: Transport {
    public let modelName: String
    private let handle: OpaquePointer
    private let interfaceNumber: Int32
    private let endpointIn: UInt8
    private let endpointOut: UInt8
    // Big enough that a whole live view frame usually arrives in one read
    private var readBuffer = [UInt8](repeating: 0, count: 1 << 20)
    private let timeoutMilliseconds: UInt32 = 3000

    init(handle: OpaquePointer, interfaceNumber: Int32, endpointIn: UInt8, endpointOut: UInt8, modelName: String) {
        self.handle = handle
        self.interfaceNumber = interfaceNumber
        self.endpointIn = endpointIn
        self.endpointOut = endpointOut
        self.modelName = modelName
    }

    deinit {
        libusb_release_interface(handle, interfaceNumber)
        libusb_close(handle)
    }

    public func send(_ bytes: Data) throws {
        var outgoing = [UInt8](bytes)
        var transferred: Int32 = 0
        let result = libusb_bulk_transfer(handle, endpointOut, &outgoing, Int32(outgoing.count), &transferred, timeoutMilliseconds)
        try check(result, doing: "sending to the camera")
    }

    public func receiveContainer() throws -> Data {
        var container = try readChunk()
        // A zero length packet can trail the previous transfer, so skip it
        while container.isEmpty { container = try readChunk() }
        guard let declared = Container.declaredLength(of: container) else {
            throw PTPError.malformed("packet shorter than a length field")
        }
        while container.count < declared {
            container.append(try readChunk())
        }
        return container
    }

    private func readChunk() throws -> Data {
        var transferred: Int32 = 0
        let result = readBuffer.withUnsafeMutableBufferPointer { buffer in
            libusb_bulk_transfer(handle, endpointIn, buffer.baseAddress, Int32(buffer.count), &transferred, timeoutMilliseconds)
        }
        try check(result, doing: "reading from the camera")
        return Data(readBuffer[0..<Int(transferred)])
    }

    private func check(_ result: Int32, doing what: String) throws {
        guard result == 0 else {
            throw PTPError.transport("\(what) failed with \(String(cString: libusb_error_name(result)))")
        }
    }
}

public enum USB {
    /// A cheap look at the bus without opening anything, for waiting on a replug.
    public static func canonIsPluggedIn() -> Bool {
        withDeviceList { devices in devices.contains { findPTPInterface(on: $0) != nil } }
    }

    /// Opens and claims the first Canon still image interface, or nil when no camera is around.
    public static func openCanon() throws -> USBTransport? {
        try withDeviceList { devices in
            for device in devices {
                guard let ptp = findPTPInterface(on: device) else { continue }
                var handle: OpaquePointer?
                let opened = libusb_open(device, &handle)
                guard opened == 0, let handle else {
                    throw PTPError.transport("couldn't open the camera (\(String(cString: libusb_error_name(opened))))")
                }
                libusb_set_auto_detach_kernel_driver(handle, 1)
                let claimed = libusb_claim_interface(handle, ptp.interface)
                guard claimed == 0 else {
                    libusb_close(handle)
                    throw USBError.heldByAnotherApp
                }
                // Clears any half finished transfer a crashed run left on the pipes
                libusb_clear_halt(handle, ptp.endpointIn)
                libusb_clear_halt(handle, ptp.endpointOut)
                return USBTransport(
                    handle: handle, interfaceNumber: ptp.interface,
                    endpointIn: ptp.endpointIn, endpointOut: ptp.endpointOut,
                    modelName: productName(device: device, handle: handle)
                )
            }
            return nil
        }
    }

    struct PTPInterface {
        var interface: Int32
        var endpointIn: UInt8
        var endpointOut: UInt8
    }

    static func findPTPInterface(on device: OpaquePointer) -> PTPInterface? {
        var descriptor = libusb_device_descriptor()
        guard libusb_get_device_descriptor(device, &descriptor) == 0, descriptor.idVendor == canonVendorID else { return nil }
        var config: UnsafeMutablePointer<libusb_config_descriptor>?
        guard libusb_get_active_config_descriptor(device, &config) == 0, let config else { return nil }
        defer { libusb_free_config_descriptor(config) }

        for interfaceIndex in 0..<Int(config.pointee.bNumInterfaces) {
            let interface = config.pointee.interface[interfaceIndex]
            guard interface.num_altsetting > 0 else { continue }
            let setting = interface.altsetting[0]
            guard setting.bInterfaceClass == stillImageClass else { continue }
            var endpointIn: UInt8 = 0
            var endpointOut: UInt8 = 0
            for endpointIndex in 0..<Int(setting.bNumEndpoints) {
                let endpoint = setting.endpoint[endpointIndex]
                let isBulk = endpoint.bmAttributes & 0x3 == UInt8(LIBUSB_TRANSFER_TYPE_BULK.rawValue)
                guard isBulk else { continue }
                if endpoint.bEndpointAddress & 0x80 != 0 {
                    endpointIn = endpoint.bEndpointAddress
                } else {
                    endpointOut = endpoint.bEndpointAddress
                }
            }
            if endpointIn != 0, endpointOut != 0 {
                return PTPInterface(interface: Int32(setting.bInterfaceNumber), endpointIn: endpointIn, endpointOut: endpointOut)
            }
        }
        return nil
    }

    static func productName(device: OpaquePointer, handle: OpaquePointer) -> String {
        var descriptor = libusb_device_descriptor()
        libusb_get_device_descriptor(device, &descriptor)
        var text = [UInt8](repeating: 0, count: 128)
        let length = libusb_get_string_descriptor_ascii(handle, descriptor.iProduct, &text, Int32(text.count))
        guard length > 0 else { return "Canon camera" }
        return String(decoding: text[0..<Int(length)], as: UTF8.self)
    }

    private static func withDeviceList<T>(_ body: ([OpaquePointer]) throws -> T) rethrows -> T {
        ensureInitialized()
        var list: UnsafeMutablePointer<OpaquePointer?>?
        let count = libusb_get_device_list(nil, &list)
        defer { libusb_free_device_list(list, 1) }
        let devices = (0..<max(count, 0)).compactMap { list?[$0] }
        return try body(devices)
    }

    private static let initialized: Bool = libusb_init(nil) == 0

    private static func ensureInitialized() {
        _ = initialized
    }
}

public enum USBError: Error, CustomStringConvertible {
    case heldByAnotherApp

    public var description: String {
        switch self {
        case .heldByAnotherApp:
            return "Another app is using the camera. Close EOS Utility, Image Capture or Photos."
        }
    }
}

/// macOS starts ptpcamerad the moment a camera appears, and it grabs the USB interface before we can.
public func freeCameraFromMacOS() {
    for daemon in ["ptpcamerad", "PTPCamera"] {
        let kill = Process()
        kill.executableURL = URL(fileURLWithPath: "/usr/bin/killall")
        kill.arguments = ["-9", daemon]
        kill.standardOutput = FileHandle.nullDevice
        kill.standardError = FileHandle.nullDevice
        try? kill.run()
        kill.waitUntilExit()
    }
}
