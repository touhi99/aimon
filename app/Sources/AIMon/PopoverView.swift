import SwiftUI
import ServiceManagement
import Charts

private func limitColor(_ p: Double) -> Color { p >= 85 ? .red : p >= 60 ? .orange : .green }
private func kindColor(_ k: String?) -> Color {
    switch k { case "claude_sub": return .accentColor; case "ollama_sub": return .purple; case "api": return .orange; default: return .gray }
}
private func kindLabel(_ k: String?) -> String {
    switch k { case "claude_sub": return "Claude plan"; case "ollama_sub": return "Ollama"; case "api": return "API"; default: return "local" }
}

private struct ContentHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

struct PopoverView: View {
    @ObservedObject var store: Store
    @State private var loginOn = SMAppService.mainApp.status == .enabled
    @State private var hoverDay: Date?
    @State private var hoverHour: Date?
    @State private var hoverOllama: Date?
    @State private var tab = 0
    @State private var contentH: CGFloat = 400
    // leave room for the menu bar, the tab picker, the footer and padding
    private var maxScrollHeight: CGFloat { max(300, (NSScreen.main?.visibleFrame.height ?? 800) - 170) }
    @State private var openSession: String?
    @State private var openProc: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            if let s = store.snap {
                Picker("", selection: $tab) {
                    Text("Usage").tag(0); Text("Trace").tag(1); Text("Network").tag(2)
                }.pickerStyle(.segmented).labelsHidden()
                ScrollView(.vertical, showsIndicators: true) {
                    VStack(alignment: .leading, spacing: 20) {
                        switch tab {
                        case 1: traceTab()
                        case 2: networkTab()
                        default:
                            limits(s)
                            ollamaPlan(s)
                            api(s)
                            projects(s)
                        }
                    }
                    .padding(.trailing, 10)
                    .background(GeometryReader { g in Color.clear.preference(key: ContentHeightKey.self, value: g.size.height) })
                }
                .onPreferenceChange(ContentHeightKey.self) { contentH = $0 }
                .frame(height: min(max(contentH, 120), maxScrollHeight))
                footer(s)
            } else {
                Text(store.error ?? "Reading usage…").foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 120)
            }
        }
        .padding(18)
        .frame(width: 540)
    }

    func title(_ t: String, trailing: String? = nil) -> some View {
        HStack {
            Text(t.uppercased()).font(.system(size: 10, weight: .semibold)).tracking(1.0).foregroundStyle(.secondary)
            Spacer()
            if let trailing { Text(trailing).font(.system(size: 10)).foregroundStyle(.tertiary) }
        }
    }

    // MARK: Claude plan limits
    func limits(_ s: Snapshot) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            title(s.limits.source == "off" ? "Claude plan" : "Claude Pro", trailing: s.limits.source == "live" ? "live" : s.limits.source == "stale" ? "stale" : s.limits.source == "off" ? "off" : "no data")
            if s.limits.source == "off" {
                Text("Plan limits are off. Turning them on reads the Claude Code login from your Keychain and asks api.anthropic.com for your usage.").font(.system(size: 11)).foregroundStyle(.secondary)
                Button("Turn on plan limits") { store.enable("claude_limits") }.font(.system(size: 11))
            }
            if let a = s.limits.session { gauge("5-hour session", a) }
            if let w = s.limits.weekly { gauge("This week", w) }
            if s.limits.source != "off" && s.limits.session == nil && s.limits.weekly == nil {
                Text("Limits unavailable. Sign in to Claude Code.").font(.system(size: 12)).foregroundStyle(.secondary)
            }
        }
    }
    func gauge(_ name: String, _ l: Snapshot.Limit) -> some View {
        let p = l.pct ?? 0
        return VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline) {
                Text(name).font(.system(size: 12))
                Spacer()
                Text("\(Int(p.rounded()))%").font(.system(size: 22, weight: .semibold, design: .rounded)).monospacedDigit()
            }
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.primary.opacity(0.08))
                    Capsule().fill(LinearGradient(colors: [limitColor(p).opacity(0.7), limitColor(p)], startPoint: .leading, endPoint: .trailing))
                        .frame(width: max(8, g.size.width * min(p, 100) / 100))
                }
            }.frame(height: 8)
            Text(Fmt.resets(l.resets_at)).font(.system(size: 10)).foregroundStyle(.secondary)
        }
    }

    // MARK: Ollama cloud spend
    func ollamaPlan(_ s: Snapshot) -> some View {
        let o = s.ollama_limits
        let cal = Calendar.current
        let f = ISO8601DateFormatter()
        let start = o.first.flatMap { f.date(from: $0) }.map { cal.startOfDay(for: $0) }
        struct D { var day: Date; var cost: Double }
        let days: [D] = (o.daily ?? []).enumerated().compactMap { i, c in
            start.flatMap { cal.date(byAdding: .day, value: i, to: $0) }.map { D(day: $0, cost: c) }
        }
        let sel = hoverOllama.flatMap { h in days.first { cal.isDate($0.day, inSameDayAs: h) } }
        return VStack(alignment: .leading, spacing: 8) {
            title("Ollama cloud", trailing: o.source == "live" ? "spend · 30 days" : nil)
            if o.source == "live" {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    if let sel {
                        Text(Fmt.money(sel.cost)).font(.system(size: 28, weight: .semibold, design: .rounded)).monospacedDigit()
                        Text(sel.day.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day())).font(.system(size: 12)).foregroundStyle(.secondary)
                    } else {
                        Text(Fmt.money(o.today ?? 0)).font(.system(size: 28, weight: .semibold, design: .rounded)).monospacedDigit()
                        Text("today").font(.system(size: 12)).foregroundStyle(.secondary)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 1) {
                        Text("\(Fmt.money(o.week ?? 0)) · 7d").font(.system(size: 12, weight: .medium, design: .rounded)).monospacedDigit()
                        Text("\(Fmt.money(o.month ?? 0)) · 30d").font(.system(size: 12, weight: .medium, design: .rounded)).monospacedDigit()
                    }
                }
                Chart(days, id: \.day) { d in
                    BarMark(x: .value("d", d.day, unit: .day), y: .value("$", d.cost), width: .fixed(9))
                        .clipShape(RoundedRectangle(cornerRadius: 2))
                        .foregroundStyle(sel.map { cal.isDate($0.day, inSameDayAs: d.day) } == true ? Color.purple : Color.purple.opacity(0.4))
                }
                .chartXSelection(value: $hoverOllama)
                .chartYAxis(.hidden)
                .chartXAxis { AxisMarks(values: .stride(by: .day, count: 7)) { _ in AxisValueLabel(format: .dateTime.month(.abbreviated).day()).font(.system(size: 9)) } }
                .frame(height: 40)
                HStack {
                    Text("Subscription session and weekly % are only on ollama.com/dashboard.").font(.system(size: 10)).foregroundStyle(.tertiary)
                    Spacer()
                    Link("Open", destination: URL(string: "https://ollama.com/dashboard")!).font(.system(size: 10))
                }
            } else if o.source == "off" {
                Text("Off. Turning it on signs a request to ollama.com with your `ollama signin` key.").font(.system(size: 11)).foregroundStyle(.secondary)
                Button("Turn on Ollama spend") { store.enable("ollama") }.font(.system(size: 11))
            } else if o.source == "nokey" {
                Text("No Ollama sign-in key found. Run `ollama signin` to link this Mac.").font(.system(size: 11)).foregroundStyle(.secondary)
            } else {
                Text("Ollama usage unavailable right now.").font(.system(size: 12)).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: API money
    func api(_ s: Snapshot) -> some View {
        let a = s.api
        let cal = Calendar.current
        let today = cal.startOfDay(for: Date())
        let past = store.monthBack > 0
        struct D { var day: Date; var cost: Double; var hot: Bool }
        var days: [D] = []
        var total = a.month
        var apps = a.by_app
        var label = "this month"
        if past {
            let m = store.month
            let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"
            if let m, let start = f.date(from: m.first) {
                days = m.daily.enumerated().map { D(day: cal.date(byAdding: .day, value: $0.offset, to: start)!, cost: $0.element, hot: false) }
                total = m.total; apps = m.by_app; label = m.label
            } else { total = 0; apps = []; label = "…" }
        } else {
            days = a.daily.enumerated().map { D(day: cal.date(byAdding: .day, value: $0.offset - 13, to: today)!, cost: $0.element, hot: $0.offset == 13) }
        }
        let sel = hoverDay.flatMap { h in days.first { cal.isDate($0.day, inSameDayAs: h) } }
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Text("API SPEND").font(.system(size: 10, weight: .semibold)).tracking(1.0).foregroundStyle(.secondary)
                Spacer()
                Button { store.setMonth(store.monthBack + 1) } label: { Image(systemName: "chevron.left") }
                    .buttonStyle(.borderless).disabled(past && store.month?.has_prev == false)
                Button { store.setMonth(store.monthBack - 1) } label: { Image(systemName: "chevron.right") }
                    .buttonStyle(.borderless).disabled(!past)
            }
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                if let sel {
                    Text(Fmt.money(sel.cost)).font(.system(size: 34, weight: .semibold, design: .rounded)).monospacedDigit()
                    Text(sel.day.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day())).font(.system(size: 12)).foregroundStyle(.secondary)
                } else if past {
                    Text(Fmt.money(total)).font(.system(size: 34, weight: .semibold, design: .rounded)).monospacedDigit()
                    Text(label).font(.system(size: 12)).foregroundStyle(.secondary)
                } else {
                    Text(Fmt.money(a.today)).font(.system(size: 34, weight: .semibold, design: .rounded)).monospacedDigit()
                    Text("today").font(.system(size: 12)).foregroundStyle(.secondary)
                }
                Spacer()
                if !past {
                    VStack(alignment: .trailing, spacing: 1) {
                        Text(Fmt.money(total)).font(.system(size: 14, weight: .semibold, design: .rounded)).monospacedDigit()
                        Text(label).font(.system(size: 10)).foregroundStyle(.secondary)
                    }
                }
            }
            Chart(days, id: \.day) { d in
                BarMark(x: .value("d", d.day, unit: .day), y: .value("$", d.cost), width: .fixed(past ? 6 : 14))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .foregroundStyle(sel.map { cal.isDate($0.day, inSameDayAs: d.day) } == true || (sel == nil && d.hot) ? Color.orange : Color.orange.opacity(0.35))
            }
            .chartXSelection(value: $hoverDay)
            .chartYAxis(.hidden)
            .chartXAxis { AxisMarks(values: .stride(by: .day, count: 7)) { _ in AxisValueLabel(format: .dateTime.month(.abbreviated).day()).font(.system(size: 9)) } }
            .frame(height: 48)
            if !apps.isEmpty {
                HStack(spacing: 6) {
                    ForEach(apps) { app in
                        Text("\(app.name) \(Fmt.money(app.cost))")
                            .font(.system(size: 11)).padding(.horizontal, 8).padding(.vertical, 3)
                            .background(Color.orange.opacity(0.14), in: Capsule())
                    }
                }
            }
        }
    }

    // MARK: Activity (tokens, not money)
    func activity(_ s: Snapshot) -> some View {
        let now = Date()
        struct P: Identifiable { var id: Int; var t: Date; var v: Int; var kind: String }
        var pts: [P] = []
        for (i, h) in s.hourly.enumerated() {
            let t = now.addingTimeInterval(Double(i - 23) * 3600)
            pts.append(P(id: i * 3, t: t, v: h.claude, kind: "Claude"))
            pts.append(P(id: i * 3 + 1, t: t, v: h.ollama, kind: "Ollama"))
            pts.append(P(id: i * 3 + 2, t: t, v: h.other, kind: "API/local"))
        }
        let hsel = hoverHour.flatMap { h in pts.filter { abs($0.t.timeIntervalSince(h)) < 1800 } }
        let hlabel: String? = {
            guard let hsel, let t = hsel.first?.t else { return nil }
            let parts = hsel.filter { $0.v > 0 }.map { "\($0.kind) \(Fmt.tokens($0.v))" }
            return t.formatted(.dateTime.hour().minute()) + " · " + (parts.isEmpty ? "idle" : parts.joined(separator: " · "))
        }()
        return VStack(alignment: .leading, spacing: 8) {
            title("Activity · 24h", trailing: hlabel ?? "tokens")
            Chart(pts) { p in
                BarMark(x: .value("t", p.t, unit: .hour), y: .value("tokens", p.v), width: .fixed(9))
                    .foregroundStyle(by: .value("kind", p.kind))
            }
            .chartForegroundStyleScale(["Claude": Color.accentColor, "Ollama": Color.purple, "API/local": Color.orange])
            .chartLegend(.hidden)
            .chartXSelection(value: $hoverHour)
            .chartYAxis(.hidden)
            .chartXAxis { AxisMarks(values: .stride(by: .hour, count: 6)) { _ in AxisValueLabel(format: .dateTime.hour(.defaultDigits(amPM: .omitted))).font(.system(size: 9)) } }
            .frame(height: 56)
            HStack(spacing: 10) {
                chip("Claude today") { Text(Fmt.tokens(s.claude.today_tokens)).font(.system(size: 18, weight: .semibold, design: .rounded)) }
                chip("Cache hit") {
                    ZStack {
                        Circle().stroke(Color.primary.opacity(0.1), lineWidth: 4)
                        Circle().trim(from: 0, to: s.claude.cache_hit).stroke(Color.green, style: StrokeStyle(lineWidth: 4, lineCap: .round)).rotationEffect(.degrees(-90))
                        Text("\(Int(s.claude.cache_hit * 100))%").font(.system(size: 9, weight: .semibold)).monospacedDigit()
                    }.frame(width: 34, height: 34)
                }
                chip("Ollama today") { Text(Fmt.tokens(s.ollama.today_tokens)).font(.system(size: 18, weight: .semibold, design: .rounded)) }
            }
        }
    }
    func chip<C: View>(_ t: String, @ViewBuilder _ content: () -> C) -> some View {
        VStack(spacing: 5) {
            content().frame(height: 34)
            Text(t).font(.system(size: 10)).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity).padding(.vertical, 9)
        .background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    // MARK: Projects / models
    func projects(_ s: Snapshot) -> some View {
        let mx = Double(max(s.projects.map { $0.tokens }.max() ?? 1, 1))
        return VStack(alignment: .leading, spacing: 7) {
            title("Building · 7 days", trailing: "tokens · $ at API rates")
            ForEach(s.projects.prefix(5)) { p in
                HStack(spacing: 10) {
                    Text(p.name).font(.system(size: 12)).lineLimit(1).frame(width: 118, alignment: .leading)
                    GeometryReader { g in Capsule().fill(Color.accentColor.opacity(0.8)).frame(width: max(3, g.size.width * Double(p.tokens) / mx)) }.frame(height: 6)
                    Text(Fmt.tokens(p.tokens)).font(.system(size: 12, weight: .medium)).monospacedDigit().frame(width: 52, alignment: .trailing)
                    Text((p.cost ?? 0) >= 0.005 ? Fmt.money(p.cost ?? 0) : "").font(.system(size: 11)).foregroundStyle(.secondary).monospacedDigit().frame(width: 52, alignment: .trailing)
                }
            }
        }
    }
    func models(_ s: Snapshot) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            title("Models · 7 days")
            ForEach(s.models.prefix(5)) { m in
                HStack(spacing: 8) {
                    Circle().fill(kindColor(m.kind)).frame(width: 7, height: 7)
                    Text(m.name).font(.system(size: 12)).lineLimit(1)
                    Text(kindLabel(m.kind)).font(.system(size: 10)).foregroundStyle(.tertiary)
                    Spacer()
                    Text(Fmt.tokens(m.tokens)).font(.system(size: 12)).monospacedDigit()
                }
            }
        }
    }

    // MARK: Trace
    func traceTab() -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let t = store.trace {
                title("Tools · 24h", trailing: "\(t.tools_24h) calls · \(t.errors_24h) failed")
                if t.errors_24h > 0 {
                    Text(["exit": "non-zero exit", "blocked": "blocked / rejected", "error": "error"].sorted { $0.key < $1.key }.compactMap { k, label in
                        t.kinds_24h[k].map { "\($0) \(label)" } }.joined(separator: " · "))
                        .font(.system(size: 10)).foregroundStyle(.secondary)
                }
                HStack(spacing: 6) {
                    ForEach(t.top_24h.prefix(4)) { x in
                        Text("\(x.name) \(x.n)" + (x.err > 0 ? " · \(x.err)✕" : ""))
                            .font(.system(size: 11)).lineLimit(1).padding(.horizontal, 8).padding(.vertical, 3)
                            .background((x.err > 0 ? Color.red : Color.accentColor).opacity(0.14), in: Capsule())
                    }
                }
                if !t.recent_fails.isEmpty {
                    title("Recent failures")
                    ForEach(t.recent_fails.prefix(6)) { f in failRow(f, showWhen: true) }
                }
                title("Sessions · recent")
                if t.sessions.isEmpty { Text("No sessions in the last 7 days.").font(.system(size: 12)).foregroundStyle(.secondary) }
                ForEach(t.sessions.prefix(8)) { x in
                    VStack(alignment: .leading, spacing: 4) {
                        Button { openSession = openSession == x.id ? nil : x.id } label: {
                            HStack(spacing: 8) {
                                Circle().fill(x.errors > 0 ? Color.red : Color.green).frame(width: 7, height: 7)
                                Text(x.project).font(.system(size: 12, weight: .medium)).lineLimit(1)
                                Text(x.model.replacingOccurrences(of: "claude-", with: "")).font(.system(size: 10)).foregroundStyle(.tertiary).lineLimit(1)
                                Spacer()
                                Text(Fmt.tokens(x.tokens)).font(.system(size: 11)).monospacedDigit()
                                Text(Fmt.ago(x.end)).font(.system(size: 10)).foregroundStyle(.secondary).frame(width: 48, alignment: .trailing)
                            }.contentShape(Rectangle())
                        }.buttonStyle(.plain)
                        if openSession == x.id {
                            Text("\(Fmt.dur(x.start, x.end)) · \(x.calls) model calls · \(x.tools) tool calls" + (x.errors > 0 ? " · \(x.errors) failed" : ""))
                                .font(.system(size: 10)).foregroundStyle(.secondary)
                            ForEach(x.fails.prefix(5)) { f in failRow(f, showWhen: false) }
                            ForEach(x.top) { tl in
                                HStack {
                                    Text(tl.name).font(.system(size: 11)).lineLimit(1)
                                    Spacer()
                                    if tl.err > 0 { Text("\(tl.err) failed").font(.system(size: 10)).foregroundStyle(.red) }
                                    Text("\(tl.n)").font(.system(size: 11)).monospacedDigit()
                                }.padding(.leading, 15)
                            }
                        }
                    }
                }
            } else {
                Text("Reading sessions…").foregroundStyle(.secondary).frame(maxWidth: .infinity, minHeight: 80)
            }
        }
    }

    func failRow(_ f: TraceFail, showWhen: Bool) -> some View {
        let col: Color = f.kind == "blocked" ? .orange : f.kind == "exit" ? .secondary : .red
        return VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Text(f.kind == "exit" ? "exit" : f.kind).font(.system(size: 9, weight: .semibold)).foregroundStyle(col)
                    .padding(.horizontal, 5).padding(.vertical, 1).background(col.opacity(0.15), in: Capsule())
                Text(f.tool).font(.system(size: 11, weight: .medium)).lineLimit(1)
                if showWhen, let ts = f.ts { Text((f.project.map { $0 + " · " } ?? "") + Fmt.ago(ts)).font(.system(size: 10)).foregroundStyle(.tertiary).lineLimit(1) }
            }
            if !f.arg.isEmpty { Text(f.arg).font(.system(size: 10, design: .monospaced)).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle) }
            if !f.msg.isEmpty { Text(f.msg).font(.system(size: 10)).foregroundStyle(.tertiary).lineLimit(2) }
        }.padding(.leading, 15).help(f.arg + "\n" + f.msg)
    }

    // MARK: Network
    func networkTab() -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let n = store.net {
                title("Outbound · 24h", trailing: "bytes since AI Project Monitor started sampling")
                if n.procs.isEmpty { Text("Nothing sampled yet.").font(.system(size: 12)).foregroundStyle(.secondary) }
                ForEach(n.procs) { p in
                    VStack(alignment: .leading, spacing: 3) {
                        Button { openProc = openProc == p.name ? nil : p.name } label: {
                            HStack(spacing: 8) {
                                Circle().fill(p.ai ? Color.purple : Color.gray.opacity(0.5)).frame(width: 7, height: 7)
                                Text(p.name).font(.system(size: 12, weight: .medium)).lineLimit(1)
                                if p.conns > 0 { Text("\(p.conns) open").font(.system(size: 10)).foregroundStyle(.tertiary) }
                                if p.unknown > 0 { Text("\(p.unknown) unrecognised").font(.system(size: 10)).foregroundStyle(.orange) }
                                Spacer()
                                Text("↑\(Fmt.bytes(p.out))  ↓\(Fmt.bytes(p.in))").font(.system(size: 11)).monospacedDigit()
                            }.contentShape(Rectangle())
                        }.buttonStyle(.plain)
                        if openProc == p.name {
                            if p.detail.isEmpty { Text("No open connections right now.").font(.system(size: 10)).foregroundStyle(.secondary).padding(.leading, 15) }
                            ForEach(p.detail) { d in
                                HStack(spacing: 6) {
                                    Image(systemName: d.known ? "checkmark.circle.fill" : "questionmark.circle.fill")
                                        .font(.system(size: 10)).foregroundStyle(d.known ? Color.green : Color.orange)
                                    Text(d.host).font(.system(size: 10)).lineLimit(1).truncationMode(.middle)
                                    Spacer()
                                    Text(":\(d.port) · pid \(d.pid) · up \(d.up)").font(.system(size: 10)).foregroundStyle(.secondary).monospacedDigit()
                                }.padding(.leading, 15).help(d.ip)
                            }
                        } else if !p.hosts.isEmpty {
                            Text(p.hosts.prefix(3).joined(separator: " · ")).font(.system(size: 10)).foregroundStyle(.secondary)
                                .lineLimit(1).truncationMode(.middle).padding(.leading, 15)
                        }
                    }
                }
                Text("Purple = AI-related. ✓ = host belongs to a known provider or CDN (not a guarantee it is safe). ? = not recognised, often just a plain IP. Hosts are IP-level, not full domain logs.").font(.system(size: 10)).foregroundStyle(.tertiary)
            } else {
                Text("Sampling connections…").foregroundStyle(.secondary).frame(maxWidth: .infinity, minHeight: 80)
            }
        }
    }

    func footer(_ s: Snapshot) -> some View {
        HStack {
            Text("Money = API only · plans show % used").font(.system(size: 10)).foregroundStyle(.tertiary)
            Spacer()
            Button {
                do { if loginOn { try SMAppService.mainApp.unregister() } else { try SMAppService.mainApp.register() } } catch {}
                loginOn = SMAppService.mainApp.status == .enabled
            } label: { Label("Open at login", systemImage: loginOn ? "checkmark.square.fill" : "square").font(.system(size: 10)) }.buttonStyle(.borderless).foregroundStyle(.secondary)
            Button { store.refresh() } label: { Image(systemName: "arrow.clockwise") }.buttonStyle(.borderless)
            Button { NSApp.terminate(nil) } label: { Image(systemName: "power") }.buttonStyle(.borderless)
        }
    }
}
