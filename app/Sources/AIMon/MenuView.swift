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
            out.append(Attention(level: .red, text: "\(p.name): service stopped", detail: p.down.joined(separator: ", "), page: 5, project: p.down.first))
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
                out.append(Attention(level: .amber, text: "\(p.name): \(s.advisories) dependency advisor\(s.advisories == 1 ? "y" : "ies")", detail: s.items.map { $0.count == 1 ? $0[0].short : $0.map(\.package).joined(separator: ", ") } ?? "security scan", page: 3, project: p.name))
            }
        }
        return out
    }
}

struct TodayProject: Identifiable {
    var name: String; var secs: Int; var tokens: Int
    var id: String { name }
}

extension Store {
    var todayProjects: [TodayProject] {
        let start = Int(Calendar.current.startOfDay(for: Date()).timeIntervalSince1970)
        var secs: [String: Int] = [:]
        for t in trace?.sessions ?? [] where t.end >= start {
            secs[t.project, default: 0] += t.end - max(t.start, start)
        }
        var tokens: [String: Int] = [:]
        for p in snap?.projects ?? [] { tokens[p.name] = p.daily?.last ?? 0 }
        return Set(secs.keys).union(tokens.keys)
            .map { TodayProject(name: $0, secs: secs[$0] ?? 0, tokens: tokens[$0] ?? 0) }
            .filter { $0.tokens > 0 || $0.secs > 0 }
            .sorted { ($0.tokens, $0.secs) > ($1.tokens, $1.secs) }
            .prefix(3).map { $0 }
    }
}

private struct TodayStat: View {
    var label: String
    var value: String
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.system(size: 11)).foregroundStyle(Color.hMut)
            Text(value).font(.system(size: 14, weight: .semibold).monospacedDigit()).foregroundStyle(Color.hInk)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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
            if store.snap?.limits.source == "live" {
                HStack(spacing: 8) {
                    TodayStat(label: "5-hour session", value: store.snap?.limits.session?.pct.map { "\(Int($0.rounded()))%" } ?? "–")
                    TodayStat(label: "API today", value: Fmt.money(store.snap?.api.today ?? 0))
                }
            }
            if !store.todayProjects.isEmpty {
                VStack(spacing: 7) {
                    HStack {
                        Text("Claude Code today").font(.system(size: 11)).foregroundStyle(Color.hMut)
                        Spacer()
                        Text("time").font(.system(size: 11)).foregroundStyle(Color.hMut).frame(width: 64, alignment: .trailing)
                        Text("tokens").font(.system(size: 11)).foregroundStyle(Color.hMut).frame(width: 52, alignment: .trailing)
                    }
                    ForEach(store.todayProjects) { p in
                        HStack {
                            Text(p.name).font(.system(size: 13, weight: .medium)).foregroundStyle(Color.hInk).lineLimit(1)
                            Spacer()
                            Text(p.secs > 0 ? Fmt.duration(p.secs) : "–").font(.system(size: 12.5).monospacedDigit()).foregroundStyle(Color.hMut)
                                .frame(width: 64, alignment: .trailing)
                            Text(Fmt.tokens(p.tokens)).font(.system(size: 12.5).monospacedDigit()).foregroundStyle(Color.hMut)
                                .frame(width: 52, alignment: .trailing)
                        }
                    }
                }
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
