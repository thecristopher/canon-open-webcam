import SwiftUI

/// A section title with a small action on the right, like Reset.
struct SectionHeader: View {
    let title: String
    let actionTitle: String
    let action: () -> Void

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            Button(actionTitle, action: action).buttonStyle(.link).font(.caption)
        }
    }
}

/// A labeled slider with its value on the right. Double click the label to put it back.
struct AdjustmentRow: View {
    let title: String
    let symbol: String
    @Binding var value: Double
    var range: ClosedRange<Double> = -1...1
    var resetValue: Double = 0
    var lowLabel: String?
    var highLabel: String?
    var display: ((Double) -> String)?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(title, systemImage: symbol)
                    .onTapGesture(count: 2) { value = resetValue }
                    .help("Double click to reset")
                Spacer()
                Text(shownValue)
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(value == resetValue ? .tertiary : .secondary)
            }
            Slider(value: $value, in: range)
                .labelsHidden()
                .controlSize(.small)
                .frame(maxWidth: .infinity)
            if let lowLabel, let highLabel {
                HStack {
                    Text(lowLabel)
                    Spacer()
                    Text(highLabel)
                }
                .font(.caption2)
                .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 2)
    }

    /// -1 to 1 reads as -100 to +100, 0 to 1 as 0 to 100, unless the row says otherwise.
    private var shownValue: String {
        if let display { return display(value) }
        let percent = Int((value * 100).rounded())
        return range.lowerBound < 0 && percent > 0 ? "+\(percent)" : "\(percent)"
    }
}

/// A camera setting with named values, shown as a menu. Says so plainly when the mode dial is in charge.
struct CameraPickerRow: View {
    let title: String
    let symbol: String
    let control: SettingControl
    let choose: (String) -> Void

    var body: some View {
        switch control {
        case .unavailable:
            LabeledContent { Text("–").foregroundStyle(.tertiary) } label: { Label(title, systemImage: symbol) }
        case .lockedByMode(let current):
            LabeledContent { Text("\(current) · set by the camera").foregroundStyle(.secondary) } label: { Label(title, systemImage: symbol) }
        case .choices(let choices, let current):
            Picker(selection: Binding(get: { current }, set: choose)) {
                ForEach(choices, id: \.self) { Text($0).tag($0) }
            } label: {
                Label(title, systemImage: symbol)
            }
        }
    }
}

/// A camera setting with an ordered range (exposure), dragged like a slider and sent when you let go.
struct CameraSliderRow: View {
    let title: String
    let symbol: String
    let control: SettingControl
    let choose: (String) -> Void
    @State private var draft: Double?

    var body: some View {
        switch control {
        case .unavailable:
            LabeledContent { Text("–").foregroundStyle(.tertiary) } label: { Label(title, systemImage: symbol) }
        case .lockedByMode(let current):
            LabeledContent { Text("\(current) · set by the camera").foregroundStyle(.secondary) } label: { Label(title, systemImage: symbol) }
        case .choices(let choices, let current):
            slider(choices, current)
        }
    }

    private func slider(_ choices: [String], _ current: String) -> some View {
        let currentIndex = Double(choices.firstIndex(of: current) ?? choices.count / 2)
        let shown = draft ?? currentIndex
        return VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(title, systemImage: symbol)
                Spacer()
                Text(choices[Int(shown)]).font(.callout.monospacedDigit()).foregroundStyle(.secondary)
            }
            Slider(
                value: Binding(get: { shown }, set: { draft = $0 }),
                in: 0...Double(max(choices.count - 1, 1)),
                step: 1,
                onEditingChanged: { editing in
                    guard !editing, let picked = draft else { return }
                    choose(choices[Int(picked)])
                    draft = nil
                }
            )
            .labelsHidden()
            .controlSize(.small)
            .frame(maxWidth: .infinity)
        }
        .padding(.vertical, 2)
    }
}

/// Near, auto and far, for when autofocus picks the bookshelf instead of your face.
struct FocusRow: View {
    @ObservedObject var model: CameraModel

    var body: some View {
        LabeledContent {
            HStack(spacing: 6) {
                Button { model.focus("near", steps: 2) } label: { Image(systemName: "chevron.left.2") }
                    .help("Focus closer")
                Button("Auto") { model.focus("auto") }
                Button { model.focus("far", steps: 2) } label: { Image(systemName: "chevron.right.2") }
                    .help("Focus farther")
            }
            .controlSize(.small)
        } label: {
            Label("Focus", systemImage: "scope")
        }
        .disabled(!model.state.isLive)
    }
}
