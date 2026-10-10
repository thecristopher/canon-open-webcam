import Testing
@testable import CanonPTP

@Test func isoCodesReadAsNumbers() {
    #expect(CameraSetting.iso.label(for: 0x58) == "400")
}

@Test func apertureLabelsGetTheFStopPrefix() {
    #expect(CameraSetting.aperture.label(for: 0x30) == "f/5.6")
}

@Test func apertureZeroMeansTheCameraPicks() {
    #expect(CameraSetting.aperture.label(for: 0) == "auto")
}

@Test func unknownCodesShowUpAsHexInsteadOfLying() {
    #expect(CameraSetting.iso.label(for: 0x99) == "0x99")
}

@Test func typingAnFStopFindsItsCode() {
    #expect(CameraSetting.aperture.code(for: "f/5.6", choices: []) == 0x30)
}

@Test func duplicateLabelsPreferWhatTheCameraAccepts() {
    #expect(CameraSetting.aperture.code(for: "4.5", choices: [0x2C]) == 0x2C)
}

@Test func plusSignsAreOptionalForExposure() {
    #expect(CameraSetting.exposure.code(for: "+1", choices: []) == 0x08)
}

@Test func whiteBalanceNamesIgnoreCase() {
    #expect(CameraSetting.whiteBalance.code(for: "Daylight", choices: []) == 1)
}

@Test func colorTemperatureTakesKelvin() {
    #expect(CameraSetting.colorTemperature.code(for: "5200K", choices: []) == 5200)
}

@Test func nonsenseInputFindsNothing() {
    #expect(CameraSetting.iso.code(for: "over 9000", choices: []) == nil)
}
