import SwiftUI

private struct Item: Identifiable {
    var name: String
    var state: String
    var pace: PaceProject?
    var row: ProjRow?
    var use: UseRow?
    var id: String { name }
}

private func securityAge(_ s: Sec) -> Int? {
    guard let c = s.checked, let date = ISO8601DateFormatter().date(from: c) else { return nil }
    return Calendar.current.dateComponents([.day], from: date, to: Date()).day
}

private func securityIssue(_ s: Sec?) -> (String, Color)? {
    guard let s else { return nil }
    if s.secrets > 0 { return ("\(s.secrets) secret\(s.secrets == 1 ? "" : "s") in git history", .hRose) }
    if s.advisories > 0 { return ("\(s.advisories) dependency advisor\(s.advisories == 1 ? "y" : "ies")", .hOrange) }
    if let d = securityAge(s), d > 10 { return ("scan \(d) days old", .hOrange) }
    return nil
}

private func gitState(_ p: ProjRow) -> (String, Color, String) {
    if !p.remote { return ("no remote", .hOrange, "icloud.slash") }
    if !p.upstream { return ("branch not pushed", .hOrange, "arrow.up.circle") }
    if p.ahead > 0 { return ("\(p.ahead) unpushed", .hOrange, "arrow.up.circle") }
    if p.changed > 0 { return ("\(p.changed) changed", .hSoft, "pencil") }
    return ("pushed", .hMut, "checkmark")
}

struct ProjectsPage: View {
    @ObservedObject var store: Store
    @ObservedObject var nav: Nav

    private var items: [Item] {
        let rows = store.projects?.projects ?? []
        let uses = store.usage?.rows ?? []
        var out: [Item] = (store.pace?.projects ?? []).map { p in
            Item(name: p.name, state: p.state ?? "", pace: p, row: rows.first { $0.name == p.name }, use: uses.first { $0.name == p.name })
        }
        for r in rows where !out.contains(where: { $0.name == r.name }) {
            out.append(Item(name: r.name, state: r.state ?? "", row: r, use: uses.first { $0.name == r.name }))
        }
        return out
    }

