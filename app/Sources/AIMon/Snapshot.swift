import Foundation

struct Snapshot: Codable {
    struct Limit: Codable { var pct: Double?; var resets_at: String? }
    struct Limits: Codable {
        var source: String
        var fetched: Int?
        var session: Limit?
        var weekly: Limit?
    }
    struct API: Codable {
        struct App: Codable, Identifiable { var name: String; var cost: Double; var tokens: Int; var id: String { name } }
        var today: Double, week: Double, month: Double
        var daily: [Double]
        var by_app: [App]
    }
    struct Claude: Codable { var today_tokens: Int; var week_tokens: Int; var output: Int; var cache_read: Int; var cache_hit: Double }
    struct Ollama: Codable { var today_tokens: Int; var week_tokens: Int }
    struct Hour: Codable { var claude: Int; var ollama: Int; var other: Int }
    struct Named: Codable, Identifiable { var name: String; var tokens: Int; var cost: Double?; var kind: String?; var id: String { name + (kind ?? "") } }
    var generated: Int
    var limits: Limits
    struct OllamaLimits: Codable {
        var source: String
        var first: String?
        var daily: [Double]?
        var today: Double?
        var week: Double?
        var month: Double?
        var requests: Int?
    }
    var ollama_limits: OllamaLimits
    var api: API
    var claude: Claude
    var ollama: Ollama
    var hourly: [Hour]
    var daily_tokens: [Int]
    var projects: [Named]
    var models: [Named]
}

struct MonthView: Codable {
    var label: String
    var first: String
    var total: Double
    var daily: [Double]
    var has_prev: Bool
    var by_app: [Snapshot.API.App]
}

struct TraceTool: Codable, Identifiable { var name: String; var n: Int; var err: Int; var id: String { name } }
struct TraceFail: Codable, Identifiable {
    var tool: String; var arg: String; var msg: String; var kind: String
    var ts: Int?; var project: String?
    var id: String { "\(tool)\(arg)\(msg)\(ts ?? 0)" }
}
struct TraceSession: Codable, Identifiable {
    var fails: [TraceFail]
    var id: String; var project: String; var model: String
    var start: Int; var end: Int; var tokens: Int; var calls: Int; var tools: Int; var errors: Int
    var top: [TraceTool]
}
struct TraceData: Codable {
    var sessions: [TraceSession]
    var tools_24h: Int; var errors_24h: Int
    var top_24h: [TraceTool]
    var kinds_24h: [String: Int]
    var recent_fails: [TraceFail]
}
struct NetConn: Codable, Identifiable {
    var host: String; var ip: String; var port: String; var known: Bool; var pid: String; var up: String
    var id: String { pid + ip + port }
}
struct NetProc: Codable, Identifiable {
    var name: String; var ai: Bool; var conns: Int; var unknown: Int
    var `in`: Int; var out: Int; var hosts: [String]; var detail: [NetConn]
    var id: String { name }
}
struct NetData: Codable { var procs: [NetProc] }
struct ProjRow: Codable, Identifiable {
    var name: String; var path: String
    var running: Int; var down: [String]; var ports: [Int]
    var remote: Bool; var upstream: Bool; var ahead: Int; var changed: Int
    var id: String { path }
}
struct ProjData: Codable { var root: String?; var projects: [ProjRow] }

enum Fmt {
    static func bytes(_ n: Int) -> String {
        let f = Double(n)
        if f >= 1e9 { return String(format: "%.1f GB", f / 1e9) }
        if f >= 1e6 { return String(format: "%.1f MB", f / 1e6) }
        if f >= 1e3 { return String(format: "%.0f KB", f / 1e3) }
        return "\(n) B"
    }
    static func ago(_ ts: Int) -> String {
        let s = Int(Date().timeIntervalSince1970) - ts
        if s < 90 { return "now" }
        if s < 3600 { return "\(s / 60)m ago" }
        if s < 86400 { return "\(s / 3600)h ago" }
        return "\(s / 86400)d ago"
    }
    static func dur(_ a: Int, _ b: Int) -> String {
        let m = max(0, b - a) / 60
        return m < 60 ? "\(max(m, 1))m" : "\(m / 60)h \(m % 60)m"
    }
    static func money(_ v: Double) -> String {
        if v == 0 { return "$0" }
        return v >= 100 ? String(format: "$%.0f", v) : String(format: "$%.2f", v)
    }
    static func tokens(_ n: Int) -> String {
        let f = Double(n)
        if f >= 1e9 { return String(format: "%.2fB", f / 1e9) }
        if f >= 1e6 { return String(format: "%.1fM", f / 1e6) }
        if f >= 1e3 { return String(format: "%.0fK", f / 1e3) }
        return "\(n)"
    }
    static func resets(_ iso: String?) -> String {
        guard let iso else { return "" }
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let d = f.date(from: iso) ?? ISO8601DateFormatter().date(from: iso) else { return "" }
        let s = Int(d.timeIntervalSinceNow)
        if s <= 0 { return "resetting" }
        if s < 3600 { return "resets in \(s / 60)m" }
        if s < 86400 { return "resets in \(s / 3600)h \((s % 3600) / 60)m" }
        let df = DateFormatter(); df.dateFormat = "EEE HH:mm"
        return "resets " + df.string(from: d)
    }
}

