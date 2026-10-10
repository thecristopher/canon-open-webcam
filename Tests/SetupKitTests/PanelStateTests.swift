import Testing
import WebcamEngine
@testable import SetupKit

private func status(_ kind: PhaseKind, trouble: String? = nil, fps: Double = 0) -> EngineStatus {
    EngineStatus(state: "", kind: kind, trouble: trouble, model: "Canon EOS Rebel T7", lens: "EF-S18-55mm", framesPerSecond: fps)
}

@Test func noAnswerFromTheEngineMeansItIsNotRunning() {
    #expect(PanelState.from(nil) == .notRunning)
}

@Test func liveShowsTheModelLensAndSpeed() {
    let state = PanelState.from(status(.live, fps: 24.3))
    #expect(state.title == "Canon EOS Rebel T7")
    #expect(state.subtitle == "EF-S18-55mm · 24 fps")
}

@Test func troubleCarriesTheEnginesReason() {
    let state = PanelState.from(status(.trouble, trouble: "Leon unplugged it"))
    #expect(state.subtitle == "Leon unplugged it")
}

@Test func theMenuBarIconFillsInWhileLive() {
    #expect(PanelState.from(status(.live)).menuBarSymbol == "web.camera.fill")
    #expect(PanelState.from(status(.idle)).menuBarSymbol == "web.camera")
}

@Test func aStuckEngineGetsAWarningIcon() {
    #expect(PanelState.from(status(.trouble, trouble: "busy")).menuBarSymbol == "exclamationmark.triangle")
}

@Test func aSettingTheCameraHasNotReportedIsUnavailable() {
    #expect(SettingControl.from(nil) == .unavailable)
}

@Test func aSettingWithNoChoicesIsLockedByTheModeDial() {
    #expect(SettingControl.from(SettingStatus(value: "auto", choices: [])) == .lockedByMode(current: "auto"))
}

@Test func aSettingWithChoicesBecomesAPicker() {
    let control = SettingControl.from(SettingStatus(value: "400", choices: ["100", "400"]))
    #expect(control == .choices(["100", "400"], current: "400"))
}