    var body: some View {
        let all = items
        let groups: [(String, [Item])] = [
            ("In development", all.filter { $0.state == "moving" }),
            ("Built", all.filter { $0.state == "built" }),
            ("Parked and other", all.filter { $0.state != "moving" && $0.state != "built" }),
        ]
        VStack(alignment: .leading, spacing: 26) {
            if all.isEmpty {
                Text("Reading projects…").font(.system(size: 14)).foregroundStyle(Color.hMut).frame(maxWidth: .infinity, minHeight: 120)
            }
            ForEach(groups.filter { !$0.1.isEmpty }, id: \.0) { g in
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 8) {
                        Text(g.0).font(.system(size: 15, weight: .semibold)).foregroundStyle(Color.hInk)
                        Text("\(g.1.count)").font(.system(size: 12, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hMut)
                            .padding(.horizontal, 7).padding(.vertical, 1).background(Color.white.opacity(0.06), in: Capsule())
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 270), spacing: 12)], spacing: 12) {
                        ForEach(g.1) { card($0) }
                    }
                    if let sel = g.1.first(where: { $0.name == nav.project }) { detail(sel) }
                }
            }
            HStack(spacing: 14) {
                legend(emerald, "commit day")
                legend(.hRose, "service stopped")
                legend(.hOrange, "needs a push or fix")
                Spacer()
                Text("Click a project for its year").font(.system(size: 12)).foregroundStyle(Color.hMut)
            }
        }
    }

    private func legend(_ c: Color, _ t: String) -> some View {
        HStack(spacing: 6) {
            RoundedRectangle(cornerRadius: 2).fill(c).frame(width: 9, height: 9)
            Text(t).font(.system(size: 12)).foregroundStyle(Color.hMut)
        }
    }

    private func card(_ it: Item) -> some View {
        let on = nav.project == it.name
        let down = !(it.row?.down.isEmpty ?? true)
        let running = it.row?.running ?? 0
        let dot: Color = down ? .hRose : running > 0 ? emerald : Color.white.opacity(0.18)
        let days = it.pace?.commit_days28 ?? 0
        return Button { withAnimation(.easeOut(duration: 0.15)) { nav.project = on ? nil : it.name } } label: {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 9) {
                    Circle().fill(dot).frame(width: 9, height: 9).shadow(color: dot == emerald || down ? dot : .clear, radius: 4)
                    Text(it.name).font(.system(size: 15, weight: .semibold)).foregroundStyle(Color.hInk).lineLimit(1)
                    Spacer(minLength: 4)
                    Text("\(days)").font(.system(size: 20, weight: .semibold)).monospacedDigit()
                        .foregroundStyle(days >= 5 ? emerald : days > 0 ? Color.hSoft : Color.hMut)
                        .help("Commit days in the last 28")
                }
                strip(it.pace?.strip ?? [], tint: (it.pace?.maintained ?? false) ? emerald : .hOrange, height: 18)
                HStack(spacing: 6) {
                    if down {
                        Pill(text: "stopped", tint: .hRose, icon: "exclamationmark.triangle.fill")
                    } else if running > 0 {
                        Pill(text: it.row?.ports.first.map { ":\($0)" } ?? "running", tint: emerald, icon: "bolt.fill")
                    }
                    if let r = it.row {
                        let g = gitState(r)
                        Pill(text: g.0, tint: g.1, icon: g.2)
                    }
                    if let u = it.use, u.secs7 > 0 {
                        Pill(text: Fmt.duration(u.secs7), tint: .hSoft, icon: "clock").help("Time in its app, last 7 days")
                    }
                    if let s = securityIssue(it.row?.security) {
                        Pill(text: "", tint: s.1, icon: "exclamationmark.shield.fill")
                            .help(([s.0] + (it.row?.security?.items?.map(\.short) ?? [])).joined(separator: "\n"))
                    }
                    Spacer(minLength: 0)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                LinearGradient(colors: [Color(hex: 0x17171a), Color(hex: 0x121214)], startPoint: .top, endPoint: .bottom),
                in: RoundedRectangle(cornerRadius: 18, style: .continuous)
            )
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(on ? emerald.opacity(0.7) : Color.hLine, lineWidth: on ? 1.5 : 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func strip(_ s: [Bool], tint: Color, height: CGFloat) -> some View {
        HStack(spacing: 2.5) {
            ForEach(Array(s.enumerated()), id: \.offset) { _, on in
                RoundedRectangle(cornerRadius: 3, style: .continuous)
                    .fill(on ? tint : Color.white.opacity(0.05))
                    .frame(maxWidth: .infinity).frame(height: height)
            }
        }
    }

    private func detail(_ it: Item) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text(it.name).font(.system(size: 20, weight: .semibold)).foregroundStyle(Color.hInk)
                Spacer()
                CloseButton { withAnimation(.easeOut(duration: 0.15)) { nav.project = nil } }
            }
            HStack(alignment: .top, spacing: 14) {
                fact("Commit days", "\(it.pace?.commit_days28 ?? 0)", it.use.map { "\($0.commits28) commits in 4 weeks" } ?? "of the last 28")
                fact("In its app", it.use.map { Fmt.duration($0.secs28) } ?? "–", it.use == nil ? "no app tracked" : "in 4 weeks")
                if let r = it.row {
                    let agents = r.running + r.down.count
                    fact("Services", agents == 0 ? "none" : "\(r.running) / \(agents)", r.ports.isEmpty ? "running" : r.ports.map { ":\($0)" }.joined(separator: " "))
                    let g = gitState(r)
                    fact("Git", g.0, r.changed > 0 && g.0 != "\(r.changed) changed" ? "\(r.changed) changed" : "")
                }
            }
            if let r = it.row, !r.down.isEmpty {
                Label("Stopped: " + r.down.joined(separator: ", "), systemImage: "exclamationmark.triangle.fill")
                    .font(.system(size: 13, weight: .medium)).foregroundStyle(Color.hRose)
            }
            if let s = securityIssue(it.row?.security) {
                Label(s.0, systemImage: "exclamationmark.shield.fill").font(.system(size: 13, weight: .medium)).foregroundStyle(s.1)
                ForEach(it.row?.security?.items ?? []) { i in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(i.short).font(.system(size: 12.5, weight: .semibold, design: .monospaced)).foregroundStyle(Color.hInk)
                        if !i.summary.isEmpty { Text(i.summary).font(.system(size: 12.5)).foregroundStyle(Color.hMut) }
                    }
                    .textSelection(.enabled).padding(.leading, 22)
                }
            }
            if let path = it.row?.path {
                VStack(alignment: .leading, spacing: 8) {
                    let year = store.commitYears[path]
                    Label13("Commit days, last year", trailing: year.map { "\($0.days.count) days, \($0.counts.reduce(0, +)) commits" })
                    if let year {
                        YearGrid(commits: year)
                    } else {
                        strip(it.pace?.strip ?? [], tint: emerald, height: 26)
                    }
                }
                .task(id: path) { store.loadCommits(path) }
            }
            if let u = it.use {
                VStack(alignment: .leading, spacing: 8) {
                    Label13("Days in its app, last year")
                    YearGrid(row: u, since: store.usage?.since)
                }
            }
            if let p = it.row?.path {
                Text(p).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Color.hMut).textSelection(.enabled)
            }
        }
        .padding(20).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(hex: 0x141417), in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(emerald.opacity(0.35), lineWidth: 1))
    }

    private func fact(_ label: String, _ value: String, _ note: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label).font(.system(size: 12)).foregroundStyle(Color.hMut)
            Text(value).font(.system(size: 20, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk).lineLimit(1)
            Text(note.isEmpty ? " " : note).font(.system(size: 11.5)).foregroundStyle(Color.hMut).lineLimit(1)
        }
        .padding(12).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