@MainActor
final class Store: ObservableObject {
    @Published var snap: Snapshot?
    @Published var error: String?
    @Published var monthBack = 0            // 0 = live card, 1 = last month, ...
    @Published var month: MonthView?
    @Published var trace: TraceData?
    @Published var net: NetData?
    @Published var projects: ProjData?
    var onChange: (() -> Void)?
    private var timer: Timer?

    private var binary: String {
        if let r = Bundle.main.resourcePath, FileManager.default.isExecutableFile(atPath: r + "/aimon") { return r + "/aimon" }
        return "aimon"
    }

    /// Turns on an optional feature that uses a login on this Mac (`claude_limits` or `ollama`), then reloads.
    func enable(_ key: String) {
        let bin = binary
        Task.detached {
            _ = Store.run(bin, ["config", key, "on"])
            await MainActor.run { self.refresh() }
        }
    }

    /// Sets the folder whose git repositories the Projects tab lists, then reloads.
    func setProjects(_ folder: String) {
        let bin = binary
        Task.detached {
            _ = Store.run(bin, ["config", "projects", folder])
            await MainActor.run { self.refresh() }
        }
    }

    func start() {
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    func setMonth(_ back: Int) {
        monthBack = max(0, back)
        if monthBack == 0 { month = nil; return }
        let bin = binary, n = monthBack
        Task.detached {
            let p = Process()
            p.executableURL = URL(fileURLWithPath: bin)
            p.arguments = ["month", "\(n)"]
            let out = Pipe()
            p.standardOutput = out
            p.standardError = FileHandle.nullDevice
            guard (try? p.run()) != nil else { return }
            let data = out.fileHandleForReading.readDataToEndOfFile()
            p.waitUntilExit()
            if let m = try? JSONDecoder().decode(MonthView.self, from: data) {
                await MainActor.run { if self.monthBack == n { self.month = m } }
            }
        }
    }

    nonisolated private static func run(_ bin: String, _ args: [String]) -> Data? {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: bin)
        p.arguments = args
        let out = Pipe()
        p.standardOutput = out
        p.standardError = FileHandle.nullDevice
        guard (try? p.run()) != nil else { return nil }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        return data
    }

    private var refreshing = false

    func refresh() {
        guard !refreshing else { return }   // never stack refreshes: each one scans logs and writes the database
        refreshing = true
        let bin = binary
        Task.detached {
            defer { Task { @MainActor in self.refreshing = false } }
            do {
                let s = try JSONDecoder().decode(Snapshot.self, from: Store.run(bin, ["snapshot"]) ?? Data())
                await MainActor.run { self.snap = s; self.error = nil; self.onChange?() }
            } catch {
                await MainActor.run { self.error = "\(error)"; self.onChange?() }
            }
            let p = Store.run(bin, ["projects"]).flatMap { try? JSONDecoder().decode(ProjData.self, from: $0) }
            await MainActor.run { if let p { self.projects = p; self.onChange?() } }
            // sequential on purpose: every run scans logs and writes the same database
            let n = Store.run(bin, ["net"]).flatMap { try? JSONDecoder().decode(NetData.self, from: $0) }
            let t = Store.run(bin, ["trace"]).flatMap { try? JSONDecoder().decode(TraceData.self, from: $0) }
            await MainActor.run { if let n { self.net = n }; if let t { self.trace = t } }
        }
    }
}
