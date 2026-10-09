import Foundation

/// GetViewFinderData returns [length][type][bytes] blobs, and the JPEG is the one typed 1 or 11.
public func jpegFromViewFinder(_ blob: Data) -> Data? {
    let jpegTypes: Set<UInt32> = [1, 11]
    var offset = 0
    while let length = blob.uint32(at: offset).map(Int.init), let type = blob.uint32(at: offset + 4) {
        guard length >= 8, offset + length <= blob.count else { return nil }
        if jpegTypes.contains(type) {
            return blob.subdata(in: blob.startIndex + offset + 8 ..< blob.startIndex + offset + length)
        }
        offset += length
    }
    return nil
}