struct YearGrid: View {
    let used: [String: Int]
    let since: Int?
    let describe: (Int) -> String

    init(row: UseRow, since: Int?) {
        let byTime = row.day_secs.contains { $0 > 0 }
        used = Dictionary(uniqueKeysWithValues: zip(row.days, byTime ? row.day_secs : row.counts)).filter { $0.value > 0 }
        self.since = since
        describe = { byTime ? Fmt.duration($0) : "\($0) opens" }
    }

    init(commits: CommitYear) {
        used = Dictionary(uniqueKeysWithValues: zip(commits.days, commits.counts))
        since = nil
        describe = { $0 == 1 ? "1 commit" : "\($0) commits" }
    }

    var body: some View {
        let cal = Calendar(identifier: .iso8601)
        let today = cal.startOfDay(for: Date())
        let tracked = cal.startOfDay(for: since.map { Date(timeIntervalSince1970: TimeInterval($0)) } ?? today)
        let thisMonday = cal.date(from: cal.dateComponents([.yearForWeekOfYear, .weekOfYear], from: today))!
        let start = cal.date(byAdding: .weekOfYear, value: -52, to: thisMonday)!
        let fmt = DateFormatter()
        fmt.dateFormat = "yyyy-MM-dd"
        let top = max(1, used.values.max() ?? 1)
        let weeks = (0...52).map { cal.date(byAdding: .weekOfYear, value: $0, to: start)! }
        return GeometryReader { g in
            let cell = min(13, (g.size.width - 52 * 3) / 53)
            HStack(spacing: 3) {
                ForEach(weeks, id: \.self) { w in
                    VStack(spacing: 3) {
                        ForEach(0..<7, id: \.self) { d in
                            let day = cal.date(byAdding: .day, value: d, to: w)!
                            let v = used[fmt.string(from: day)]
                            RoundedRectangle(cornerRadius: 2.5, style: .continuous)
                                .fill(day > today ? Color.clear : v.map { emerald.opacity(0.3 + 0.7 * (Double($0) / Double(top)).squareRoot()) }
                                    ?? Color.white.opacity(day < tracked ? 0.025 : 0.06))
                                .frame(width: cell, height: cell)
                                .help(day.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated)) + (v.map { " · " + describe($0) } ?? ""))
                        }
                    }
                }
            }
        }
        .frame(height: 7 * 13 + 6 * 3)
    }
}
