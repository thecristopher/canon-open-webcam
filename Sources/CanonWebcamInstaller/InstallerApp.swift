import SetupKit
import SwiftUI

@main
struct InstallerApp: App {
    @NSApplicationDelegateAdaptor(SetupAppDelegate.self) private var delegate

    var body: some Scene {
        Window("Install Canon Webcam", id: "installer") {
            InstallerView()
        }
        .windowResizability(.contentSize)
    }
}
