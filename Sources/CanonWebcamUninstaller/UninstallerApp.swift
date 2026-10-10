import SetupKit
import SwiftUI

@main
struct UninstallerApp: App {
    @NSApplicationDelegateAdaptor(QuitWhenClosedDelegate.self) private var delegate

    init() {
        if CommandLine.arguments.contains("--uninstall") {
            let restoreCanon = CommandLine.arguments.contains("--restore-canon")
            Headless.run("Uninstall") { try await SetupActions.uninstall(restoreCanon: restoreCanon) }
        }
    }

    var body: some Scene {
        Window("Uninstall Canon Webcam", id: "uninstaller") {
            UninstallerView()
        }
        .windowResizability(.contentSize)
    }
}
