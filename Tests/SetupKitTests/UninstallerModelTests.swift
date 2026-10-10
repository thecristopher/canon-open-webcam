import Testing
@testable import SetupKit

@MainActor
@Test func nothingInstalledMeansNothingToRemove() {
    let model = UninstallerModel(pieces: [], canonUtilityIsPresent: false)
    #expect(model.stage == .nothingInstalled)
}

@MainActor
@Test func anythingInstalledMeansReadyToRemove() {
    let model = UninstallerModel(pieces: ["Its log file"], canonUtilityIsPresent: true)
    #expect(model.stage == .ready)
}

@MainActor
@Test func canonStaysOffUnlessYouAskForIt() {
    let model = UninstallerModel(pieces: ["Its log file"], canonUtilityIsPresent: true)
    #expect(model.restoreCanon == false)
}
