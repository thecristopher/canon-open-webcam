import Foundation

/// Pulls "Canon EOS 1500D" out of the PTP DeviceInfo dataset, since the USB name is just "Canon Digital Camera".
public func modelName(fromDeviceInfo info: Data) -> String? {
    var reader = DatasetReader(info)
    // Fixed header: standard version, vendor extension id and version, extension description, functional mode
    guard reader.skip(2 + 4 + 2), reader.skipString(), reader.skip(2) else { return nil }
    // Operations, events, properties, capture formats, image formats
    for _ in 0..<5 {
        guard reader.skipUInt16Array() else { return nil }
    }
    guard let manufacturer = reader.string(), let model = reader.string() else { return nil }
    // "Canon Inc." plus "Canon EOS Rebel T7" would say Canon twice
    let brand = manufacturer.split(separator: " ").first.map { $0.lowercased() } ?? ""
    if brand.isEmpty || model.lowercased().hasPrefix(brand) { return model }
    return "\(manufacturer) \(model)"
}

/// Walks PTP datasets, where strings are a length byte followed by UTF-16 characters.
struct DatasetReader {
    private let bytes: Data
    private var offset = 0

    init(_ bytes: Data) {
        self.bytes = bytes
    }

    mutating func skip(_ count: Int) -> Bool {
        guard offset + count <= bytes.count else { return false }
        offset += count
        return true
    }

    mutating func skipString() -> Bool {
        string() != nil
    }

    mutating func skipUInt16Array() -> Bool {
        guard let count = bytes.uint32(at: offset) else { return false }
        return skip(4 + Int(count) * 2)
    }

    mutating func string() -> String? {
        guard offset < bytes.count else { return nil }
        let characterCount = Int(bytes[bytes.startIndex + offset])
        offset += 1
        var units: [UInt16] = []
        for _ in 0..<characterCount {
            guard let unit = bytes.uint16(at: offset) else { return nil }
            units.append(unit)
            offset += 2
        }
        // The count includes the trailing null
        return String(decoding: units.prefix { $0 != 0 }, as: UTF16.self)
    }
}
