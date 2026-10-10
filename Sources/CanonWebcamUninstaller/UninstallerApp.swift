import SetupKit
import SwiftUI

@main
struct UninstallerApp: App {
    @NSApplicationDelegateAdaptor(SetupAppDelegate.self) private var delegate

    var body: some Scene {
        Window("Uninstall Canon Webcam", id: "uninstaller") {
            UninstallerView()
        }
        .windowResizability(.contentSize)
    }
}
