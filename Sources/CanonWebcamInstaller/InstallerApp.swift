import SetupKit
import SwiftUI

@main
struct InstallerApp: App {
    @NSApplicationDelegateAdaptor(QuitWhenClosedDelegate.self) private var delegate

    init() {
        if CommandLine.arguments.contains("--install") {
            Headless.run("Install") { try await SetupActions.install() }
        }
    }

    var body: some Scene {
        Window("Install Canon Webcam", id: "installer") {
            InstallerView()
        }
        .windowResizability(.contentSize)
    }
}
