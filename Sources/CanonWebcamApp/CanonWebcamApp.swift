import AppKit
import SetupKit
import SwiftUI

@main
struct CanonWebcamApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var camera = CameraModel()

    var body: some Scene {
        Window("Canon Webcam", id: "main") {
            CameraWindow(model: camera)
        }
        .defaultSize(width: 1180, height: 700)
        .defaultLaunchBehavior(.suppressed)

        MenuBarExtra {
            MenuBarPanelHost(model: camera)
        } label: {
            MenuBarIcon(model: camera)
        }
        .menuBarExtraStyle(.window)

        Window("Set Up Canon Webcam", id: "setup") {
            InstallerView()
        }
        .windowResizability(.contentSize)
        .defaultLaunchBehavior(.suppressed)

        Window("Uninstall Canon Webcam", id: "uninstall") {
            UninstallerView()
        }
        .windowResizability(.contentSize)
        .defaultLaunchBehavior(.suppressed)
    }
}

/// Lives on in the menu bar after its windows close, and brings the main window back when reopened from Finder.
final class AppDelegate: NSObject, NSApplicationDelegate {
    @MainActor static var openMainWindow: (() -> Void)?

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        if !hasVisibleWindows { Task { @MainActor in AppDelegate.openMainWindow?() } }
        return true
    }
}

/// Hands the menu bar panel a way to open windows, and brings them to the front since we're a menu bar app.
struct MenuBarPanelHost: View {
    @ObservedObject var model: CameraModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        MenuBarPanel(model: model) { id in
            openWindow(id: id)
            NSApplication.shared.activate()
        }
    }
}

/// The camera in the menu bar. It also runs everything the app does at launch.
struct MenuBarIcon: View {
    @ObservedObject var model: CameraModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Image(systemName: model.state.menuBarSymbol)
            .task { await launch() }
    }

    private func launch() async {
        model.startPolling()
        AppDelegate.openMainWindow = { show("main") }
        await SetupActions.syncInstalledEngine()
        if await Task.detached(operation: { SetupActions.needsSetup() }).value {
            show("setup")
        } else if !CommandLine.arguments.contains(AppLoginItem.backgroundFlag) {
            // Opened by hand, so show the window. At login we stay quietly in the menu bar.
            show("main")
        }
    }

    private func show(_ id: String) {
        openWindow(id: id)
        NSApplication.shared.activate()
    }
}
