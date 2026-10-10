import Testing
@testable import SetupKit

@Test func theLoginItemOpensTheAppQuietlyInTheMenuBar() {
    let plist = AppLoginItem.plist(appExecutable: "/Applications/Canon Webcam.app/Contents/MacOS/CanonWebcam")
    #expect(plist["ProgramArguments"] as? [String] == ["/Applications/Canon Webcam.app/Contents/MacOS/CanonWebcam", "--background"])
}

@Test func theLoginItemRunsOncePerLogin() {
    let plist = AppLoginItem.plist(appExecutable: "/Applications/Canon Webcam.app/Contents/MacOS/CanonWebcam")
    #expect(plist["RunAtLoad"] as? Bool == true)
    #expect(plist["KeepAlive"] == nil)
}

@Test func theLoginItemOnlyRunsInARealLoginSession() {
    let plist = AppLoginItem.plist(appExecutable: "/tmp/Ada")
    #expect(plist["LimitLoadToSessionType"] as? String == "Aqua")
}
