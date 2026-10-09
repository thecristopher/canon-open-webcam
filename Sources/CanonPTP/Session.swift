import Foundation

/// Anything that can move whole PTP containers back and forth. USB in real life, a script in tests.
public protocol Transport: AnyObject {
    func send(_ bytes: Data) throws
    /// Returns exactly one container, already stitched together from however many USB packets it took.
    func receiveContainer() throws -> Data
}

public struct Reply: Equatable, Sendable {
    public var code: UInt16
    public var data: Data
    public var parameters: [UInt32]

    public var isOK: Bool { code == ResponseCode.ok.rawValue }
}

/// Plain PTP transactions: command out, optional data out or in, response back.
public final class PTPSession {
    private let transport: Transport
    private var nextTransactionID: UInt32 = 0

    public init(transport: Transport) {
        self.transport = transport
    }

    @discardableResult
    public func run(_ operation: Operation, _ parameters: [UInt32] = [], sending outgoing: Data? = nil) throws -> Reply {
        // OpenSession must be transaction 0, everything after counts up from there
        let transactionID = operation == .openSession ? 0 : takeTransactionID()
        try transport.send(Container.command(operation, transactionID: transactionID, parameters: parameters).encoded())
        if let outgoing {
            try transport.send(Container.data(for: operation, transactionID: transactionID, payload: outgoing).encoded())
        }

        var incoming = Data()
        while true {
            let container = try Container.decode(transport.receiveContainer())
            switch container.type {
            case .data:
                incoming.append(container.payload)
            case .response:
                guard container.transactionID == transactionID else {
                    throw PTPError.unexpectedTransaction(expected: transactionID, got: container.transactionID)
                }
                return Reply(code: container.code, data: incoming, parameters: container.parameters)
            case .command, .event:
                // Stray events on the bulk pipe are noise, the interesting ones come through GetEvent
                continue
            }
        }
    }

    /// Same as run, but anything other than OK becomes an error.
    @discardableResult
    public func require(_ operation: Operation, _ parameters: [UInt32] = [], sending outgoing: Data? = nil) throws -> Reply {
        let reply = try run(operation, parameters, sending: outgoing)
        guard reply.isOK else { throw PTPError.response(reply.code, during: operation.rawValue) }
        return reply
    }

    private func takeTransactionID() -> UInt32 {
        nextTransactionID &+= 1
        if nextTransactionID == 0 || nextTransactionID == .max { nextTransactionID = 1 }
        return nextTransactionID
    }
}
