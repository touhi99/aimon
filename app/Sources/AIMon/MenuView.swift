import ServiceManagement
import SwiftUI

struct Attention: Identifiable {
    enum Level { case red, amber }
    var level: Level
    var text: String
    var detail: String
    var page: Int
    var project: String?
    var id: String { text }
}

extension Store {
    var attention: [Attention] {
        var out: [Attention] = []
        for p in projects?.projects ?? [] where !p.down.isEmpty {
            out.append(Attention(level: .red, text: "\(p.name): service stopped", detail: p.down.joined(separator: ", "), page: 3, project: p.name))
        }
        if let l = snap?.limits, l.source == "live" {
            for (name, lim) in [("5-hour session", l.session), ("weekly", l.weekly)] {
                if let pct = lim?.pct, pct >= 85 {
                    out.append(Attention(level: .red, text: "Claude plan \(name) at \(Int(pct.rounded()))%", detail: Fmt.resets(lim?.resets_at), page: 0))
                }
            }
        }
        for p in projects?.projects ?? [] {
            guard let s = p.security else { continue }
            if s.secrets > 0 {
                out.append(Attention(level: .red, text: "\(p.name): \(s.secrets) secret\(s.secrets == 1 ? "" : "s") in git history", detail: "security scan", page: 3, project: p.name))
            } else if s.advisories > 0 {
                out.append(Attention(level: .amber, text: "\(p.name): \(s.advisories) dependency advisor\(s.advisories == 1 ? "y" : "ies")", detail: "security scan", page: 3, project: p.name))
            }
        }
        return out
    }
}

struct MenuView: View {
    @ObservedObject var store: Store
    var openApp: (Int, String?) -> Void
    @State private var loginOn = SMAppService.mainApp.status == .enabled

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 16) {
                PlanRing(store: store, size: 74)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Claude plan").font(.system(size: 15, weight: .semibold)).foregroundStyle(Color.hInk)
                    Text(Fmt.resets(store.snap?.limits.weekly?.resets_at)).font(.system(size: 12.5)).foregroundStyle(Color.hMut)
                }
                Spacer()
            }
            if store.attention.isEmpty {
                Label("Nothing needs attention", systemImage: "checkmark.circle.fill")
                    .font(.system(size: 14, weight: .medium)).foregroundStyle(emerald)
                    .padding(.vertical, 4)
            }
            ForEach(store.attention) { a in AlertCard(a: a) { openApp(a.page, a.project) } }
            Button { openApp(6, nil) } label: {
                Text("Open AI Monitor").font(.system(size: 14, weight: .semibold)).foregroundStyle(Color(hex: 0x04130c))
                    .frame(maxWidth: .infinity, minHeight: 40)
                    .background(emerald, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .shadow(color: emerald.opacity(0.3), radius: 10, y: 4)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            HStack {
                Button {
                    do { if loginOn { try SMAppService.mainApp.unregister() } else { try SMAppService.mainApp.register() } } catch {}
                    loginOn = SMAppService.mainApp.status == .enabled
                } label: { Label("Open at login", systemImage: loginOn ? "checkmark.square.fill" : "square").font(.system(size: 12)) }
                    .buttonStyle(.borderless).foregroundStyle(Color.hMut)
                Spacer()
                Button { store.refresh() } label: { Image(systemName: "arrow.clockwise") }.buttonStyle(.borderless).foregroundStyle(Color.hMut).help("Refresh")
                Button { NSApp.terminate(nil) } label: { Image(systemName: "power") }.buttonStyle(.borderless).foregroundStyle(Color.hMut).help("Quit")
            }
        }
        .padding(18)
        .frame(width: 380)
        .background(Color(hex: 0x141416))
        .environment(\.colorScheme, .dark)
    }
}
