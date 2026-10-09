import CoreMedia
import CoreMediaIO
import CoreVideo
import Foundation

/// Feeds frames straight into OBS's camera extension, the same way OBS itself does, so OBS never has to run.
public final class VirtualCamera {
    public static let deviceName = "OBS Virtual Camera"
    public static let width = 1920
    public static let height = 1080

    private let device: CMIOObjectID
    private let sinkStream: CMIOObjectID
    private let queue: CMSimpleQueue

    private init(device: CMIOObjectID, sinkStream: CMIOObjectID, queue: CMSimpleQueue) {
        self.device = device
        self.sinkStream = sinkStream
        self.queue = queue
    }

    deinit {
        CMIODeviceStopStream(device, sinkStream)
    }

    /// Finds the extension and opens its sink. Nil when OBS's camera extension isn't installed or approved.
    public static func open() -> VirtualCamera? {
        CMIO.revealCameraExtensions()
        guard let device = CMIO.device(named: deviceName),
              let sink = CMIO.streams(of: device).first(where: { CMIO.isSinkStream($0) })
        else { return nil }

        var queueReference: Unmanaged<CMSimpleQueue>?
        let copied = CMIOStreamCopyBufferQueue(sink, { _, _, _ in }, nil, &queueReference)
        guard copied == noErr, let queueReference else { return nil }
        let queue = queueReference.takeRetainedValue()
        guard CMIODeviceStartStream(device, sink) == noErr else { return nil }
        return VirtualCamera(device: device, sinkStream: sink, queue: queue)
    }

    /// Hands a frame to the extension. Returns false when it was dropped because apps are behind.
    @discardableResult
    public func send(_ pixelBuffer: CVPixelBuffer, at time: CMTime = CMClockGetTime(CMClockGetHostTimeClock())) -> Bool {
        guard CMSimpleQueueGetCount(queue) < CMSimpleQueueGetCapacity(queue),
              let sample = makeSampleBuffer(pixelBuffer, at: time)
        else { return false }
        return CMSimpleQueueEnqueue(queue, element: Unmanaged.passRetained(sample).toOpaque()) == noErr
    }

    /// While we aren't feeding the camera, the system flag means exactly "some app opened it".
    public static func appsAreWatchingWhileIdle() -> Bool {
        CMIO.revealCameraExtensions()
        guard let device = CMIO.device(named: deviceName) else { return false }
        return CMIO.isRunningSomewhere(device)
    }

    /// Our own sink counts as "running" too, so we step off for a moment and see if anyone else is still there.
    /// An app that is watching sees OBS's logo for a frame during this, so the engine only probes when it must.
    /// Measured: with nobody watching the flag drops within about 10ms.
    public func probeAppsWatching() -> Bool {
        CMIODeviceStopStream(device, sinkStream)
        defer { CMIODeviceStartStream(device, sinkStream) }
        let giveUp = Date().addingTimeInterval(0.02)
        while Date() < giveUp {
            if !CMIO.isRunningSomewhere(device) { return false }
            usleep(1_000)
        }
        return true
    }

    private func makeSampleBuffer(_ pixelBuffer: CVPixelBuffer, at time: CMTime) -> CMSampleBuffer? {
        var format: CMVideoFormatDescription?
        guard CMVideoFormatDescriptionCreateForImageBuffer(allocator: nil, imageBuffer: pixelBuffer, formatDescriptionOut: &format) == noErr,
              let format
        else { return nil }
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: time, decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?
        CMSampleBufferCreateReadyWithImageBuffer(
            allocator: nil, imageBuffer: pixelBuffer, formatDescription: format,
            sampleTiming: &timing, sampleBufferOut: &sample
        )
        return sample
    }
}

/// The handful of CoreMediaIO property reads we need, without the pointer gymnastics leaking everywhere.
enum CMIO {
    static func revealCameraExtensions() {
        // Camera extensions stay invisible to CMIO clients unless we opt in
        var address = globalAddress(kCMIOHardwarePropertyAllowScreenCaptureDevices)
        var allow: UInt32 = 1
        CMIOObjectSetPropertyData(CMIOObjectID(kCMIOObjectSystemObject), &address, 0, nil, 4, &allow)
    }

    static func device(named name: String) -> CMIOObjectID? {
        objectIDs(CMIOObjectID(kCMIOObjectSystemObject), kCMIOHardwarePropertyDevices).first { self.name(of: $0) == name }
    }

    static func streams(of device: CMIOObjectID) -> [CMIOObjectID] {
        objectIDs(device, kCMIODevicePropertyStreams)
    }

    static func isSinkStream(_ stream: CMIOObjectID) -> Bool {
        // Direction 0 is the side a feeder writes to, apps read from direction 1
        uint32(stream, kCMIOStreamPropertyDirection) == 0
    }

    static func isRunningSomewhere(_ device: CMIOObjectID) -> Bool {
        uint32(device, kCMIODevicePropertyDeviceIsRunningSomewhere) == 1
    }

    static func name(of object: CMIOObjectID) -> String? {
        var address = globalAddress(kCMIOObjectPropertyName)
        var name: Unmanaged<CFString>?
        var used: UInt32 = 0
        let status = CMIOObjectGetPropertyData(object, &address, 0, nil, UInt32(MemoryLayout<CFString?>.size), &used, &name)
        guard status == noErr else { return nil }
        return name?.takeRetainedValue() as String?
    }

    private static func objectIDs(_ object: CMIOObjectID, _ selector: Int) -> [CMIOObjectID] {
        var address = globalAddress(selector)
        var size: UInt32 = 0
        guard CMIOObjectGetPropertyDataSize(object, &address, 0, nil, &size) == noErr else { return [] }
        var ids = [CMIOObjectID](repeating: 0, count: Int(size) / MemoryLayout<CMIOObjectID>.size)
        var used: UInt32 = 0
        guard CMIOObjectGetPropertyData(object, &address, 0, nil, size, &used, &ids) == noErr else { return [] }
        return ids
    }

    private static func uint32(_ object: CMIOObjectID, _ selector: Int) -> UInt32? {
        var address = globalAddress(selector)
        var value: UInt32 = 0
        var used: UInt32 = 0
        guard CMIOObjectGetPropertyData(object, &address, 0, nil, 4, &used, &value) == noErr else { return nil }
        return value
    }

    private static func globalAddress(_ selector: Int) -> CMIOObjectPropertyAddress {
        CMIOObjectPropertyAddress(
            mSelector: CMIOObjectPropertySelector(selector),
            mScope: CMIOObjectPropertyScope(kCMIOObjectPropertyScopeGlobal),
            mElement: CMIOObjectPropertyElement(kCMIOObjectPropertyElementMain)
        )
    }
}
