import AppKit
import SwiftUI

private let recentSecs = 86400

private func isRecent(_ t: Int?) -> Bool {
    guard let t else { return false }
    return Int(Date().timeIntervalSince1970) - t < recentSecs
}

private func stamp(_ t: Int?) -> String {
    guard let t else { return "" }
    let d = Date(timeIntervalSince1970: TimeInterval(t))
    return Calendar.current.isDateInToday(d) ? d.formatted(.dateTime.hour().minute()) : d.formatted(.dateTime.month(.abbreviated).day().hour().minute())
}

private enum Row: Identifiable {
    case line(Int, LogLine)
    case divider(Int, String)
    var id: Int {
        switch self {
        case .line(let i, _): i * 2
        case .divider(let i, _): i * 2 + 1
        }
    }
}

private func rows(_ s: LogService, errorsOnly: Bool) -> [Row] {
    var out: [Row] = []
    var prev: LogLine?
    for (i, l) in s.lines.enumerated() where !errorsOnly || l.err {
        if i == 0, l.t == nil, let tr = s.tracked {
            out.append(.divider(i, "Written before \(stamp(tr)), when aimon started dating lines"))
        } else if let p = prev, p.t == nil, l.t != nil {
            out.append(.divider(i, "From here, times are when the line was written"))
        } else if let p = prev, let a = p.pid, let b = l.pid, a != b {
            out.append(.divider(i, "Restarted \(stamp(l.t))"))
        }
        out.append(.line(i, l))
        prev = l
    }
    return out
}

private func ago(_ ts: Int?) -> String {
    guard let ts else { return "never written" }
    let s = Int(Date().timeIntervalSince1970) - ts
    if s < 60 { return "just now" }
    if s < 3600 { return "\(s / 60)m ago" }
    if s < 86400 { return "\(s / 3600)h ago" }
    return "\(s / 86400)d ago"
}

struct LogsPage: View {
    @ObservedObject var store: Store
    @ObservedObject var nav: Nav
    @State private var errorsOnly = false

