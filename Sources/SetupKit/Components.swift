import AppKit
import SwiftUI

/// The app's little camera badge, so both windows feel like the same product.
struct AppBadge: View {
    var tint: Color = .blue

    var body: some View {
        RoundedRectangle(cornerRadius: 14, style: .continuous)
            .fill(LinearGradient(colors: [tint.opacity(0.85), tint], startPoint: .top, endPoint: .bottom))
            .frame(width: 56, height: 56)
            .overlay(Image(systemName: "web.camera.fill").font(.system(size: 26, weight: .semibold)).foregroundStyle(.white))
            .shadow(color: tint.opacity(0.3), radius: 6, y: 3)
    }
}

struct Header: View {
    let title: String
    let subtitle: String
    var tint: Color = .blue

    var body: some View {
        HStack(spacing: 14) {
            AppBadge(tint: tint)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.title2.weight(.semibold))
                Text(subtitle).foregroundStyle(.secondary)
            }
            Spacer()
        }
    }
}

/// A soft rounded panel that groups related content.
struct Card<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) { content }
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(nsColor: .controlBackgroundColor)))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color(nsColor: .separatorColor)))
    }
}

/// One line of the installer checklist: an icon that says where it stands, words, and maybe a button.
struct StepRow: View {
    let number: Int
    let title: String
    let status: StepStatus
    var action: () -> Void = {}
    var secondaryTitle: String?
    var secondaryAction: () -> Void = {}

    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            icon.frame(width: 26, height: 26)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.headline)
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            buttons
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .animation(.easeInOut(duration: 0.2), value: status)
    }

    private var detail: String {
        switch status {
        case .done(let text), .working(let text), .waitingOn(let text): return text
        case .needsAction(_, let text): return text
        }
    }

    @ViewBuilder private var icon: some View {
        switch status {
        case .done:
            Image(systemName: "checkmark.circle.fill").font(.system(size: 22)).foregroundStyle(.green)
        case .working:
            ProgressView().controlSize(.small)
        case .needsAction:
            Text("\(number)")
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: 22, height: 22)
                .background(Circle().fill(Color.accentColor))
        case .waitingOn:
            Image(systemName: "circle.dashed").font(.system(size: 22)).foregroundStyle(.tertiary)
        }
    }

    @ViewBuilder private var buttons: some View {
        HStack(spacing: 8) {
            if let secondaryTitle {
                Button(secondaryTitle, action: secondaryAction).buttonStyle(.link).font(.callout)
            }
            if case .needsAction(let button, _) = status {
                Button(button, action: action).buttonStyle(.borderedProminent).controlSize(.large)
            }
        }
    }
}

/// For the standalone installer and uninstaller: one window, one job, gone when it closes.
public final class QuitWhenClosedDelegate: NSObject, NSApplicationDelegate {
    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}
