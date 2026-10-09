import Foundation
import Testing
@testable import CanonPTP

private let fakeJPEG = Data([0xFF, 0xD8, 0xFF, 0xE0, 0x42, 0xFF, 0xD9])

private func viewFinderBlob(type: UInt32, payload: Data) -> Data {
    littleEndian(UInt32(payload.count + 8), type) + payload
}

@Test func theJPEGIsPulledOutOfTheViewFinderBlob() {
    #expect(jpegFromViewFinder(viewFinderBlob(type: 1, payload: fakeJPEG)) == fakeJPEG)
}

@Test func metadataBlobsBeforeTheJPEGAreSkipped() {
    let blob = viewFinderBlob(type: 4, payload: Data([1, 2, 3, 4])) + viewFinderBlob(type: 1, payload: fakeJPEG)
    #expect(jpegFromViewFinder(blob) == fakeJPEG)
}

@Test func aBlobWithoutAJPEGGivesNothing() {
    #expect(jpegFromViewFinder(viewFinderBlob(type: 4, payload: Data([1]))) == nil)
}

@Test func nextFrameIsNilWhileTheCameraIsStillCooking() throws {
    let fake = FakeCamera()
    fake.answer(.canonNotReady)
    #expect(try CanonCamera(transport: fake).nextFrame() == nil)
}

@Test func nextFrameHandsBackTheJPEG() throws {
    let fake = FakeCamera()
    fake.answer(.ok, data: viewFinderBlob(type: 1, payload: fakeJPEG))
    #expect(try CanonCamera(transport: fake).nextFrame() == fakeJPEG)
}

@Test func connectRecoversFromAStaleSession() throws {
    let fake = FakeCamera()
    fake.answer(.sessionAlreadyOpen)
    try CanonCamera(transport: fake).connect()
    let codes = fake.commands.prefix(3).map(\.code)
    #expect(codes == [Operation.openSession.rawValue, Operation.closeSession.rawValue, Operation.openSession.rawValue])
}

@Test func connectLearnsPropertiesFromTheFirstEventDump() throws {
    let fake = FakeCamera()
    // open session, device info, remote mode, event mode, then the event dump
    for _ in 0..<4 { fake.answer(.ok) }
    fake.answer(.ok, data: littleEndian(16, 0xC189, 0xD103, 0x58, 8, 0))
    let camera = CanonCamera(transport: fake)
    try camera.connect()
    #expect(camera.state.value(of: .iso) == 0x58)
}

@Test func settingAPropertySendsSizeCodeAndValue() throws {
    let fake = FakeCamera()
    try CanonCamera(transport: fake).set(.iso, to: 0x58)
    #expect(fake.sent.last?.payload == littleEndian(12, 0xD103, 0x58))
}

@Test func liveViewGoesToTheComputer() throws {
    let fake = FakeCamera()
    let camera = CanonCamera(transport: fake)
    try camera.startLiveView()
    #expect(camera.state.value(of: .liveViewOutput) == LiveViewOutput.computer)
}

@Test func movieModeAnsweringBusyDoesNotStopLiveView() throws {
    let fake = FakeCamera()
    fake.answer(.deviceBusy)
    let camera = CanonCamera(transport: fake)
    try camera.startLiveView()
    #expect(camera.state.value(of: .liveViewOutput) == LiveViewOutput.computer)
}

@Test func disconnectGivesTheScreenBackToTheCamera() throws {
    let fake = FakeCamera()
    let camera = CanonCamera(transport: fake)
    try camera.startLiveView()
    camera.disconnect()
    #expect(camera.state.value(of: .liveViewOutput) == LiveViewOutput.cameraScreen)
}
