import CanonPTP
import Foundation
import Testing
@testable import WebcamEngine

@Test func setIsoBecomesACameraCommand() {
    #expect(EngineCommand.from(.set("iso", to: "400")) == .success(.setCamera(.iso, "400")))
}

@Test func mirrorUnderstandsOnAndOff() {
    #expect(EngineCommand.from(.set("mirror", to: "on")) == .success(.setMirror(true)))
    #expect(EngineCommand.from(.set("mirror", to: "off")) == .success(.setMirror(false)))
}

@Test func mirrorRejectsMaybe() {
    guard case .failure = EngineCommand.from(.set("mirror", to: "maybe")) else {
        Issue.record("maybe is not a mirror setting")
        return
    }
}

@Test func zoomAcceptsAnXSuffix() {
    #expect(EngineCommand.from(.set("zoom", to: "1.5x")) == .success(.setZoom(1.5)))
}

@Test func zoomPastThreeIsRefused() {
    guard case .failure = EngineCommand.from(.set("zoom", to: "8")) else {
        Issue.record("zoom 8 should be refused")
        return
    }
}

@Test func unknownSettingsListWhatDoesExist() {
    guard case .failure(let error) = EngineCommand.from(.set("plaga", to: "on")) else {
        Issue.record("plaga is not a setting")
        return
    }
    #expect(error.description.contains("white-balance"))
}

@Test func focusNearTwoIsAMediumStep() {
    #expect(EngineCommand.from(.focus("near", steps: 2)) == .success(.focus(.nearMedium)))
}

@Test func focusStepsAreClampedToCanonsThreeSizes() {
    #expect(EngineCommand.from(.focus("far", steps: 9)) == .success(.focus(.farLarge)))
}

@Test func plainFocusMeansAutofocus() {
    #expect(EngineCommand.from(.focus("auto")) == .success(.autofocus))
}

@Test func requestsSurviveTheTripThroughJSON() throws {
    let request = ControlRequest.set("white-balance", to: "daylight")
    let decoded = try JSONDecoder().decode(ControlRequest.self, from: JSONEncoder().encode(request))
    #expect(decoded == request)
}

@Test func theSocketCarriesARequestToTheHandlerAndBack() throws {
    let path = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("cw-\(UUID().uuidString.prefix(8)).sock")
    let server = ControlServer(path: path) { request in .success("Hello \(request.name ?? "")") }
    try server.start()
    let response = try ControlClient.send(.set("Melina", to: "x"), to: path)
    #expect(response.message == "Hello Melina")
}

@Test func talkingToNobodySaysTheEngineIsntRunning() {
    let nowhere = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("cw-missing.sock")
    #expect(throws: SocketError.self) { try ControlClient.send(.status, to: nowhere) }
}
