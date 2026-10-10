import Foundation

/// One PTP packet: a 12 byte header (length, type, code, transaction id) and a payload.
public struct Container: Equatable, Sendable {
    public static let headerSize = 12

    public var type: ContainerType
    public var code: UInt16
    public var transactionID: UInt32
    public var payload: Data

    public init(type: ContainerType, code: UInt16, transactionID: UInt32, payload: Data = Data()) {
        self.type = type
        self.code = code
        self.transactionID = transactionID
        self.payload = payload
    }

    public static func command(_ operation: Operation, transactionID: UInt32, parameters: [UInt32] = []) -> Container {
        Container(type: .command, code: operation.rawValue, transactionID: transactionID, payload: packed(parameters))
    }

    public static func data(for operation: Operation, transactionID: UInt32, payload: Data) -> Container {
        Container(type: .data, code: operation.rawValue, transactionID: transactionID, payload: payload)
    }

    public func encoded() -> Data {
        var bytes = Data(capacity: Container.headerSize + payload.count)
        bytes.append(littleEndian: UInt32(Container.headerSize + payload.count))
        bytes.append(littleEndian: type.rawValue)
        bytes.append(littleEndian: code)
        bytes.append(littleEndian: transactionID)
        bytes.append(payload)
        return bytes
    }

    /// Total size the header promises, so the reader knows when a multi packet transfer is complete.
    public static func declaredLength(of bytes: Data) -> Int? {
        bytes.uint32(at: 0).map(Int.init)
    }

    public static func decode(_ bytes: Data) throws -> Container {
        guard let length = bytes.uint32(at: 0), let rawType = bytes.uint16(at: 4),
              let code = bytes.uint16(at: 6), let transactionID = bytes.uint32(at: 8)
        else { throw PTPError.malformed("container shorter than its header") }
        guard let type = ContainerType(rawValue: rawType) else {
            throw PTPError.malformed("unknown container type \(rawType)")
        }
        guard Int(length) <= bytes.count, Int(length) >= headerSize else {
            throw PTPError.malformed("container says \(length) bytes but has \(bytes.count)")
        }
        let payload = bytes.subdata(in: bytes.startIndex + headerSize ..< bytes.startIndex + Int(length))
        return Container(type: type, code: code, transactionID: transactionID, payload: payload)
    }

    public var parameters: [UInt32] {
        stride(from: 0, to: payload.count - 3, by: 4).compactMap { payload.uint32(at: $0) }
    }

    static func packed(_ values: [UInt32]) -> Data {
        var bytes = Data()
        values.forEach { bytes.append(littleEndian: $0) }
        return bytes
    }
}

public enum PTPError: Error, Equatable, CustomStringConvertible {
    case malformed(String)
    case response(UInt16, during: UInt16)
    case transport(String)
    case unexpectedTransaction(expected: UInt32, got: UInt32)

    public var description: String {
        switch self {
        case .malformed(let detail):
            return "camera sent something odd: \(detail)"
        case .response(let code, let operation):
            return String(format: "camera answered 0x%04X to operation 0x%04X", code, operation)
        case .transport(let detail):
            return "USB trouble: \(detail)"
        case .unexpectedTransaction(let expected, let got):
            return "camera answered transaction \(got) while we waited for \(expected)"
        }
    }
}
