import Foundation

// PTP is little endian all the way down

extension Data {
    public func uint16(at offset: Int) -> UInt16? {
        guard offset >= 0, offset + 2 <= count else { return nil }
        let start = startIndex + offset
        return UInt16(self[start]) | UInt16(self[start + 1]) << 8
    }

    public func uint32(at offset: Int) -> UInt32? {
        guard offset >= 0, offset + 4 <= count else { return nil }
        let start = startIndex + offset
        return (0..<4).reduce(UInt32(0)) { value, byte in value | UInt32(self[start + byte]) << (8 * byte) }
    }

    mutating func append(littleEndian value: UInt16) {
        Swift.withUnsafeBytes(of: value.littleEndian) { append(contentsOf: $0) }
    }

    mutating func append(littleEndian value: UInt32) {
        Swift.withUnsafeBytes(of: value.littleEndian) { append(contentsOf: $0) }
    }
}
