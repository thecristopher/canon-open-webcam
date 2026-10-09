import Testing
@testable import WebcamEngine

@Test func noArgumentsMeansStart() {
    #expect(CLICommand.parse([]) == .success(.start(alwaysOn: false)))
}

@Test func aBareFlagStillMeansStart() {
    #expect(CLICommand.parse(["--always-on"]) == .success(.start(alwaysOn: true)))
}

@Test func setNeedsBothANameAndAValue() {
    guard case .failure = CLICommand.parse(["set", "iso"]) else {
        Issue.record("set iso without a value should fail")
        return
    }
}

@Test func setPassesNameAndValueThrough() {
    #expect(CLICommand.parse(["set", "exposure", "-1"]) == .success(.set(name: "exposure", value: "-1")))
}

@Test func focusDefaultsToAutoWithOneStep() {
    #expect(CLICommand.parse(["focus"]) == .success(.focus(action: "auto", steps: 1)))
}

@Test func installCanBeAlwaysOn() {
    #expect(CLICommand.parse(["install", "--always-on"]) == .success(.install(alwaysOn: true)))
}

@Test func benchTakesSeconds() {
    #expect(CLICommand.parse(["bench", "3"]) == .success(.bench(seconds: 3)))
}

@Test func strangeWordsGetTheUsage() {
    guard case .failure(let error) = CLICommand.parse(["mutate"]) else {
        Issue.record("mutate is not a command")
        return
    }
    #expect(error.description.contains("canon-webcam status"))
}