    var body: some View {
        if let d = store.logs {
            VStack(alignment: .leading, spacing: 16) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
                    ForEach(d.services) { tile($0) }
                }
                if let s = d.services.first(where: { $0.label == nav.project }) {
                    detail(s)
                }
                HStack(spacing: 14) {
                    legend(emerald, "running")
                    legend(Color.white.opacity(0.3), "runs on a schedule or by hand")
                    legend(.hRose, "stopped or failed")
                    Spacer()
                    Text("Lines older than 24 hours are dimmed").font(.system(size: 12)).foregroundStyle(Color.hMut)
                    Text("Click a service for its log").font(.system(size: 12)).foregroundStyle(Color.hMut)
                }
            }
        } else {
            Text("Reading logs…").font(.system(size: 14)).foregroundStyle(Color.hMut).frame(maxWidth: .infinity, minHeight: 120)
        }
    }

    private func color(_ s: LogService) -> Color {
        s.failing ? .hRose : s.pid != nil ? emerald : Color.white.opacity(0.3)
    }

    private func legend(_ c: Color, _ t: String) -> some View {
        HStack(spacing: 6) {
            Circle().fill(c).frame(width: 8, height: 8)
            Text(t).font(.system(size: 12)).foregroundStyle(Color.hMut)
        }
    }

    private func tile(_ s: LogService) -> some View {
        let on = nav.project == s.label
        let c = color(s)
        return Button { withAnimation(.easeOut(duration: 0.15)) { nav.project = on ? nil : s.label } } label: {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Circle().fill(c).frame(width: 9, height: 9).shadow(color: c == emerald || s.failing ? c : .clear, radius: 4)
                    Text(s.name).font(.system(size: 14, weight: .semibold)).foregroundStyle(Color.hInk).lineLimit(1)
                    Spacer(minLength: 4)
                }
                HStack(spacing: 6) {
                    if s.down {
                        Pill(text: "stopped", tint: .hRose, icon: "exclamationmark.triangle.fill")
                    } else if s.status != 0 {
                        Pill(text: "exit \(s.status)", tint: .hRose, icon: "exclamationmark.triangle.fill")
                    }
                    if s.errors > 0 {
                        Pill(text: "\(s.errors) in 24h", tint: .hRose, icon: "exclamationmark.circle")
                            .help("Errors in the last 24 hours, the latest \(ago(s.last_error))")
                    }
                    Spacer(minLength: 0)
                    Text(ago(s.modified)).font(.system(size: 12)).foregroundStyle(Color.hMut)
                }
                .frame(height: 24)
                Text(s.project ?? "not in the workspace").font(.system(size: 12)).foregroundStyle(Color.hMut).lineLimit(1)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                LinearGradient(colors: [Color(hex: 0x17171a), Color(hex: 0x121214)], startPoint: .top, endPoint: .bottom),
                in: RoundedRectangle(cornerRadius: 16, style: .continuous)
            )
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(on ? emerald.opacity(0.7) : Color.hLine, lineWidth: on ? 1.5 : 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func detail(_ s: LogService) -> some View {
        let list = rows(s, errorsOnly: errorsOnly)
        return VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                Text(s.name).font(.system(size: 20, weight: .semibold)).foregroundStyle(Color.hInk)
                Text(Fmt.bytes(s.size)).font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.hMut)
                Spacer()
                Toggle("Errors only", isOn: $errorsOnly).toggleStyle(.switch).controlSize(.small)
                    .font(.system(size: 12.5)).foregroundStyle(Color.hSoft)
                Button("Open in Console") {
                    NSWorkspace.shared.open([URL(fileURLWithPath: s.path)], withApplicationAt: URL(fileURLWithPath: "/System/Applications/Utilities/Console.app"), configuration: NSWorkspace.OpenConfiguration())
                }
                .controlSize(.small)
                CloseButton { withAnimation(.easeOut(duration: 0.15)) { nav.project = nil } }
            }
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        if list.isEmpty {
                            Text(errorsOnly ? "No error lines in the latest output." : "The log is empty.")
                                .font(.system(size: 12.5)).foregroundStyle(Color.hMut)
                        }
                        ForEach(list) { r in
                            switch r {
                            case .divider(_, let text):
                                HStack(spacing: 8) {
                                    Rectangle().fill(Color.hLine).frame(width: 16, height: 1)
                                    Text(text).font(.system(size: 11.5, weight: .medium)).foregroundStyle(Color.hMut)
                                    Rectangle().fill(Color.hLine).frame(height: 1)
                                }
                                .padding(.vertical, 6).padding(.horizontal, 8)
                            case .line(_, let l):
                                let now = isRecent(l.t)
                                HStack(alignment: .firstTextBaseline, spacing: 10) {
                                    Text(stamp(l.t)).font(.system(size: 11, design: .monospaced)).foregroundStyle(Color.hMut)
                                        .frame(width: 92, alignment: .trailing)
                                        .help(l.t == nil ? "Written before aimon started dating lines" : l.own ? "Time printed by the service" : "Time aimon saw the line, within 30 seconds")
                                    Text(l.text.isEmpty ? " " : l.text)
                                        .font(.system(size: 11.5, design: .monospaced))
                                        .foregroundStyle(l.err ? Color.hRose : Color.hSoft)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                }
                                .padding(.horizontal, 8).padding(.vertical, 1)
                                .background(l.err && now ? Color.hRose.opacity(0.1) : .clear)
                                .opacity(now ? 1 : 0.45)
                            }
                        }
                        Color.clear.frame(height: 1).id("end")
                    }
                    .textSelection(.enabled)
                }
                .frame(height: 420)
                .background(Color.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .onAppear { proxy.scrollTo("end", anchor: .bottom) }
                .onChange(of: s.lines.count) { proxy.scrollTo("end", anchor: .bottom) }
                .onChange(of: errorsOnly) { proxy.scrollTo("end", anchor: .bottom) }
            }
            Text(s.path).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Color.hMut).textSelection(.enabled)
        }
        .padding(20).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(hex: 0x141417), in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(emerald.opacity(0.35), lineWidth: 1))
    }
}
