import Foundation
import Testing
@testable import CanonPTP

@Test func openSessionAlwaysUsesTransactionZero() throws {
    let camera = FakeCamera()
    try PTPSession(transport: camera).run(.openSession, [1])
    #expect(camera.commands.first?.transactionID == 0)
}

@Test func transactionsCountUpAfterOpening() throws {
    let camera = FakeCamera()
    let session = PTPSession(transport: camera)
    try session.run(.openSession, [1])
    try session.run(.canonGetEvent)
    try session.run(.canonGetEvent)
    #expect(camera.commands.map(\.transactionID) == [0, 1, 2])
}

@Test func dataSentByTheCameraLandsInTheReply() throws {
    let camera = FakeCamera()
    camera.answer(.ok, data: Data([1, 2, 3]))
    let reply = try PTPSession(transport: camera).run(.canonGetEvent)
    #expect(reply.data == Data([1, 2, 3]))
}

@Test func outgoingDataFollowsTheCommandWithTheSameTransaction() throws {
    let camera = FakeCamera()
    try PTPSession(transport: camera).run(.canonSetPropertyValue, sending: Data([9]))
    #expect(camera.sent.map(\.type) == [.command, .data])
    #expect(camera.sent[0].transactionID == camera.sent[1].transactionID)
}

@Test func requireTurnsABadAnswerIntoAnError() {
    let camera = FakeCamera()
    camera.answer(.generalError)
    #expect(throws: PTPError.response(0x2002, during: 0x9116)) {
        try PTPSession(transport: camera).require(.canonGetEvent)
    }
}
