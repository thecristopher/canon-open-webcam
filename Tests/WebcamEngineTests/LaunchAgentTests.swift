import Foundation
import Testing
@testable import WebcamEngine

@Test func theAgentRunsStartFromTheInstalledBinary() {
    let plist = LaunchAgent.plist(binary: "/opt/ada/canon-webcam", alwaysOn: false, logPath: "/tmp/log")
    #expect(plist["ProgramArguments"] as? [String] == ["/opt/ada/canon-webcam", "start"])
}

@Test func alwaysOnIsPassedThroughToTheAgent() {
    let plist = LaunchAgent.plist(binary: "/opt/ada/canon-webcam", alwaysOn: true, logPath: "/tmp/log")
    #expect(plist["ProgramArguments"] as? [String] == ["/opt/ada/canon-webcam", "start", "--always-on"])
}

@Test func theAgentComesBackWhenItDies() {
    let plist = LaunchAgent.plist(binary: "/opt/ada/canon-webcam", alwaysOn: false, logPath: "/tmp/log")
    #expect(plist["KeepAlive"] as? Bool == true)
}

@Test func disabledServicesAreReadInTheNewFormat() {
    let output = """
    disabled services = {
        "com.canon.usa.EWCService" => disabled
        "com.apple.Siri.agent" => enabled
    }
    """
    #expect(LaunchAgent.disabledServices(fromPrintDisabled: output) == ["com.canon.usa.EWCService"])
}

@Test func disabledServicesAreReadInTheOldFormat() {
    let output = "\"com.canon.usa.EWCPairingService\" => true\n\"com.apple.Siri.agent\" => false"
    #expect(LaunchAgent.disabledServices(fromPrintDisabled: output) == ["com.canon.usa.EWCPairingService"])
}

private func scratchFolder() throws -> URL {
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent("canon-webcam-link-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    return folder
}

@Test func theCommandGetsLinkedOntoThePath() throws {
    let folder = try scratchFolder()
    let binary = folder.appendingPathComponent("canon-webcam")
    let link = folder.appendingPathComponent("bin/canon-webcam")
    try LaunchAgent.linkCommand(to: binary, from: link)
    #expect(try FileManager.default.destinationOfSymbolicLink(atPath: link.path) == binary.path)
}

@Test func someoneElsesFileIsNeverReplaced() throws {
    let folder = try scratchFolder()
    let link = folder.appendingPathComponent("canon-webcam")
    try Data("Chris Redfield's script".utf8).write(to: link)
    try LaunchAgent.linkCommand(to: folder.appendingPathComponent("ours"), from: link)
    #expect(try String(contentsOf: link, encoding: .utf8) == "Chris Redfield's script")
}

@Test func uninstallOnlyRemovesOurOwnLink() throws {
    let folder = try scratchFolder()
    let link = folder.appendingPathComponent("canon-webcam")
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: folder.appendingPathComponent("theirs"))
    LaunchAgent.unlinkCommand(at: link, pointingTo: folder.appendingPathComponent("ours"))
    #expect((try? FileManager.default.destinationOfSymbolicLink(atPath: link.path)) != nil)
}
