import Foundation
import Testing
@testable import CanonPTP

private func ptpString(_ text: String) -> Data {
    let units = Array(text.utf16) + [0]
    var bytes = Data([UInt8(units.count)])
    units.forEach { bytes.append(littleEndian: $0) }
    return bytes
}

private func deviceInfo(manufacturer: String, model: String) -> Data {
    var bytes = Data([100, 0]) + littleEndian(6) + Data([100, 0])
    bytes += ptpString("microsoft.com: 1.0")
    bytes += Data([0, 0])
    // Five arrays (operations, events, properties, capture and image formats), one entry each
    for _ in 0..<5 { bytes += littleEndian(1) + Data([0x01, 0x10]) }
    bytes += ptpString(manufacturer) + ptpString(model)
    return bytes
}

@Test func theBrandIsNotSaidTwice() {
    let info = deviceInfo(manufacturer: "Canon Inc.", model: "Canon EOS Rebel T7")
    #expect(modelName(fromDeviceInfo: info) == "Canon EOS Rebel T7")
}

@Test func aModelWithoutTheBrandGetsItPrepended() {
    let info = deviceInfo(manufacturer: "Umbrella Corporation", model: "STARS Cam")
    #expect(modelName(fromDeviceInfo: info) == "Umbrella Corporation STARS Cam")
}

@Test func aTruncatedDatasetGivesNoName() {
    #expect(modelName(fromDeviceInfo: Data([100, 0, 6])) == nil)
}
