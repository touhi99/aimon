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
    struct Named: Codable, Identifiable { var name: String; var tokens: Int; var cost: Double?; var kind: String?; var daily: [Int]?; var id: String { name + (kind ?? "") } }
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

struct TraceDays: Codable { var daily: [TraceDay] }
struct DaySessions: Codable { var sessions: [TraceSession] }
struct NetDay: Codable, Identifiable {
    var day: Int; var `in`: Int; var out: Int
    var id: Int { day }
}
struct NetRangeProc: Codable, Identifiable {
    var name: String; var `in`: Int; var out: Int; var ai: Bool
    var id: String { name }
}
struct NetHistory: Codable { var first: String?; var daily: [NetDay]; var procs: [NetRangeProc] }
struct CommitYear: Codable { var days: [String]; var counts: [Int] }

struct LogService: Codable, Identifiable {
    var label: String; var project: String?; var path: String; var kind: String
    var loaded: Bool; var pid: Int?; var status: Int; var down: Bool
    var size: Int; var modified: Int?; var errors: Int; var last_error: Int?; var tracked: Int?; var lines: [LogLine]
    var id: String { label }
    var failing: Bool { down || status != 0 }
    var name: String { label.split(separator: ".").dropFirst().joined(separator: ".") }
}
struct LogLine: Codable {
    var t: Int?; var own: Bool; var pid: Int?; var err: Bool; var text: String
}
struct LogData: Codable { var dir: String; var services: [LogService] }

struct TokenHistory: Codable {
    var daily: [Int]
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
    var daily: [TraceDay]?
}
struct TraceDay: Codable, Identifiable {
    var day: Int; var calls: Int; var exit: Int; var blocked: Int; var error: Int
    var id: Int { day }
    var fails: Int { exit + blocked + error }
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
    var name: String; var state: String?; var path: String
    var running: Int; var down: [String]; var ports: [Int]
    var remote: Bool; var upstream: Bool; var ahead: Int; var changed: Int
    var security: Sec?
    var id: String { path }
}
struct SecItem: Codable, Identifiable {
    var package: String; var version: String; var id: String; var summary: String
    var short: String { "\(package) \(version) · \(id)" }
}
struct Sec: Codable { var secrets: Int; var advisories: Int; var items: [SecItem]?; var checked: String? }
struct ProjData: Codable { var root: String?; var projects: [ProjRow] }
struct UseRow: Codable, Identifiable {
    var name: String; var path: String; var apps: [String]
    var days7: Int; var days28: Int; var uses7: Int; var uses28: Int; var secs7: Int; var secs28: Int; var last: Int?
    var commits7: Int; var commits28: Int; var status: String
    var weeks: [Int]; var week_secs: [Int]; var days: [String]; var counts: [Int]; var day_secs: [Int]
    var id: String { path }
}
struct UseData: Codable { var since: Int?; var rows: [UseRow]; var untracked: [String] }

