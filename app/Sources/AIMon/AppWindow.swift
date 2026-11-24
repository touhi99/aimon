import SwiftUI

final class Nav: ObservableObject {
    @Published var page = Int(ProcessInfo.processInfo.environment["AIMON_TAB"] ?? "") ?? 6
    @Published var project = ProcessInfo.processInfo.environment["AIMON_OPEN"]
}

private struct PageEntry: Identifiable {
    let id: Int
    let label: String
    let icon: String
}

struct AppWindowView: View {
    @ObservedObject var store: Store
    @ObservedObject var nav: Nav

    private let entries = [
        PageEntry(id: 6, label: "Overview", icon: "square.grid.2x2"),
        PageEntry(id: 3, label: "Projects", icon: "folder"),
        PageEntry(id: 0, label: "Spend", icon: "dollarsign.circle"),
        PageEntry(id: 1, label: "Activity", icon: "waveform.path.ecg"),
        PageEntry(id: 2, label: "Network", icon: "network"),
        PageEntry(id: 5, label: "Logs", icon: "doc.text.magnifyingglass"),
    ]

    var body: some View {
        HStack(spacing: 0) {
            sidebar
            ZStack(alignment: .topLeading) {
                Color.hBg
                Glow()
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        Text(entries.first { $0.id == nav.page }?.label ?? "Overview")
                            .font(.system(size: 28, weight: .semibold)).tracking(-0.4).foregroundStyle(Color.hInk)
                        switch nav.page {
                        case 0: SpendPage(store: store)
                        case 1: ActivityPage(store: store)
                        case 2: NetworkPage(store: store)
                        case 3: ProjectsPage(store: store, nav: nav)
                        case 5: LogsPage(store: store, nav: nav)
                        default: OverviewView(store: store, nav: nav)
                        }
                    }
                    .padding(.horizontal, 32).padding(.top, 28).padding(.bottom, 40)
                    .frame(maxWidth: 1000, alignment: .leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Color.hLine, lineWidth: 1))
            .padding([.top, .bottom, .trailing], 8)
        }
        .background(Color.hSide)
        .environment(\.colorScheme, .dark)
        .frame(minWidth: 860, minHeight: 600)
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 10) {
                AppMark(size: 30).shadow(color: emerald.opacity(0.35), radius: 7)
                Text("AI Monitor").font(.system(size: 15, weight: .semibold)).foregroundStyle(Color.hInk)
            }
            .padding(.horizontal, 8).padding(.top, 28).padding(.bottom, 14)
            ForEach(entries) { e in
                let on = nav.page == e.id
                let badge = e.id == 6 ? store.attention.count : 0
                Button { nav.page = e.id } label: {
                    HStack(spacing: 10) {
                        Image(systemName: e.icon).font(.system(size: 13)).frame(width: 18)
                        Text(e.label).font(.system(size: 13.5, weight: on ? .medium : .regular))
                        Spacer(minLength: 0)
                        if badge > 0 {
                            Text("\(badge)").font(.system(size: 11, weight: .semibold)).monospacedDigit()
                                .padding(.horizontal, 7).padding(.vertical, 1)
                                .background(Color.hOrange.opacity(0.15), in: Capsule()).foregroundStyle(Color.hOrange)
                        }
                    }
                    .foregroundStyle(on ? Color.hInk : Color.hSoft)
                    .padding(.horizontal, 10)
                    .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
                    .background(on ? Color.white.opacity(0.07) : Color.clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            Spacer()
            Button { store.refresh() } label: {
                Label("Refresh", systemImage: "arrow.clockwise").font(.system(size: 12)).foregroundStyle(Color.hMut)
                    .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading).contentShape(Rectangle())
            }
            .buttonStyle(.plain).padding(.horizontal, 10)
        }
        .padding(.horizontal, 10).padding(.bottom, 12)
        .frame(width: 210)
    }
}
