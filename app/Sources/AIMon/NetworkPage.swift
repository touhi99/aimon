import Charts
import SwiftUI

private let violet = Color(hex: 0xc084fc)

struct NetworkPage: View {
    @ObservedObject var store: Store
    @State private var open: String?

    var body: some View {
        if store.netDays > 1 {
            history(store.netHistory)
        } else if let n = store.net {
            let top = Double(max(n.procs.map { max($0.in, $0.out) }.max() ?? 1, 1))
            VStack(alignment: .leading, spacing: 14) {
                header("click a process for its hosts")
                if n.procs.isEmpty {
                    Text("Nothing sampled yet.").font(.system(size: 13)).foregroundStyle(Color.hMut)
                }
                VStack(spacing: 2) {
                    ForEach(n.procs) { p in row(p, top: top) }
                }
                HStack(spacing: 16) {
                    legend(violet, "AI process")
                    legend(.hOrange, "has hosts not recognised")
                    Spacer()
                }
            }
            .padding(20).housePanel()
        } else {
            Text("Sampling connections…").font(.system(size: 14)).foregroundStyle(Color.hMut).frame(maxWidth: .infinity, minHeight: 120)
        }
    }

    private func header(_ hint: String?) -> some View {
        PanelHeader(title: "Traffic per process", hint: hint,
                    options: [(days: 1, label: "24H"), (days: 90, label: "90D"), (days: 365, label: "1Y")],
                    value: store.netDays, pick: store.setNetDays)
    }

