import Foundation
import Testing
@testable import CanonPTP

@Test func commandEncodesLengthTypeCodeTransactionAndParameters() {
    let bytes = Container.command(.openSession, transactionID: 0, parameters: [1]).encoded()
    #expect(bytes == Data([16, 0, 0, 0, 1, 0, 0x02, 0x10, 0, 0, 0, 0, 1, 0, 0, 0]))
}

@Test func decodingGivesBackWhatWasEncoded() throws {
    let original = Container(type: .response, code: 0x2001, transactionID: 7, payload: littleEndian(42))
    #expect(try Container.decode(original.encoded()) == original)
}

@Test func decodingIgnoresBytesPastTheDeclaredLength() throws {
    var bytes = Container(type: .response, code: 0x2001, transactionID: 3).encoded()
    bytes.append(contentsOf: [0xDE, 0xAD])
    #expect(try Container.decode(bytes).payload.isEmpty)
}

@Test func decodingATruncatedHeaderThrows() {
    #expect(throws: PTPError.self) { try Container.decode(Data([12, 0, 0])) }
}

@Test func decodingAnUnknownTypeThrows() {
    let bytes = Data([12, 0, 0, 0, 9, 0, 0x01, 0x20, 0, 0, 0, 0])
    #expect(throws: PTPError.self) { try Container.decode(bytes) }
}

@Test func parametersReadAsLittleEndianWords() {
    let container = Container(type: .response, code: 0x2001, transactionID: 1, payload: littleEndian(5, 0xA102))
    #expect(container.parameters == [5, 0xA102])
}
