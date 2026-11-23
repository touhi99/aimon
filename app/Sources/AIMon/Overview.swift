import SwiftUI

struct PlanRing: View {
    @ObservedObject var store: Store
    var size: CGFloat = 150

    var body: some View {
        let l = store.snap?.limits
        let live = l?.source == "live"
        let week = live ? l?.weekly?.pct : nil
        HouseRing(value: (week ?? 0) / 100, colors: (week ?? 0) >= 85 ? [Color.hOrange, Color.hRose] : [emerald, Color.hAmber], size: size, stroke: size / 11.5) {
            VStack(spacing: 1) {
                Text(week.map { "\(Int($0.rounded()))%" } ?? Fmt.money(store.snap?.api.today ?? 0))
                    .font(.system(size: size * 0.22, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
                Text(week == nil ? "API today" : size < 100 ? "week" : "plan this week").font(.system(size: max(11, size * 0.08))).foregroundStyle(Color.hMut)
            }
        }
    }
}

struct AlertCard: View {
    let a: Attention
    var action: (() -> Void)?

    var body: some View {
        let tint = a.level == .red ? Color.hRose : Color.hOrange
        Button { action?() } label: {
            HStack(spacing: 14) {
                Circle().fill(tint).frame(width: 10, height: 10).shadow(color: tint, radius: 6)
                VStack(alignment: .leading, spacing: 2) {
                    Text(a.text).font(.system(size: 15, weight: .semibold)).foregroundStyle(Color.hInk)
                    if !a.detail.isEmpty { Text(a.detail).font(.system(size: 13)).foregroundStyle(Color.hMut).lineLimit(1) }
                }
                Spacer(minLength: 0)
                if action != nil { Image(systemName: "chevron.right").font(.system(size: 12, weight: .semibold)).foregroundStyle(Color.hMut) }
            }
            .padding(.horizontal, 16).padding(.vertical, 13)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(LinearGradient(colors: [tint.opacity(0.14), tint.opacity(0.03)], startPoint: .leading, endPoint: .trailing),
                        in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(tint.opacity(0.35), lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct Bars: View {
    let values: [Int?]
    let color: Color

    var body: some View {
        let top = CGFloat(max(values.compactMap { $0 }.max() ?? 1, 1))
        HStack(alignment: .bottom, spacing: 5) {
            ForEach(Array(values.enumerated()), id: \.offset) { i, v in
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(LinearGradient(colors: [color, color.opacity(0.35)], startPoint: .top, endPoint: .bottom))
                    .opacity(v == nil ? 0 : i == values.count - 1 ? 1 : 0.7)
                    .frame(height: v.map { max(5, 70 * CGFloat($0) / top) } ?? 5)
                    .frame(maxWidth: .infinity)
            }
        }
        .frame(height: 70, alignment: .bottom)
    }
}

struct OverviewView: View {
    @ObservedObject var store: Store
    @ObservedObject var nav: Nav

    private func go(_ page: Int, _ project: String? = nil) {
        nav.project = project
        nav.page = page
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            hero
            ForEach(store.attention) { a in AlertCard(a: a) { go(a.page, a.project) } }
            if let p = store.pace { pace(p) }
        }
    }

    private var hero: some View {
        HStack(spacing: 28) {
            Button { go(0) } label: { PlanRing(store: store).contentShape(Circle()) }.buttonStyle(.plain)
            HStack(spacing: 12) {
                stat("API this month", Fmt.money(store.snap?.api.month ?? 0), page: 0)
                stat("Tokens, 7 days", store.snap.map { Fmt.tokens($0.daily_tokens.suffix(7).reduce(0, +)) } ?? "–", page: 0)
                stat("Failed tool calls, 24 h", store.trace.map { "\($0.errors_24h)" } ?? "–", page: 1)
            }
        }
        .padding(22).frame(maxWidth: .infinity, alignment: .leading).housePanel()
    }

    private func stat(_ label: String, _ value: String, page: Int) -> some View {
        Button { go(page) } label: {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text(label).font(.system(size: 12.5)).foregroundStyle(Color.hMut).lineLimit(1)
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.hMut)
                }
                Text(value).font(.system(size: 26, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk)
            }
            .padding(14).frame(maxWidth: .infinity, alignment: .leading)
            .hoverRow()
            .background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Color.hLine, lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    private func pace(_ p: PaceData) -> some View {
        let run = p.weeks.last { $0.runs > 0 }
        let fix = p.weeks.last { $0.rate != nil }?.rate
        return HStack(alignment: .top, spacing: 14) {
            card("Agent works alone", run.map { Fmt.duration($0.p90_secs) } ?? "–", .hInk) {
                Bars(values: p.weeks.map { $0.runs > 0 ? $0.p90_secs : nil }, color: emerald)
            }
            .help("90th percentile of time from a prompt to the agent's last reply")
            card("You correct it", fix.map { "\($0)%" } ?? "–", .hAmber) {
                Bars(values: p.weeks.map(\.rate), color: .hAmber)
            }
            .help("Share of your messages that pushed back, from Backtrail")
            Button { go(3) } label: {
                card("Projects kept alive", "\(p.maintained) / \(p.projects_total)", .hInk) {
                    HouseRing(value: Double(p.maintained) / Double(max(p.projects_total, 1)), colors: [emerald, emerald], size: 62, stroke: 8) { EmptyView() }
                        .frame(maxWidth: .infinity, minHeight: 70, alignment: .bottomLeading)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Repositories with commits on 5 or more days in 4 weeks")
        }
    }

    private func card<C: View>(_ label: String, _ value: String, _ tint: Color, @ViewBuilder chart: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(label).font(.system(size: 13)).foregroundStyle(Color.hMut)
            Text(value).font(.system(size: 30, weight: .semibold)).monospacedDigit().foregroundStyle(tint)
            chart()
        }
        .padding(18).frame(maxWidth: .infinity, alignment: .leading).housePanel()
    }
}
