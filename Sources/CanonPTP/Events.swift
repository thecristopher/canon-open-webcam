import Foundation

public enum CameraEvent: Equatable, Sendable {
    case propertyChanged(UInt32, value: UInt32)
    case textPropertyChanged(UInt32, text: String)
    case choicesChanged(UInt32, choices: [UInt32])
}

/// Canon packs every event into one GetEvent blob of [size][type][body] records.
public func parseCanonEvents(_ blob: Data) -> [CameraEvent] {
    var events: [CameraEvent] = []
    var offset = 0
    while let size = blob.uint32(at: offset).map(Int.init), let type = blob.uint32(at: offset + 4) {
        // A zero type or a bogus size marks the end of the list
        guard type != 0, size >= 8, offset + size <= blob.count else { break }
        let record = blob.subdata(in: blob.startIndex + offset ..< blob.startIndex + offset + size)
        if let event = parseRecord(type: type, record: record) {
            events.append(event)
        }
        offset += size
    }
    return events
}

private func parseRecord(type: UInt32, record: Data) -> CameraEvent? {
    guard let property = record.uint32(at: 8) else { return nil }
    switch CanonEvent(rawValue: type) {
    case .propertyChanged:
        if record.count == 16, let value = record.uint32(at: 12) {
            return .propertyChanged(property, value: value)
        }
        if property == CanonProperty.lensName.rawValue {
            return .textPropertyChanged(property, text: cString(record.dropFirst(12)))
        }
        return nil
    case .availableValuesChanged:
        guard let count = record.uint32(at: 16) else { return nil }
        let choices = (0..<Int(count)).compactMap { record.uint32(at: 20 + 4 * $0) }
        return .choicesChanged(property, choices: choices)
    case nil:
        return nil
    }
}

private func cString(_ bytes: Data) -> String {
    let text = bytes.prefix { $0 != 0 }
    return String(decoding: text, as: UTF8.self)
}