    private func history(_ h: NetHistory?) -> some View {
        let days = h?.daily ?? []
        let procs = h?.procs ?? []
        let top = Double(max(procs.map { max($0.in, $0.out) }.max() ?? 1, 1))
        let since = h?.first.flatMap { f in
            let fmt = DateFormatter(); fmt.dateFormat = "yyyy-MM-dd"
            return fmt.date(from: f)?.formatted(.dateTime.month(.abbreviated).day())
        }
        return VStack(alignment: .leading, spacing: 14) {
            header(since.map { "kept since \($0)" })
            if h == nil {
                Text("Reading history…").font(.system(size: 13)).foregroundStyle(Color.hMut)
            } else {
                Chart(days) { d in
                    BarMark(x: .value("day", Date(timeIntervalSince1970: TimeInterval(d.day)), unit: .day), y: .value("bytes", d.out))
                        .foregroundStyle(by: .value("dir", "sent"))
                    BarMark(x: .value("day", Date(timeIntervalSince1970: TimeInterval(d.day)), unit: .day), y: .value("bytes", d.in))
                        .foregroundStyle(by: .value("dir", "received"))
                }
                .chartForegroundStyleScale(["sent": violet, "received": violet.opacity(0.4)])
                .chartLegend(.hidden)
                .chartYAxis {
                    AxisMarks(position: .leading, values: .automatic(desiredCount: 3)) { v in
                        AxisGridLine().foregroundStyle(Color.white.opacity(0.06))
                        AxisValueLabel { if let b = v.as(Int.self) { Text(Fmt.bytes(b)).foregroundStyle(Color.hMut) } }
                    }
                }
                .chartXAxis {
                    if store.netDays > 90 {
                        AxisMarks(values: .stride(by: .month)) { _ in AxisValueLabel(format: .dateTime.month(.abbreviated)).foregroundStyle(Color.hMut) }
                    } else {
                        AxisMarks(values: .stride(by: .day, count: 14)) { _ in AxisValueLabel(format: .dateTime.month(.abbreviated).day()).foregroundStyle(Color.hMut) }
                    }
                }
                .frame(height: 150)
                VStack(spacing: 2) {
                    ForEach(procs) { p in
                        HStack(spacing: 12) {
                            Circle().fill(p.ai ? violet : Color.white.opacity(0.18)).frame(width: 8, height: 8)
                            Text(p.name).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Color.hInk).lineLimit(1).frame(width: 210, alignment: .leading)
                            bars(out: p.out, in: p.in, top: top, tint: p.ai ? violet : Color(hex: 0x71717a))
                            Text("↑ \(Fmt.bytes(p.out))").font(.system(size: 12.5, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk).frame(width: 76, alignment: .trailing)
                            Text("↓ \(Fmt.bytes(p.in))").font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hMut).frame(width: 76, alignment: .trailing)
                        }
                        .frame(height: 34).padding(.horizontal, 8)
                    }
                }
                HStack(spacing: 16) {
                    legend(violet, "AI process")
                    Spacer()
                    Text("Open connections show on 24H").font(.system(size: 12)).foregroundStyle(Color.hMut)
                }
            }
        }
        .padding(20).housePanel()
    }

    private func bars(out: Int, in inb: Int, top: Double, tint: Color) -> some View {
        GeometryReader { g in
            HStack(spacing: 2) {
                Capsule().fill(tint).frame(width: max(3, g.size.width * (Double(out) / top).squareRoot() * 0.5))
                Capsule().fill(tint.opacity(0.4)).frame(width: max(3, g.size.width * (Double(inb) / top).squareRoot() * 0.5))
            }
            .frame(height: 10).frame(maxHeight: .infinity)
        }
    }

    private func legend(_ c: Color, _ t: String) -> some View {
        HStack(spacing: 6) {
            Circle().fill(c).frame(width: 8, height: 8)
            Text(t).font(.system(size: 12)).foregroundStyle(Color.hMut)
        }
    }

    private func row(_ p: NetProc, top: Double) -> some View {
        let on = open == p.name
        let tint: Color = p.unknown > 0 ? .hOrange : p.ai ? violet : Color(hex: 0x71717a)
        return VStack(alignment: .leading, spacing: 8) {
            Button { withAnimation(.easeOut(duration: 0.15)) { open = on ? nil : p.name } } label: {
                HStack(spacing: 12) {
                    Circle().fill(p.ai ? violet : Color.white.opacity(0.18)).frame(width: 8, height: 8)
                    Text(p.name).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Color.hInk).lineLimit(1).frame(width: 210, alignment: .leading)
                    bars(out: p.out, in: p.in, top: top, tint: tint)
                    if p.unknown > 0 { Pill(text: "\(p.unknown) unknown", tint: .hOrange) }
                    Text("↑ \(Fmt.bytes(p.out))").font(.system(size: 12.5, weight: .semibold)).monospacedDigit().foregroundStyle(Color.hInk).frame(width: 76, alignment: .trailing)
                    Text("↓ \(Fmt.bytes(p.in))").font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hMut).frame(width: 76, alignment: .trailing)
                    Image(systemName: on ? "chevron.down" : "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.hMut)
                }
                .frame(height: 34).padding(.horizontal, 8)
            }
            .buttonStyle(.plain)
            if on {
                VStack(alignment: .leading, spacing: 6) {
                    if p.detail.isEmpty {
                        Text(p.hosts.isEmpty ? "No open connections right now." : p.hosts.joined(separator: "  ·  "))
                            .font(.system(size: 12.5)).foregroundStyle(Color.hMut)
                    }
                    ForEach(p.detail) { d in
                        HStack(spacing: 8) {
                            Image(systemName: d.known ? "checkmark.circle.fill" : "questionmark.circle.fill")
                                .font(.system(size: 12)).foregroundStyle(d.known ? emerald : Color.hOrange)
                            Text(d.host).font(.system(size: 12.5)).foregroundStyle(Color.hSoft).lineLimit(1).truncationMode(.middle)
                            Spacer()
                            Text(":\(d.port)  pid \(d.pid)  up \(d.up)").font(.system(size: 12)).monospacedDigit().foregroundStyle(Color.hMut)
                        }
                        .help(d.ip)
                    }
                    Text("Hosts come from reverse DNS, so a CDN name often stands in for the real service.")
                        .font(.system(size: 11.5)).foregroundStyle(Color.hMut)
                }
                .padding(.leading, 28).padding(.trailing, 8).padding(.bottom, 10)
            }
        }
        .hoverRow(on)
    }
}