struct PaceWeek: Codable, Identifiable {
    var week: String; var runs: Int; var p50_secs: Int; var p90_secs: Int; var tagged_turns: Int; var corrections: Int
    var id: String { week }
    var rate: Int? { tagged_turns > 0 ? corrections * 100 / tagged_turns : nil }
}
struct PaceProject: Codable, Identifiable {
    var name: String; var state: String?; var commit_days28: Int; var maintained: Bool; var strip: [Bool]?
    var id: String { name }
}
struct PaceData: Codable {
    var weeks: [PaceWeek]; var corrections_tracked: Bool
    var maintained: Int; var projects_total: Int; var projects: [PaceProject]
}

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
    static func duration(_ secs: Int) -> String {
        if secs < 60 { return secs == 0 ? "0m" : "<1m" }
        let m = secs / 60
        return m < 60 ? "\(m)m" : "\(m / 60)h \(m % 60)m"
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
    @Published var monthBack = 0
    @Published var month: MonthView?
    @Published var tokenDays = 14
    @Published var tokenHistory: [Int]?
    @Published var failDays = 7
    @Published var failHistory: [TraceDay]?
    @Published var daySessions: [Int: [TraceSession]] = [:]
    @Published var netDays = 1
    @Published var netHistory: NetHistory?
    @Published var commitYears: [String: CommitYear] = [:]
    @Published var logs: LogData?
    @Published var trace: TraceData?
    @Published var net: NetData?
    @Published var projects: ProjData?
    @Published var usage: UseData?
    @Published var pace: PaceData?
    private var usageAt = Date.distantPast
    var trackedApps: Set<String> { Set(usage?.rows.flatMap { $0.apps } ?? []) }
    var onChange: (() -> Void)?
    private var timer: Timer?

    private var binary: String {
        if let r = Bundle.main.resourcePath, FileManager.default.isExecutableFile(atPath: r + "/aimon") { return r + "/aimon" }
        return "aimon"
    }

    func enable(_ key: String) {
        let bin = binary
        Task.detached {
            _ = Store.run(bin, ["config", key, "on"])
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
            if let data = Store.run(bin, ["month", "\(n)"]), let m = try? JSONDecoder().decode(MonthView.self, from: data) {
                await MainActor.run { if self.monthBack == n { self.month = m } }
            }
        }
    }

    func setTokenDays(_ n: Int) {
        tokenDays = n
        if n == 14 { tokenHistory = nil; return }
        let bin = binary
        Task.detached {
            guard let data = Store.run(bin, ["tokens", "\(n)"]),
                  let h = try? JSONDecoder().decode(TokenHistory.self, from: data) else { return }
            await MainActor.run { if self.tokenDays == n { self.tokenHistory = h.daily } }
        }
    }

    private func fetch<T: Decodable & Sendable>(_ args: [String], _ type: T.Type, _ apply: @escaping @MainActor (T) -> Void) {
        let bin = binary
        Task.detached {
            guard let data = Store.run(bin, args), let v = try? JSONDecoder().decode(T.self, from: data) else { return }
            await MainActor.run { apply(v) }
        }
    }

    func setFailDays(_ n: Int) {
        failDays = n
        if n == 7 { failHistory = nil; return }
        fetch(["fails", "\(n)"], TraceDays.self) { if self.failDays == n { self.failHistory = $0.daily } }
    }

    func loadSessions(_ day: Int) {
        guard daySessions[day] == nil else { return }
        fetch(["sessions", "\(day)"], DaySessions.self) { self.daySessions[day] = $0.sessions }
    }

    func setNetDays(_ n: Int) {
        netDays = n
        if n == 1 { netHistory = nil; return }
        fetch(["netdays", "\(n)"], NetHistory.self) { if self.netDays == n { self.netHistory = $0 } }
    }

    func loadCommits(_ path: String) {
        fetch(["commits", path], CommitYear.self) { self.commitYears[path] = $0 }
    }

    func recordUse(_ key: String) {
        let bin = binary
        Task.detached { _ = Store.run(bin, ["use", key, "--via", "app"]) }
    }

    func recordTime(_ key: String, _ secs: Int) {
        let bin = binary
        Task.detached { _ = Store.run(bin, ["time", key, String(secs)]) }
    }

    nonisolated private static func run(_ bin: String, _ args: [String]) -> Data? {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: bin)
        p.arguments = args
        let out = Pipe()
        p.standardOutput = out
        let err = Pipe()
        p.standardError = err
        guard (try? p.run()) != nil else { return nil }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        let errData = err.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        if p.terminationStatus != 0 {
            let stamp = ISO8601DateFormatter().string(from: Date())
            FileHandle.standardError.write(Data("\(stamp) aimon \(args.joined(separator: " ")) exited \(p.terminationStatus)\n".utf8) + errData)
        }
        return data
    }

    private var refreshing = false

    func refresh() {
        guard !refreshing else { return }
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
            if await MainActor.run(body: { Date().timeIntervalSince(self.usageAt) > 300 }) {
                let u = Store.run(bin, ["usage"]).flatMap { try? JSONDecoder().decode(UseData.self, from: $0) }
                await MainActor.run { if let u { self.usage = u; self.usageAt = Date() } }
                let pc = Store.run(bin, ["pace"]).flatMap { try? JSONDecoder().decode(PaceData.self, from: $0) }
                await MainActor.run { if let pc { self.pace = pc } }
            }
            let lg = Store.run(bin, ["logs"]).flatMap { try? JSONDecoder().decode(LogData.self, from: $0) }
            await MainActor.run { if let lg { self.logs = lg } }
            let n = Store.run(bin, ["net"]).flatMap { try? JSONDecoder().decode(NetData.self, from: $0) }
            let t = Store.run(bin, ["trace"]).flatMap { try? JSONDecoder().decode(TraceData.self, from: $0) }
            await MainActor.run { if let n { self.net = n }; if let t { self.trace = t } }
        }
    }
}
