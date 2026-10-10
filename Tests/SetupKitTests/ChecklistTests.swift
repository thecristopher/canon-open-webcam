import Testing
@testable import SetupKit

private func mac(obs: Bool = true, driver: Bool = true, installed: Bool = false, camera: Bool = true, homebrew: Bool = true) -> InstallerFacts {
    InstallerFacts(obsInstalled: obs, driverReady: driver, installed: installed, cameraConnected: camera, hasHomebrew: homebrew)
}

@Test func withoutOBSTheFirstButtonInstallsIt() {
    let status = Checklist.status(of: .driver, facts: mac(obs: false, driver: false), activity: .idle)
    guard case .needsAction(let button, _) = status else { Issue.record("expected a button, got \(status)"); return }
    #expect(button == "Install OBS")
}

@Test func withoutHomebrewWeSendYouToTheDownloadPage() {
    let status = Checklist.status(of: .driver, facts: mac(obs: false, driver: false, homebrew: false), activity: .idle)
    guard case .needsAction(let button, _) = status else { Issue.record("expected a button, got \(status)"); return }
    #expect(button == "Download OBS")
}

@Test func withOBSButNoDriverWeOfferToSetItUp() {
    let status = Checklist.status(of: .driver, facts: mac(driver: false), activity: .idle)
    guard case .needsAction(let button, _) = status else { Issue.record("expected a button, got \(status)"); return }
    #expect(button == "Set Up Driver")
}

@Test func whileWaitingForApprovalWeSayWhereToClick() {
    let status = Checklist.status(of: .driver, facts: mac(driver: false), activity: .waitingForApproval)
    guard case .working(let text) = status else { Issue.record("expected working, got \(status)"); return }
    #expect(text.contains("System Settings"))
}

@Test func installWaitsForTheDriver() {
    let status = Checklist.status(of: .install, facts: mac(driver: false), activity: .idle)
    #expect(status == .waitingOn("Set up the camera driver first"))
}

@Test func installIsOneClickOnceTheDriverIsReady() {
    let status = Checklist.status(of: .install, facts: mac(), activity: .idle)
    guard case .needsAction(let button, _) = status else { Issue.record("expected a button, got \(status)"); return }
    #expect(button == "Install")
}

@Test func aMissingCameraNeverBlocksInstalling() {
    let status = Checklist.status(of: .install, facts: mac(camera: false), activity: .idle)
    guard case .needsAction = status else { Issue.record("install should not wait on the camera, got \(status)"); return }
}

@Test func driverAndInstallTogetherMeanWereFinished() {
    #expect(Checklist.isFinished(mac(installed: true, camera: false)))
    #expect(Checklist.isFinished(mac(driver: false, installed: true)) == false)
}
