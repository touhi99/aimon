import Charts
import SwiftUI

private let modelColors: [Color] = [emerald, Color(hex: 0x38bdf8), Color(hex: 0xc084fc), .hAmber, .hOrange, Color(hex: 0x2dd4bf), .hRose]

private struct Day: Identifiable {
    var day: Date
    var value: Double
    var id: Date { day }
}

private func last14(_ values: [Double]) -> [Day] {
    let today = Calendar.current.startOfDay(for: Date())
    return values.enumerated().map { Day(day: Calendar.current.date(byAdding: .day, value: $0.offset - (values.count - 1), to: today)!, value: $0.element) }
}

private func shortModel(_ m: String) -> String {
    m.replacingOccurrences(of: "claude-", with: "")
        .replacingOccurrences(of: #"(\d)-(\d)"#, with: "$1.$2", options: .regularExpression)
        .replacingOccurrences(of: "-", with: " ").capitalized
}

struct SpendPage: View {
    @ObservedObject var store: Store
    @State private var hoverTok: Date?
    @State private var hoverApi: Date?
    @State private var allProjects = false
    @State private var openProject: String?

    var body: some View {
        if let s = store.snap {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top, spacing: 16) {
                    plan(s).frame(maxWidth: .infinity)
                    api(s).frame(maxWidth: .infinity)
                }
                .fixedSize(horizontal: false, vertical: true)
                tokens(s)
                models(s)
                projects(s)
                if ["live", "off"].contains(s.ollama_limits.source) { ollama(s) }
            }
        } else {
            Text(store.error ?? "Reading usage…").font(.system(size: 14)).foregroundStyle(Color.hMut).frame(maxWidth: .infinity, minHeight: 120)
        }
    }

    private func panel<C: View>(@ViewBuilder _ c: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 14, content: c)
            .padding(20).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading).housePanel()
    }

    private func plan(_ s: Snapshot) -> some View {
        panel {
            Label13("Claude plan", trailing: s.limits.source == "stale" ? "stale" : nil)
            if s.limits.source == "off" {
                Text("Plan limits are off. Turning them on reads the Claude Code login from your Keychain and asks api.anthropic.com for your usage.")
                    .font(.system(size: 13)).foregroundStyle(Color.hSoft).fixedSize(horizontal: false, vertical: true)
                Button("Turn on plan limits") { store.enable("claude_limits") }
            } else if s.limits.session == nil && s.limits.weekly == nil {
                Text("Limits unavailable. Sign in to Claude Code.").font(.system(size: 13)).foregroundStyle(Color.hSoft)
            } else {
                if let a = s.limits.session { gauge("5-hour session", a) }
                if let w = s.limits.weekly { gauge("This week", w) }
            }
        }
    }

    private func gauge(_ name: String, _ l: Snapshot.Limit) -> some View {
        let p = l.pct ?? 0
        let tint: [Color] = p >= 85 ? [.hOrange, .hRose] : [emerald, emerald.opacity(0.8)]
        return VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                Text(name).font(.system(size: 13.5)).foregroundStyle(Color.hSoft)
                Spacer()
                Text("\(Int(p.rounded()))%").font(.system(size: 24, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
            }
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.06))
                    Capsule().fill(LinearGradient(colors: tint, startPoint: .leading, endPoint: .trailing))
                        .frame(width: max(8, g.size.width * min(p, 100) / 100))
                        .shadow(color: tint[0].opacity(0.4), radius: 4)
                }
            }
            .frame(height: 9)
            Text(Fmt.resets(l.resets_at)).font(.system(size: 12)).foregroundStyle(Color.hMut)
        }
    }

    private func api(_ s: Snapshot) -> some View {
        let cal = Calendar.current
        let past = store.monthBack > 0
        var days: [Day] = last14(s.api.daily)
        var total = s.api.month
        var apps = s.api.by_app
        var label = "this month"
        if past {
            let f = DateFormatter()
            f.dateFormat = "yyyy-MM-dd"
            if let m = store.month, let start = f.date(from: m.first) {
                days = m.daily.enumerated().map { Day(day: cal.date(byAdding: .day, value: $0.offset, to: start)!, value: $0.element) }
                total = m.total
                apps = m.by_app
                label = m.label
            } else { total = 0; apps = []; label = "…" }
        }
        let sel = hoverApi.flatMap { h in days.first { cal.isDate($0.day, inSameDayAs: h) } }
        return panel {
            HStack(spacing: 8) {
                Text("API spend").font(.system(size: 13)).foregroundStyle(Color.hMut)
                Spacer()
                Button { store.setMonth(store.monthBack + 1) } label: { Image(systemName: "chevron.left") }
                    .buttonStyle(.borderless).disabled(past && store.month?.has_prev == false)
                Button { store.setMonth(store.monthBack - 1) } label: { Image(systemName: "chevron.right") }
                    .buttonStyle(.borderless).disabled(!past)
            }
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(Fmt.money(sel?.value ?? total)).font(.system(size: 30, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
                Text(sel.map { $0.day.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day()) } ?? label)
                    .font(.system(size: 13)).foregroundStyle(Color.hMut)
            }
            Chart(days) { d in
                BarMark(x: .value("d", d.day, unit: .day), y: .value("$", d.value))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .foregroundStyle(sel.map { cal.isDate($0.day, inSameDayAs: d.day) } ?? true ? Color.hAmber : Color.hAmber.opacity(0.35))
            }
            .chartXSelection(value: $hoverApi)
            .chartYAxis(.hidden)
            .chartXAxis { AxisMarks(values: .stride(by: .day, count: 7)) { _ in AxisValueLabel(format: .dateTime.month(.abbreviated).day()).foregroundStyle(Color.hMut) } }
            .frame(height: 70)
            if !apps.isEmpty {
                HStack(spacing: 6) {
                    ForEach(apps.prefix(4)) { a in Pill(text: "\(a.name) \(Fmt.money(a.cost))", tint: .hAmber) }
                }
            }
        }
    }

    private func tokens(_ s: Snapshot) -> some View {
        let days = last14(s.daily_tokens.map(Double.init))
        let cal = Calendar.current
        let sel = hoverTok.flatMap { h in days.first { cal.isDate($0.day, inSameDayAs: h) } }
        let week = s.daily_tokens.suffix(7).reduce(0, +)
        return panel {
            Label13("Tokens per day, 14 days")
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(Fmt.tokens(Int(sel?.value ?? Double(week)))).font(.system(size: 30, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
                Text(sel.map { $0.day.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day()) } ?? "last 7 days")
                    .font(.system(size: 13)).foregroundStyle(Color.hMut)
                Spacer()
                Text("\(Int((s.claude.cache_hit * 100).rounded()))% from cache today").font(.system(size: 12.5)).foregroundStyle(Color.hMut)
            }
            Chart(days) { d in
                AreaMark(x: .value("d", d.day, unit: .day), y: .value("t", d.value))
                    .interpolationMethod(.monotone)
                    .foregroundStyle(LinearGradient(colors: [emerald.opacity(0.45), emerald.opacity(0.02)], startPoint: .top, endPoint: .bottom))
                LineMark(x: .value("d", d.day, unit: .day), y: .value("t", d.value))
                    .interpolationMethod(.monotone)
                    .foregroundStyle(emerald).lineStyle(StrokeStyle(lineWidth: 2.2, lineCap: .round))
                if let sel, cal.isDate(sel.day, inSameDayAs: d.day) {
                    PointMark(x: .value("d", d.day, unit: .day), y: .value("t", d.value)).foregroundStyle(emerald).symbolSize(70)
                }
            }
            .chartXSelection(value: $hoverTok)
            .chartYAxis(.hidden)
            .chartXAxis { AxisMarks(values: .stride(by: .day, count: 2)) { _ in AxisValueLabel(format: .dateTime.weekday(.abbreviated)).foregroundStyle(Color.hMut) } }
            .frame(height: 150)
        }
    }

    private func models(_ s: Snapshot) -> some View {
        var merged: [(String, Int)] = []
        for m in s.models {
            let name = shortModel(m.name) + (m.kind == "api" ? " API" : "")
            if let i = merged.firstIndex(where: { $0.0 == name }) { merged[i].1 += m.tokens } else { merged.append((name, m.tokens)) }
        }
        merged.sort { $0.1 > $1.1 }
        let total = max(merged.reduce(0) { $0 + $1.1 }, 1)
        let big = merged.filter { $0.1 * 100 >= total }.prefix(6)
        let rest = total - big.reduce(0) { $0 + $1.1 }
        let shown = Array(big) + (rest * 100 >= total ? [("Other", rest)] : [])
        return panel {
            Label13("By model, 7 days")
            GeometryReader { g in
                HStack(spacing: 3) {
                    ForEach(Array(shown.enumerated()), id: \.offset) { i, m in
                        RoundedRectangle(cornerRadius: 5, style: .continuous)
                            .fill(modelColors[i % modelColors.count])
                            .frame(width: max(4, (g.size.width - 3 * CGFloat(shown.count - 1)) * CGFloat(m.1) / CGFloat(total)))
                            .help("\(m.0): \(Fmt.tokens(m.1))")
                    }
                }
            }
            .frame(height: 22)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 190), spacing: 10, alignment: .leading)], alignment: .leading, spacing: 8) {
                ForEach(Array(shown.enumerated()), id: \.offset) { i, m in
                    HStack(spacing: 8) {
                        RoundedRectangle(cornerRadius: 3).fill(modelColors[i % modelColors.count]).frame(width: 10, height: 10)
                        Text(m.0).font(.system(size: 13)).foregroundStyle(Color.hSoft).lineLimit(1)
                        Text("\(m.1 * 100 / total)%").font(.system(size: 13, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
                    }
                }
            }
        }
    }

    private func projects(_ s: Snapshot) -> some View {
        let top = Double(max(s.projects.first?.tokens ?? 1, 1))
        let shown = allProjects ? s.projects : Array(s.projects.prefix(6))
        return panel {
            Label13("Projects, 7 days", trailing: "tokens in and out · $ at API rates")
            VStack(spacing: 2) {
                ForEach(shown) { p in
                    let on = openProject == p.name
                    VStack(alignment: .leading, spacing: 10) {
                        Button { withAnimation(.easeOut(duration: 0.15)) { openProject = on ? nil : p.name } } label: {
                            HStack(spacing: 12) {
                                Text(p.name).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Color.hInk).lineLimit(1).frame(width: 150, alignment: .leading)
                                GeometryReader { g in
                                    Capsule().fill(LinearGradient(colors: [emerald.opacity(0.55), emerald], startPoint: .leading, endPoint: .trailing))
                                        .frame(width: max(4, g.size.width * Double(p.tokens) / top), height: 10)
                                        .frame(maxHeight: .infinity)
                                }
                                Text(Fmt.tokens(p.tokens)).font(.system(size: 13, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk).frame(width: 58, alignment: .trailing)
                                Text((p.cost ?? 0) >= 0.005 ? Fmt.money(p.cost ?? 0) : "").font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hMut).frame(width: 62, alignment: .trailing)
                                Image(systemName: on ? "chevron.down" : "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.hMut)
                            }
                            .frame(height: 30)
                            .padding(.horizontal, 8)
                        }
                        .buttonStyle(.plain)
                        if on, let d = p.daily { series(d).padding(.horizontal, 8).padding(.bottom, 8) }
                    }
                    .hoverRow(on)
                }
            }
            if s.projects.count > 6 {
                Button { withAnimation(.easeOut(duration: 0.15)) { allProjects.toggle() } } label: {
                    Text(allProjects ? "Show top 6" : "\(s.projects.count - 6) more projects · show all")
                        .font(.system(size: 13, weight: .medium)).foregroundStyle(emerald)
                        .frame(maxWidth: .infinity, minHeight: 32).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func series(_ daily: [Int]) -> some View {
        Chart(last14(daily.map(Double.init))) { d in
            BarMark(x: .value("d", d.day, unit: .day), y: .value("t", d.value))
                .clipShape(RoundedRectangle(cornerRadius: 3))
                .foregroundStyle(emerald.opacity(0.8))
        }
        .chartYAxis { AxisMarks(position: .trailing, values: .automatic(desiredCount: 2)) { v in
            AxisValueLabel { if let n = v.as(Double.self) { Text(Fmt.tokens(Int(n))) } }.foregroundStyle(Color.hMut)
        } }
        .chartXAxis { AxisMarks(values: .stride(by: .day, count: 2)) { _ in AxisValueLabel(format: .dateTime.weekday(.abbreviated)).foregroundStyle(Color.hMut) } }
        .frame(height: 90)
    }

    private func ollama(_ s: Snapshot) -> some View {
        let o = s.ollama_limits
        return panel {
            HStack {
                Label13("Ollama cloud")
                Link("ollama.com", destination: URL(string: "https://ollama.com/dashboard")!).font(.system(size: 12))
            }
            if o.source == "off" {
                Button("Turn on Ollama spend") { store.enable("ollama") }
            } else {
            HStack(spacing: 20) {
                ForEach([("today", o.today), ("7 days", o.week), ("30 days", o.month)], id: \.0) { l, v in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(Fmt.money(v ?? 0)).font(.system(size: 22, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
                        Text(l).font(.system(size: 12)).foregroundStyle(Color.hMut)
                    }
                }
                Spacer()
            }
            }
        }
    }
}
