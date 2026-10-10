import AppKit
import SwiftUI

/// The small panel under the menu bar icon: how the camera's doing, and a way into the full app.
public struct MenuBarPanel: View {
    @ObservedObject var model: CameraModel
    let openWindow: (String) -> Void

    public init(model: CameraModel, openWindow: @escaping (String) -> Void) {
        self.model = model
        self.openWindow = openWindow
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                AppBadge().scaleEffect(0.7).frame(width: 40, height: 40)
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.state.title).font(.headline)
                    Text(model.state.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                }
                Spacer()
            }
            .padding(14)

            VStack(spacing: 8) {
                Button { openWindow("main") } label: {
                    Text("Open Canon Webcam").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                Toggle(isOn: model.binding(\.mirror, name: "mirror", format: { $0 ? "on" : "off" })) {
                    Text("Mirror for everyone")
                }
                .toggleStyle(.switch)
                .controlSize(.small)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.horizontal, 14)
            .padding(.bottom, 12)

            Divider()
            HStack {
                Menu {
                    Button("About Canon Webcam") { Credits.showAboutPanel() }
                    Divider()
                    Button("Set Up…") { openWindow("setup") }
                    Button("Open Log") { SetupActions.openLog() }
                    Divider()
                    Button("Uninstall Canon Webcam…") { openWindow("uninstall") }
                    Divider()
                    Button("Quit Canon Webcam") { NSApplication.shared.terminate(nil) }
                } label: {
                    Image(systemName: "gearshape")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                Spacer()
                Text("Pick **OBS Virtual Camera** in your video app").font(.caption).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
        }
        .frame(width: 320)
    }
}
