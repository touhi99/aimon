import SwiftUI

private let failKinds: [(key: String, label: String, color: Color)] = [
    ("exit", "command exit", .hAmber),
    ("blocked", "blocked", .hOrange),
    ("error", "tool error", .hRose),
]
private let toolColors: [Color] = [emerald, Color(hex: 0x38bdf8), Color(hex: 0xc084fc), Color(hex: 0x2dd4bf), .hAmber, Color(hex: 0x52525b)]

private func count(_ d: TraceDay, _ key: String) -> Int { key == "exit" ? d.exit : key == "blocked" ? d.blocked : d.error }

struct ActivityPage: View {
    @ObservedObject var store: Store
    @State private var day: Int?
    @State private var open: String?

    var body: some View {
        if let t = store.trace {
            let days = t.daily ?? []
            let picked = day ?? days.last?.day ?? 0
            VStack(alignment: .leading, spacing: 16) {
                fails(days, picked: picked)
                tools(t)
                sessions(t, day: picked)
            }
        } else {
            Text("Reading sessions…").font(.system(size: 14)).foregroundStyle(Color.hMut).frame(maxWidth: .infinity, minHeight: 120)
        }
    }

    private func fails(_ days: [TraceDay], picked: Int) -> some View {
        let top = CGFloat(max(days.map(\.fails).max() ?? 1, 1))
        return VStack(alignment: .leading, spacing: 14) {
            Label13("Failed tool calls per day, 7 days", trailing: "click a day for its sessions")
            HStack(alignment: .bottom, spacing: 12) {
                ForEach(days) { d in
                    let on = d.day == picked
                    let date = Date(timeIntervalSince1970: TimeInterval(d.day))
                    Button { withAnimation(.easeOut(duration: 0.15)) { day = d.day; open = nil } } label: {
                        VStack(spacing: 6) {
                            Text("\(d.fails)").font(.system(size: 13, weight: .semibold)).monospacedDigit()
                                .foregroundStyle(on ? Color.hInk : Color.hMut)
                            VStack(spacing: 2) {
                                ForEach(failKinds.reversed(), id: \.key) { k in
                                    let v = count(d, k.key)
                                    if v > 0 {
                                        Rectangle().fill(k.color).frame(height: max(2, 150 * CGFloat(v) / top))
                                    }
                                }
                            }
                            .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                            .frame(height: 150, alignment: .bottom)
                            .opacity(on ? 1 : 0.6)
                            Text(date.formatted(.dateTime.weekday(.abbreviated))).font(.system(size: 12.5, weight: on ? .semibold : .regular))
                                .foregroundStyle(on ? Color.hInk : Color.hMut)
                        }
                        .padding(.vertical, 8).padding(.horizontal, 6)
                        .frame(maxWidth: .infinity)
                        .background(Color.white.opacity(on ? 0.05 : 0), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help("\(d.calls) tool calls, \(d.fails) failed")
                }
            }
            HStack(spacing: 16) {
                ForEach(failKinds, id: \.key) { k in
                    HStack(spacing: 6) {
                        RoundedRectangle(cornerRadius: 2).fill(k.color).frame(width: 9, height: 9)
                        Text(k.label).font(.system(size: 12)).foregroundStyle(Color.hMut)
                    }
                }
            }
        }
        .padding(20).housePanel()
    }

    private func tools(_ t: TraceData) -> some View {
        let top = Array(t.top_24h.prefix(5))
        let rest = t.tools_24h - top.reduce(0) { $0 + $1.n }
        let parts: [(String, Int, Int)] = top.map { ($0.name, $0.n, $0.err) } + (rest > 0 ? [("other", rest, 0)] : [])
        let total = max(t.tools_24h, 1)
        return VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                Text("Tool calls, 24 h").font(.system(size: 13)).foregroundStyle(Color.hMut)
                Spacer()
                if t.errors_24h > 0 { Pill(text: "\(t.errors_24h) failed", tint: .hRose) }
                Text(t.tools_24h.formatted()).font(.system(size: 22, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
            }
            GeometryReader { g in
                HStack(spacing: 3) {
                    ForEach(Array(parts.enumerated()), id: \.offset) { i, p in
                        RoundedRectangle(cornerRadius: 5, style: .continuous)
                            .fill(toolColors[min(i, toolColors.count - 1)])
                            .frame(width: max(4, (g.size.width - 3 * CGFloat(parts.count - 1)) * CGFloat(p.1) / CGFloat(total)))
                    }
                }
            }
            .frame(height: 22)
            HStack(spacing: 16) {
                ForEach(Array(parts.enumerated()), id: \.offset) { i, p in
                    HStack(spacing: 6) {
                        RoundedRectangle(cornerRadius: 2).fill(toolColors[min(i, toolColors.count - 1)]).frame(width: 9, height: 9)
                        Text(p.0).font(.system(size: 12.5)).foregroundStyle(Color.hSoft)
                        Text("\(p.1)").font(.system(size: 12.5, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
                        if p.2 > 0 { Text("\(p.2) failed").font(.system(size: 12)).foregroundStyle(Color.hRose) }
                    }
                    .lineLimit(1)
                }
            }
        }
        .padding(20).housePanel()
    }

    private func sessions(_ t: TraceData, day: Int) -> some View {
        let list = t.sessions.filter { $0.end >= day && $0.start < day + 86400 }
        let date = Date(timeIntervalSince1970: TimeInterval(day))
        return VStack(alignment: .leading, spacing: 10) {
            Label13("Sessions on " + date.formatted(.dateTime.weekday(.wide).day().month(.abbreviated)), trailing: "\(list.count)")
            if list.isEmpty {
                Text("No Claude Code sessions that day.").font(.system(size: 13)).foregroundStyle(Color.hMut).padding(.vertical, 8)
            }
            VStack(spacing: 2) {
                ForEach(list) { x in sessionRow(x) }
            }
        }
        .padding(20).housePanel()
    }

    private func sessionRow(_ x: TraceSession) -> some View {
        let on = open == x.id
        return VStack(alignment: .leading, spacing: 10) {
            Button { withAnimation(.easeOut(duration: 0.15)) { open = on ? nil : x.id } } label: {
                HStack(spacing: 12) {
                    Circle().fill(x.errors > 0 ? Color.hRose : emerald).frame(width: 8, height: 8)
                    Text(x.project).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Color.hInk).lineLimit(1).frame(width: 150, alignment: .leading)
                    Text(Date(timeIntervalSince1970: TimeInterval(x.start)).formatted(.dateTime.hour().minute())).font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hMut)
                    Text(Fmt.dur(x.start, x.end)).font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hMut)
                    Spacer()
                    if x.errors > 0 { Pill(text: "\(x.errors) failed", tint: .hRose) }
                    Text("\(x.tools) tools").font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hSoft).frame(width: 74, alignment: .trailing)
                    Text(Fmt.tokens(x.tokens)).font(.system(size: 12.5, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk).frame(width: 56, alignment: .trailing)
                    Image(systemName: on ? "chevron.down" : "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.hMut)
                }
                .frame(height: 34).padding(.horizontal, 8)
            }
            .buttonStyle(.plain)
            if on {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(spacing: 6) {
                        Pill(text: x.model.replacingOccurrences(of: "claude-", with: ""), tint: .hSoft)
                        ForEach(x.top.prefix(6)) { tl in Pill(text: "\(tl.name) \(tl.n)", tint: tl.err > 0 ? .hRose : .hSoft) }
                    }
                    ForEach(x.fails) { f in failRow(f) }
                }
                .padding(.leading, 28).padding(.trailing, 8).padding(.bottom, 10)
            }
        }
        .hoverRow(on)
    }

    private func failRow(_ f: TraceFail) -> some View {
        let k = failKinds.first { $0.key == f.kind } ?? failKinds[2]
        return HStack(alignment: .top, spacing: 10) {
            RoundedRectangle(cornerRadius: 1.5).fill(k.color).frame(width: 3)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(f.tool).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Color.hInk)
                    Text(k.label).font(.system(size: 12)).foregroundStyle(k.color)
                }
                if !f.arg.isEmpty {
                    Text(f.arg).font(.system(size: 12, design: .monospaced)).foregroundStyle(Color.hSoft).lineLimit(1).truncationMode(.middle)
                }
                if !f.msg.isEmpty { Text(f.msg).font(.system(size: 12)).foregroundStyle(Color.hMut).lineLimit(2) }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
        .help(f.arg + "\n" + f.msg)
    }
}
