import Foundation
import Testing
@testable import WebcamEngine

private func scratchFile() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("canon-webcam-tests-\(UUID().uuidString)/options.json")
}

@Test func optionsComeBackTheWayTheyWereSaved() throws {
    let store = OptionsStore(file: scratchFile())
    try store.save(FrameOptions(mirror: true, zoom: 1.5))
    #expect(store.load() == FrameOptions(mirror: true, zoom: 1.5))
}

@Test func aMissingFileMeansDefaults() {
    #expect(OptionsStore(file: scratchFile()).load() == FrameOptions())
}

@Test func aCorruptFileMeansDefaultsInsteadOfACrash() throws {
    let file = scratchFile()
    try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
    try Data("Nemesis was here".utf8).write(to: file)
    #expect(OptionsStore(file: file).load() == FrameOptions())
}
