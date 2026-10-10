import AppKit
import SwiftUI

/// Who made this, and a polite note that Canon had nothing to do with it.
public enum Credits {
    public static let author = "Cristopher Cervantes"
    public static let madeBy = "Made by \(author)"
    public static let disclaimer = "An independent project, not affiliated with or endorsed by Canon Inc. Canon and EOS are trademarks of Canon Inc."

    /// The standard About window, with the credit and the disclaimer in it.
    @MainActor
    public static func showAboutPanel() {
        let credits = NSAttributedString(
            string: "\(madeBy)\n\n\(disclaimer)",
            attributes: [
                .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
                .foregroundColor: NSColor.secondaryLabelColor,
            ]
        )
        NSApplication.shared.orderFrontStandardAboutPanel(options: [.credits: credits])
        NSApplication.shared.activate()
    }
}

/// The two quiet lines at the bottom of a window.
struct CreditFooter: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(Credits.madeBy).font(.caption.weight(.medium)).foregroundStyle(.secondary)
            Text(Credits.disclaimer).font(.caption2).foregroundStyle(.tertiary).fixedSize(horizontal: false, vertical: true)
        }
    }
}
