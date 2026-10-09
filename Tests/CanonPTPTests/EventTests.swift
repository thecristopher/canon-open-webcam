import Foundation
import Testing
@testable import CanonPTP

private func record(_ words: UInt32...) -> Data {
    var body = Container.packed(words)
    body.insert(contentsOf: littleEndian(UInt32(body.count + 4)), at: 0)
    return body
}

private let endOfEvents = littleEndian(8, 0)

@Test func aPropertyChangeCarriesItsNewValue() {
    let blob = record(0xC189, 0xD103, 0x58) + endOfEvents
    #expect(parseCanonEvents(blob) == [.propertyChanged(0xD103, value: 0x58)])
}

@Test func anAvailableListCarriesEveryChoice() {
    let blob = record(0xC18A, 0xD103, 3, 3, 0x48, 0x50, 0x58) + endOfEvents
    #expect(parseCanonEvents(blob) == [.choicesChanged(0xD103, choices: [0x48, 0x50, 0x58])])
}

@Test func theLensNameComesThroughAsText() {
    var blob = littleEndian(0, 0xC189, 0xD1D8) + Data("Ranni's Moonlight 50mm".utf8) + Data([0, 0])
    blob.replaceSubrange(0..<4, with: littleEndian(UInt32(blob.count)))
    #expect(parseCanonEvents(blob + endOfEvents) == [.textPropertyChanged(0xD1D8, text: "Ranni's Moonlight 50mm")])
}

@Test func parsingStopsAtTheTerminator() {
    let blob = endOfEvents + record(0xC189, 0xD103, 0x58)
    #expect(parseCanonEvents(blob).isEmpty)
}

@Test func unknownEventTypesAreSkippedNotFatal() {
    let blob = record(0xC1A7, 1, 2) + record(0xC189, 0xD101, 0x30) + endOfEvents
    #expect(parseCanonEvents(blob) == [.propertyChanged(0xD101, value: 0x30)])
}

@Test func aRecordClaimingMoreBytesThanExistIsDropped() {
    let blob = littleEndian(400, 0xC189, 0xD103, 0x58)
    #expect(parseCanonEvents(blob).isEmpty)
}

@Test func stateRemembersTheLatestValues() {
    var state = CameraState()
    state.apply([.propertyChanged(0xD103, value: 0x48), .propertyChanged(0xD103, value: 0x58)])
    #expect(state.value(of: .iso) == 0x58)
}
