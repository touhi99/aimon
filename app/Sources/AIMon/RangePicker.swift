import SwiftUI

struct RangePicker: View {
    let options: [(days: Int, label: String)]
    let value: Int
    let pick: (Int) -> Void

    var body: some View {
        Picker("", selection: Binding(get: { value }, set: pick)) {
            ForEach(options, id: \.days) { Text($0.label).tag($0.days) }
        }
        .pickerStyle(.segmented).labelsHidden().fixedSize()
    }
}

struct PanelHeader: View {
    let title: String
    var hint: String?
    let options: [(days: Int, label: String)]
    let value: Int
    let pick: (Int) -> Void

    var body: some View {
        HStack(spacing: 12) {
            Text(title).font(.system(size: 13)).foregroundStyle(Color.hMut)
            Spacer()
            if let hint { Text(hint).font(.system(size: 12)).foregroundStyle(Color.hMut) }
            RangePicker(options: options, value: value, pick: pick)
        }
    }
}
