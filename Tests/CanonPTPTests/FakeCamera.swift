import Foundation
@testable import CanonPTP

/// Pretends to be a camera on the other end of the cable. Each command pops the next scripted answer.
final class FakeCamera: Transport {
    struct Answer {
        var code: ResponseCode
        var data: Data?
    }

    var answers: [Answer] = []
    private(set) var sent: [Container] = []
    private var inbox: [Data] = []

    func answer(_ code: ResponseCode, data: Data? = nil) {
        answers.append(Answer(code: code, data: data))
    }

    var commands: [Container] { sent.filter { $0.type == .command } }

    func send(_ bytes: Data) throws {
        let container = try Container.decode(bytes)
        sent.append(container)
        guard container.type == .command else { return }
        let answer = answers.isEmpty ? Answer(code: .ok) : answers.removeFirst()
        if let data = answer.data {
            inbox.append(Container(type: .data, code: container.code, transactionID: container.transactionID, payload: data).encoded())
        }
        inbox.append(Container(type: .response, code: answer.code.rawValue, transactionID: container.transactionID).encoded())
    }

    func receiveContainer() throws -> Data {
        guard !inbox.isEmpty else { throw PTPError.transport("fake camera has nothing to say") }
        return inbox.removeFirst()
    }
}

func littleEndian(_ values: UInt32...) -> Data {
    Container.packed(values)
}
