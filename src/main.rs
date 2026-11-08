use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

fn home() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap()) }
fn data_dir() -> PathBuf { let d = home().join("Library/Application Support/aimon"); fs::create_dir_all(&d).ok(); d }

fn open_db() -> Connection {
    let c = Connection::open(data_dir().join("aimon.db")).unwrap();
    let ver: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
    if ver < 2 { c.execute_batch("DROP TABLE IF EXISTS calls; DROP TABLE IF EXISTS files; PRAGMA user_version=2;").unwrap(); }
    // v3 adds the tools table: forget file offsets once so old logs are re-read for tool calls (calls upsert, so no double counting)
    if ver < 3 { c.execute_batch("DROP TABLE IF EXISTS files; PRAGMA user_version=3;").unwrap(); }
    // v4 stores what a tool was called with and why it failed
    if ver < 4 { c.execute_batch("DROP TABLE IF EXISTS files; PRAGMA user_version=4;").unwrap(); }
    c.execute_batch("
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS calls(
            id TEXT PRIMARY KEY, ts INTEGER, source TEXT, project TEXT, model TEXT,
            inp INTEGER, out INTEGER, cread INTEGER, cw5 INTEGER, cw1 INTEGER, session TEXT,
            kind TEXT, cost_override REAL);
        CREATE INDEX IF NOT EXISTS calls_ts ON calls(ts);
        CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, off INTEGER);
        CREATE TABLE IF NOT EXISTS tools(id TEXT PRIMARY KEY, ts INTEGER, sess TEXT, project TEXT, tool TEXT, err INTEGER DEFAULT 0);
        CREATE INDEX IF NOT EXISTS tools_sess ON tools(sess);
        CREATE TABLE IF NOT EXISTS netprev(k TEXT PRIMARY KEY, bin INTEGER, bout INTEGER);
        CREATE TABLE IF NOT EXISTS net(ts INTEGER, proc TEXT, din INTEGER, dout INTEGER);
        CREATE INDEX IF NOT EXISTS net_ts ON net(ts);
        CREATE TABLE IF NOT EXISTS rdns(ip TEXT PRIMARY KEY, name TEXT);
        CREATE TABLE IF NOT EXISTS fwd(host TEXT PRIMARY KEY, ips TEXT, ts INTEGER);
        CREATE TABLE IF NOT EXISTS pidlabel(pid TEXT, base TEXT, label TEXT, PRIMARY KEY(pid, base));
    ").unwrap();
    c.execute("ALTER TABLE tools ADD COLUMN arg TEXT", []).ok();
    c.execute("ALTER TABLE tools ADD COLUMN msg TEXT", []).ok();
    c
}

/// Inbox: any script can append JSON lines to ~/Library/Application Support/aimon/inbox/*.jsonl, e.g.
/// {"project":"my-app","model":"gpt-5","in":1200,"out":300,"cost":0.004}  (ts, cache_read, cost are optional).
fn scan_inbox(c: &Connection) {
    let dir = data_dir().join("inbox");
    fs::create_dir_all(&dir).ok();
    let Ok(rd) = fs::read_dir(&dir) else { return };
    let tx = c.unchecked_transaction().unwrap();
    for f in rd.flatten() {
        let path = f.path();
        if path.extension().map_or(true, |e| e != "jsonl") { continue; }
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let Ok(txt) = fs::read_to_string(&path) else { continue };
        for (n, line) in txt.lines().enumerate() {
            let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
            let g = |k: &str| d.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
            let ts = d.get("ts").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.timestamp())))
                .unwrap_or_else(|| f.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64));
            let project = d.get("project").and_then(|v| v.as_str()).unwrap_or("unassigned");
            let model = d.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = if d.get("local").and_then(|v| v.as_bool()).unwrap_or(false) { "local" } else { "api" };
            tx.execute("INSERT OR REPLACE INTO calls VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![format!("inbox:{name}:{n}"), ts, "log", project, model, g("in"), g("out"), g("cache_read"), g("cache_write"), 0, "", kind, d.get("cost").and_then(|v| v.as_f64())]).unwrap();
        }
    }
    tx.commit().unwrap();
}

/// `aimon log --project X --model M --in N --out N [--cost USD] [--local]` appends one line to the inbox.
fn log_cmd(a: &[String]) {
    use std::io::Write;
    let arg = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let (Some(project), Some(model)) = (arg("--project"), arg("--model")) else {
        eprintln!("usage: aimon log --project NAME --model MODEL --in N --out N [--cost USD] [--local]"); return };
    let num = |k: &str| arg(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let mut line = serde_json::json!({"ts": Local::now().timestamp(), "project": project, "model": model, "in": num("--in"), "out": num("--out"), "cache_read": num("--cache-read"), "cache_write": num("--cache-write")});
    if let Some(c) = arg("--cost").and_then(|v| v.parse::<f64>().ok()) { line["cost"] = c.into(); }
    if a.iter().any(|x| x == "--local") { line["local"] = true.into(); }
    let dir = data_dir().join("inbox");
    fs::create_dir_all(&dir).ok();
    let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join("manual.jsonl")).unwrap();
    writeln!(f, "{line}").unwrap();
    eprintln!("logged to {}", dir.join("manual.jsonl").display());
}

/// Project name for a working directory: a folder rule from aliases.json ("/path/to/repo" or "~/path/to/repo",
/// covers everything under it), otherwise the last folder name.
fn project_for(cwd: &str, aliases: &HashMap<String, String>) -> String {
    for (k, name) in aliases {
        let base = match k.strip_prefix("~/") { Some(r) => home().join(r), None if k.starts_with('/') => PathBuf::from(k), None => continue };
        if !cwd.is_empty() && Path::new(cwd).starts_with(&base) { return name.clone(); }
    }
    Path::new(cwd).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "?".into())
}

fn scan(c: &mut Connection) -> (usize, usize) {
    let aliases = load_aliases();
    let root = home().join(".claude/projects");
    let mut files = vec![];
    if let Ok(dirs) = fs::read_dir(&root) {
        for d in dirs.flatten() {
            if let Ok(fs_) = fs::read_dir(d.path()) {
                for f in fs_.flatten() {
                    if f.path().extension().map_or(false, |e| e == "jsonl") { files.push(f.path()); }
                }
            }
        }
    }
    let tx = c.transaction().unwrap();
    let (mut nfiles, mut ncalls) = (0, 0);
    for p in files {
        let key = p.to_string_lossy().to_string();
        let len = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        let off: u64 = tx.query_row("SELECT off FROM files WHERE path=?", [&key], |r| r.get::<_, i64>(0)).map(|v| v as u64).unwrap_or(0);
        if len <= off { continue; }
        nfiles += 1;
        let mut f = File::open(&p).unwrap();
        f.seek(SeekFrom::Start(off)).unwrap();
        let mut rd = BufReader::new(f);
        let mut pos = off;
        let mut line = String::new();
        loop {
            line.clear();
            let n = rd.read_line(&mut line).unwrap_or(0);
            if n == 0 { break; }
            if !line.ends_with('\n') { break; } // partial line, retry next scan
            pos += n as u64;
            if (line.contains("\"tool_use\"") || line.contains("\"is_error\":true")) && !line.contains("api-request-blob") { note_tools(&tx, &line); }
            if !line.contains("\"usage\"") { continue; }
            let Ok(d) = serde_json::from_str::<Value>(&line) else { continue };
            let Some(m) = d.get("message") else { continue };
            let Some(u) = m.get("usage") else { continue };
            let Some(id) = m.get("id").and_then(|v| v.as_str()) else { continue };
            let model = m.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            if model == "<synthetic>" { continue; }
            let g = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
            let (w5, w1) = match u.get("cache_creation") {
                Some(cc) if cc.is_object() => (g(cc, "ephemeral_5m_input_tokens"), g(cc, "ephemeral_1h_input_tokens")),
                _ => (g(u, "cache_creation_input_tokens"), 0),
            };
            let ts = d.get("timestamp").and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.timestamp()).unwrap_or(0);
            let cwd = d.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
            let project = project_for(cwd, &aliases);
            let sess = d.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
            // streaming repeats the same message id; keep the row with the most output tokens
            let kind = if model.starts_with("claude-") { "claude_sub" } else { "ollama_sub" };
            tx.execute("INSERT INTO calls VALUES(?,?,?,?,?,?,?,?,?,?,?,?,NULL)
                ON CONFLICT(id) DO UPDATE SET inp=excluded.inp,out=excluded.out,cread=excluded.cread,cw5=excluded.cw5,cw1=excluded.cw1
                WHERE excluded.out>=calls.out",
                params![id, ts, "claude-code", project, model, g(u,"input_tokens"), g(u,"output_tokens"), g(u,"cache_read_input_tokens"), w5, w1, sess, kind]).unwrap();
            ncalls += 1;
        }
        tx.execute("INSERT OR REPLACE INTO files VALUES(?,?)", params![key, pos as i64]).unwrap();
    }
    tx.commit().unwrap();
    (nfiles, ncalls)
}

struct Price { i: f64, o: f64, r: f64, w5: f64, w1: f64 }
fn load_prices() -> HashMap<String, Price> {
    // Built-in list first, then a user-supplied ~/Library/Application Support/aimon/prices.json adds models and overrides entries.
    let mut m = HashMap::new();
    let user = fs::read_to_string(data_dir().join("prices.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let built_in: Value = serde_json::from_str(include_str!("../prices.json")).unwrap();
    for v in std::iter::once(&built_in).chain(user.as_ref()) {
        for (k, p) in v["models"].as_object().into_iter().flatten() {
            let f = |n: &str| p[n].as_f64().unwrap_or(0.0);
            m.insert(k.clone(), Price { i: f("input"), o: f("output"), r: f("cache_read"), w5: f("cache_write_5m"), w1: f("cache_write_1h") });
        }
    }
    m
}

#[derive(Default, Clone)]
struct Agg { calls: i64, inp: i64, out: i64, cread: i64, cw: i64, cost: f64, unpriced: i64 }

fn report(c: &Connection, since: i64, group: &str) {
    let prices = load_prices();
    let aliases = load_aliases();
    let mut st = c.prepare("SELECT project, model, COUNT(*), SUM(inp), SUM(out), SUM(cread), SUM(cw5), SUM(cw1), SUM(cost_override), cost_override IS NOT NULL FROM calls WHERE ts>=? AND kind='api' GROUP BY project, model, cost_override IS NOT NULL").unwrap();
    let mut agg: HashMap<String, Agg> = HashMap::new();
    let mut total = Agg::default();
    let rows = st.query_map([since], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?, r.get::<_,i64>(4)?, r.get::<_,i64>(5)?, r.get::<_,i64>(6)?, r.get::<_,i64>(7)?, r.get::<_,Option<f64>>(8)?, r.get::<_,bool>(9)?))).unwrap();
    for r in rows.flatten() {
        let (proj, model, n, i, o, cr, w5, w1, over, has_over) = r;
        let (cost, un) = match prices.get(&model) {
            _ if has_over => (over.unwrap_or(0.0), 0),   // the call reported its own cost
            Some(p) => ((i as f64*p.i + o as f64*p.o + cr as f64*p.r + w5 as f64*p.w5 + w1 as f64*p.w1) / 1e6, 0),
            None => (0.0, i + o + cr + w5 + w1),
        };
        let key = if group == "model" { model } else { aliases.get(&proj).cloned().unwrap_or(proj) };
        for a in [agg.entry(key).or_default(), &mut total] {
            a.calls += n; a.inp += i; a.out += o; a.cread += cr; a.cw += w5 + w1; a.cost += cost; a.unpriced += un;
        }
    }
    let mut rows: Vec<_> = agg.into_iter().collect();
    rows.sort_by(|a, b| (b.1.cost, b.1.inp + b.1.out).partial_cmp(&(a.1.cost, a.1.inp + a.1.out)).unwrap());
    println!("{:<28}{:>8}{:>11}{:>11}{:>12}{:>11}{:>10}", group, "calls", "input", "output", "cache read", "cache wr", "cost");
    for (k, a) in rows.iter().chain(std::iter::once(&("TOTAL".to_string(), total.clone()))) {
        let cost = if a.unpriced > 0 && a.cost == 0.0 { "no price".to_string() } else { format!("${:.2}", a.cost) };
        println!("{:<28}{:>8}{:>11}{:>11}{:>12}{:>11}{:>10}", k, a.calls, h(a.inp), h(a.out), h(a.cread), h(a.cw), cost);
    }
    if total.unpriced > 0 { println!("\n{} tokens on models without a price (see prices.json).", h(total.unpriced)); }
}
fn h(n: i64) -> String {
    let f = n as f64;
    if f >= 1e9 { format!("{:.2}B", f/1e9) } else if f >= 1e6 { format!("{:.2}M", f/1e6) } else if f >= 1e3 { format!("{:.1}K", f/1e3) } else { n.to_string() }
}
fn day_start(days_back: i64) -> i64 {
    let d: NaiveDate = Local::now().date_naive() - Duration::days(days_back);
    Local.from_local_datetime(&d.and_hms_opt(0,0,0).unwrap()).unwrap().timestamp()
}

fn cost_of(prices: &HashMap<String, Price>, model: &str, i: i64, o: i64, cr: i64, w5: i64, w1: i64) -> Option<f64> {
    prices.get(model).map(|p| (i as f64*p.i + o as f64*p.o + cr as f64*p.r + w5 as f64*p.w5 + w1 as f64*p.w1) / 1e6)
}

/// Optional features that use a login on this Mac. All are off until the user turns them on:
/// `aimon config claude_limits on` / `aimon config ollama on`. Stored in config.json next to the database.
fn option_on(key: &str) -> bool {
    fs::read_to_string(data_dir().join("config.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map_or(false, |v| v[key].as_bool().unwrap_or(false))
}
fn config_cmd(a: &[String]) {
    let (Some(k), Some(v)) = (a.get(2), a.get(3)) else { eprintln!("usage: aimon config <claude_limits|ollama> <on|off>"); return };
    if !["claude_limits", "ollama"].contains(&k.as_str()) || !["on", "off"].contains(&v.as_str()) { eprintln!("usage: aimon config <claude_limits|ollama> <on|off>"); return; }
    let path = data_dir().join("config.json");
    let mut cfg = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(|v| v.is_object()).unwrap_or(serde_json::json!({}));
    cfg[k.as_str()] = serde_json::json!(v == "on");
    fs::write(&path, cfg.to_string()).ok();
    fs::remove_file(data_dir().join(if k == "ollama" { "ollama.json" } else { "limits.json" })).ok();
    fs::remove_file(data_dir().join("no_keychain")).ok();
    println!("{k} is {v}");
}

/// Claude subscription limits. Live from Anthropic's usage endpoint using Claude Code's own login,
/// cached for 60s; falls back to the copy Claude Code keeps in ~/.claude.json (marked stale).
fn fetch_limits() -> Value {
    use serde_json::json;
    if !option_on("claude_limits") { return json!({"source": "off"}); }
    let cache = data_dir().join("limits.json");
    let now = Local::now().timestamp();
    if let Ok(t) = fs::read_to_string(&cache) {
        if let Ok(v) = serde_json::from_str::<Value>(&t) {
            if v["fetched"].as_i64().map_or(false, |f| now - f < 60) && v["source"] == "live" { return v; }
        }
    }
    // Ask the Keychain at most once a day: if the item is missing (API-key user) or access was refused, don't re-prompt every refresh.
    let marker = data_dir().join("no_keychain");
    let skip = fs::metadata(&marker).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(false, |e| e.as_secs() < 86400);
    let live = (|| -> Option<Value> {
        if skip { return None; }
        let o = std::process::Command::new("security").args(["find-generic-password", "-s", "Claude Code-credentials", "-w"]).output().ok()?;
        if !o.status.success() { fs::write(&marker, "").ok(); return None; }
        let cred: Value = serde_json::from_slice(&o.stdout).ok()?;
        let tok = cred["claudeAiOauth"]["accessToken"].as_str()?.to_string();
        let o = std::process::Command::new("curl").args(["-s", "-m", "8", "https://api.anthropic.com/api/oauth/usage",
            "-H", &format!("Authorization: Bearer {tok}"), "-H", "anthropic-beta: oauth-2025-04-20"]).output().ok()?;
        let u: Value = serde_json::from_slice(&o.stdout).ok()?;
        if u.get("five_hour").is_none() { return None; }
        Some(u)
    })();
    let (u, source, fetched) = match live {
        Some(u) => (u, "live", now),
        None => {
            let cj = fs::read_to_string(home().join(".claude.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
            match cj.as_ref().and_then(|v| v.get("cachedUsageUtilization")) {
                Some(c) => (c["utilization"].clone(), "stale", c["fetchedAtMs"].as_i64().unwrap_or(0) / 1000),
                None => return json!({"source": "none"}),
            }
        }
    };
    let pick = |k: &str| json!({"pct": u[k]["utilization"], "resets_at": u[k]["resets_at"]});
    let out = json!({"source": source, "fetched": fetched, "session": pick("five_hour"), "weekly": pick("seven_day"),
        "weekly_sonnet": if u["seven_day_sonnet"].is_null() { Value::Null } else { pick("seven_day_sonnet") },
        "weekly_opus": if u["seven_day_opus"].is_null() { Value::Null } else { pick("seven_day_opus") }});
    if source == "live" { fs::write(&cache, out.to_string()).ok(); }
    out
}

/// Old folder names -> current project names. Editable in ~/Library/Application Support/aimon/aliases.json.
fn load_aliases() -> HashMap<String, String> {
    let mut m: HashMap<String, String> = HashMap::new();
    if let Some(v) = fs::read_to_string(data_dir().join("aliases.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        if let Some(o) = v.as_object() { for (k, x) in o { if let Some(x) = x.as_str() { m.insert(k.clone(), x.to_string()); } } }
    }
    m
}

/// Seed of an unencrypted OpenSSH ed25519 private key ("openssh-key-v1" container).
fn ed25519_seed(pem: &str) -> Option<[u8; 32]> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let raw = B64.decode(body.trim()).ok()?;
    let rest = raw.strip_prefix(b"openssh-key-v1\0")?;
    fn string<'a>(b: &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
        let n = u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as usize;
        Some((b.get(4..4 + n)?, b.get(4 + n..)?))
    }
    let (cipher, rest) = string(rest)?;
    if cipher != b"none" { return None; }           // encrypted keys are not supported
    let (_kdf, rest) = string(rest)?;
    let (_kdfopts, rest) = string(rest)?;
    let rest = rest.get(4..)?;                       // number of keys
    let (_pubkey, rest) = string(rest)?;
    let (private, _) = string(rest)?;
    let p = private.get(8..)?;                       // two check ints
    let (kind, p) = string(p)?;
    if kind != b"ssh-ed25519" { return None; }
    let (_pub, p) = string(p)?;
    let (sk, _) = string(p)?;                        // 32-byte seed + 32-byte public key
    sk.get(..32)?.try_into().ok()
}

/// Ollama cloud spend. GET ollama.com/api/usage, signed with the same ed25519 key `ollama signin` uses
/// (~/.ollama/id_ed25519), so no API key is needed. Undocumented endpoint: any failure degrades to "error".
/// The subscription session/weekly percentages live only behind the browser login (ollama.com/dashboard).
fn fetch_ollama() -> Value {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    if !option_on("ollama") { return json!({"source": "off"}); }
    let cache = data_dir().join("ollama.json");
    let now = Local::now().timestamp();
    if let Some(v) = fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        if v["fetched"].as_i64().map_or(false, |f| now - f < 90) && v["source"] == "live" { return v; }
    }
    let dir = home().join(".ollama");
    let (Ok(sk), Ok(pk)) = (fs::read_to_string(dir.join("id_ed25519")), fs::read_to_string(dir.join("id_ed25519.pub"))) else { return json!({"source": "nokey"}) };
    let res = (|| -> Option<Value> {
        let seed = ed25519_seed(&sk)?;
        let signing = SigningKey::from_bytes(&seed);
        let uri = format!("/api/usage?range=30d&ts={now}");
        let sig = signing.sign(format!("GET,{uri}").as_bytes());
        let auth = format!("{}:{}", pk.split_whitespace().nth(1)?, B64.encode(sig.to_bytes()));
        let o = std::process::Command::new("curl").args(["-s", "-m", "8", &format!("https://ollama.com{uri}"), "-H", &format!("Authorization: {auth}")]).output().ok()?;
        let d: Value = serde_json::from_slice(&o.stdout).ok()?;
        d.get("totals")?;
        Some(d)
    })();
    let Some(d) = res else { return json!({"source": "error", "fetched": now}) };
    let daily: Vec<f64> = d["buckets"].as_array().map(|a| a.iter().map(|b| b["usage_usd"].as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
    let first = d["buckets"].as_array().and_then(|a| a.first()).and_then(|b| b["from"].as_str()).unwrap_or("").to_string();
    let n = daily.len();
    let out = json!({"source": "live", "fetched": now, "first": first, "daily": daily,
        "today": daily.last().copied().unwrap_or(0.0), "week": daily.iter().skip(n.saturating_sub(7)).sum::<f64>(),
        "month": d["totals"]["usage_usd"].as_f64().unwrap_or(0.0), "requests": d["totals"]["request_count"].as_i64().unwrap_or(0)});
    fs::write(&cache, out.to_string()).ok();
    out
}

fn snapshot(c: &Connection) {
    use serde_json::json;
    let prices = load_prices();
    let aliases = load_aliases();
    let now = Local::now().timestamp();
    let today0 = day_start(0);
    let week0 = day_start(6);
    let month0 = { let d = Local::now().date_naive().with_day(1).unwrap(); Local.from_local_datetime(&d.and_hms_opt(0,0,0).unwrap()).unwrap().timestamp() };
    let since = day_start(13).min(month0);
    let mut st = c.prepare("SELECT ts, source, project, model, inp, out, cread, cw5, cw1, kind, cost_override FROM calls WHERE ts>=?").unwrap();
    let rows: Vec<(i64,String,String,String,i64,i64,i64,i64,i64,String,Option<f64>)> = st.query_map([since], |r| Ok((
        r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?))).unwrap().flatten().collect();

    let mut hourly = vec![[0i64; 3]; 24];            // tokens: claude_sub, ollama_sub, api+local
    let mut daily_tokens = vec![0i64; 14];
    let mut api_daily = vec![0f64; 14];
    let (mut api_today, mut api_week, mut api_month) = (0f64, 0f64, 0f64);
    let mut api_by: HashMap<String,(f64,i64)> = HashMap::new();
    let mut proj: HashMap<String,(i64,f64)> = HashMap::new();
    // Without a Claude plan (no login found) Claude Code is billed per token, so its usage counts as money.
    let limits = fetch_limits();
    let no_plan = limits["source"] == "none";
    let mut models: HashMap<String,(i64,String)> = HashMap::new();
    let (mut c_tok, mut o_tok, mut c_in, mut c_out, mut c_cr, mut c_cw) = (0i64, 0i64, 0i64, 0i64, 0i64, 0i64);
    let (mut c_week, mut o_week) = (0i64, 0i64);
    for (ts, source, pr, m, i, o, cr, w5, w1, kind, over) in &rows {
        let tok = i + o + cr + w5 + w1;
        let didx = if *ts >= today0 { 13 } else { 13 - ((today0 - ts + 86399) / 86400).min(13) as usize };
        daily_tokens[didx] += tok;
        let cost = over.or_else(|| cost_of(&prices, m, *i, *o, *cr, *w5, *w1)).unwrap_or(0.0);
        if kind == "api" || (no_plan && kind == "claude_sub") {
            api_daily[didx] += cost;
            if *ts >= today0 { api_today += cost; }
            if *ts >= week0 { api_week += cost; }
            if *ts >= month0 { api_month += cost; }
            if *ts >= month0 { let e = api_by.entry(if source == "log" { pr.clone() } else { source.clone() }).or_insert((0.0, 0)); e.0 += cost; e.1 += tok; }
        }
        if *ts >= week0 {
            if source == "claude-code" || source == "log" { let e = proj.entry(aliases.get(pr).cloned().unwrap_or_else(|| pr.clone())).or_insert((0, 0.0)); e.0 += i + o; e.1 += cost; }
            models.entry(format!("{}|{}", m, kind)).or_insert((0, kind.clone())).0 += tok;
            match kind.as_str() { "claude_sub" => c_week += tok, "ollama_sub" => o_week += tok, _ => {} }
        }
        if now - ts < 86400 {
            let h = (23 - ((now - ts) / 3600).min(23)) as usize;
            let k = match kind.as_str() { "claude_sub" => 0, "ollama_sub" => 1, _ => 2 };
            hourly[h][k] += tok;
        }
        if *ts >= today0 && kind == "claude_sub" { c_tok += tok; c_in += i; c_out += o; c_cr += cr; c_cw += w5 + w1; }
        if *ts >= today0 && kind == "ollama_sub" { o_tok += tok; }
    }
    let mut pv: Vec<_> = proj.into_iter().collect();
    pv.sort_by(|a,b| b.1.0.cmp(&a.1.0));
    let mut mv: Vec<_> = models.into_iter().collect();
    mv.sort_by(|a,b| b.1.0.cmp(&a.1.0));
    let mut av: Vec<_> = api_by.into_iter().collect();
    av.sort_by(|a,b| b.1.0.partial_cmp(&a.1.0).unwrap());
    let denom = (c_cr + c_in + c_cw).max(1) as f64;
    let out = json!({
        "generated": now,
        "limits": limits,
        "ollama_limits": fetch_ollama(),
        "api": {"today": api_today, "week": api_week, "month": api_month,
                "daily": api_daily, "by_app": av.iter().map(|(k,v)| json!({"name": k, "cost": v.0, "tokens": v.1})).collect::<Vec<_>>()},
        "claude": {"today_tokens": c_tok, "week_tokens": c_week, "output": c_out, "cache_read": c_cr, "cache_hit": c_cr as f64 / denom},
        "ollama": {"today_tokens": o_tok, "week_tokens": o_week},
        "hourly": hourly.iter().map(|h| json!({"claude": h[0], "ollama": h[1], "other": h[2]})).collect::<Vec<_>>(),
        "daily_tokens": daily_tokens,
        "projects": pv.iter().take(6).map(|(k,v)| json!({"name": k, "tokens": v.0, "cost": v.1})).collect::<Vec<_>>(),
        "models": mv.iter().take(6).map(|(k,v)| json!({"name": k.split('|').next().unwrap(), "tokens": v.0, "kind": v.1})).collect::<Vec<_>>(),
    });
    println!("{}", out);
}

/// API spend for one calendar month. `back` = 0 current month, 1 last month, ...
fn month_report(c: &Connection, back: i64) {
    use serde_json::json;
    let prices = load_prices();
    let first = { let t = Local::now().date_naive().with_day(1).unwrap(); let mut m = t.year() * 12 + t.month0() as i32 - back as i32; if m < 0 { m = 0 }; NaiveDate::from_ymd_opt(m / 12, (m % 12) as u32 + 1, 1).unwrap() };
    let next = if first.month() == 12 { NaiveDate::from_ymd_opt(first.year() + 1, 1, 1).unwrap() } else { NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1).unwrap() };
    let ts_of = |d: NaiveDate| Local.from_local_datetime(&d.and_hms_opt(0,0,0).unwrap()).unwrap().timestamp();
    let (t0, t1) = (ts_of(first), ts_of(next));
    let ndays = (next - first).num_days() as usize;
    let mut daily = vec![0f64; ndays];
    let mut by: HashMap<String, (f64, i64)> = HashMap::new();
    let mut st = c.prepare("SELECT ts, source, model, inp, out, cread, cw5, cw1, cost_override FROM calls WHERE kind='api' AND ts>=? AND ts<?").unwrap();
    let rows = st.query_map([t0, t1], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,i64>(3)?, r.get::<_,i64>(4)?, r.get::<_,i64>(5)?, r.get::<_,i64>(6)?, r.get::<_,i64>(7)?, r.get::<_,Option<f64>>(8)?))).unwrap();
    let mut total = 0.0;
    for (ts, src, m, i, o, cr, w5, w1, over) in rows.flatten() {
        let cost = over.or_else(|| cost_of(&prices, &m, i, o, cr, w5, w1)).unwrap_or(0.0);
        let di = (((ts - t0) / 86400) as usize).min(ndays - 1);
        daily[di] += cost; total += cost;
        let e = by.entry(src).or_insert((0.0, 0)); e.0 += cost; e.1 += i + o + cr + w5 + w1;
    }
    let mut av: Vec<_> = by.into_iter().collect();
    av.sort_by(|a, b| b.1.0.partial_cmp(&a.1.0).unwrap());
    let has_prev = c.prepare("SELECT 1 FROM calls WHERE kind='api' AND ts<? LIMIT 1").unwrap().exists([t0]).unwrap();
    println!("{}", json!({"label": first.format("%B %Y").to_string(), "first": first.format("%Y-%m-%d").to_string(), "total": total, "daily": daily, "has_prev": has_prev,
        "by_app": av.iter().map(|(k,v)| json!({"name": k, "cost": v.0, "tokens": v.1})).collect::<Vec<_>>()}));
}

/// Tool calls and their failures, keyed by tool_use id. Errors arrive later as a tool_result with is_error=true.
fn note_tools(tx: &rusqlite::Transaction, line: &str) {
    let Ok(d) = serde_json::from_str::<Value>(line) else { return };
    let Some(items) = d.pointer("/message/content").and_then(|v| v.as_array()) else { return };
    let ts = d.get("timestamp").and_then(|v| v.as_str()).and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.timestamp()).unwrap_or(0);
    let cwd = d.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
    let project = Path::new(cwd).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "?".into());
    let sess = d.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    for b in items {
        match b.get("type").and_then(|v| v.as_str()) {
            Some("tool_use") if d.get("type").and_then(|v| v.as_str()) == Some("assistant") => {
                let (Some(id), Some(name)) = (b.get("id").and_then(|v| v.as_str()), b.get("name").and_then(|v| v.as_str())) else { continue };
                // mcp__server__tool -> server:tool
                let tool = match name.strip_prefix("mcp__") { Some(r) => r.replacen("__", ":", 1), None => name.to_string() };
                let arg = b.get("input").map(tool_arg).unwrap_or_default();
                tx.execute("INSERT INTO tools(id, ts, sess, project, tool, arg) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET arg=excluded.arg", params![id, ts, sess, project, tool, arg]).unwrap();
            }
            Some("tool_result") if b.get("is_error").and_then(|v| v.as_bool()) == Some(true) => {
                if let Some(id) = b.get("tool_use_id").and_then(|v| v.as_str()) {
                    let text = match b.get("content") {
                        Some(Value::String(t)) => t.clone(),
                        Some(Value::Array(a)) => a.iter().filter_map(|x| x.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join(" "),
                        _ => String::new(),
                    };
                    let mut ls = text.lines().map(str::trim).filter(|l| !l.is_empty());
                    let first = ls.next().unwrap_or("");
                    // "Exit code 1" alone says nothing; add the first line of output after it
                    let joined = if first.starts_with("Exit code") { match ls.next() { Some(n) => format!("{first}: {n}"), None => first.to_string() } } else { first.to_string() };
                    let msg: String = joined.chars().take(160).collect();
                    tx.execute("UPDATE tools SET err=1, msg=? WHERE id=?", params![msg, id]).unwrap();
                }
            }
            _ => {}
        }
    }
}

/// One short line saying what a tool was asked to do: the command, file, query...
fn tool_arg(input: &Value) -> String {
    let pick = ["command", "file_path", "path", "query", "url", "pattern", "prompt", "description"].iter().find_map(|k| input.get(*k).and_then(|v| v.as_str()))
        .or_else(|| input.as_object().and_then(|o| o.values().find_map(|v| v.as_str())));
    pick.unwrap_or("").lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(140).collect()
}
fn fail_kind(msg: &str) -> &'static str {
    let m = msg.to_lowercase();
    if m.starts_with("exit code") { "exit" }
    else if m.contains("classifier") || m.contains("denied") || m.contains("rejected") || m.contains("doesn't want") || m.contains("permission") { "blocked" }
    else { "error" }
}

/// Recent Claude Code sessions with their tool use and failures.
fn trace_report(c: &Connection) {
    use serde_json::json;
    let aliases = load_aliases();
    let now = Local::now().timestamp();
    let mut st = c.prepare("SELECT session, MIN(project), MIN(ts), MAX(ts), SUM(inp+out+cread+cw5+cw1), COUNT(*) FROM calls
        WHERE source='claude-code' AND session!='' AND ts>=? GROUP BY session ORDER BY MAX(ts) DESC LIMIT 12").unwrap();
    let rows: Vec<(String,String,i64,i64,i64,i64)> = st.query_map([now - 7*86400], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap().flatten().collect();
    let mut sessions = vec![];
    for (sess, proj, t0, t1, tok, calls) in rows {
        let model: String = c.query_row("SELECT model FROM calls WHERE session=? GROUP BY model ORDER BY SUM(out) DESC LIMIT 1", [&sess], |r| r.get(0)).unwrap_or_default();
        let (nt, ne): (i64, i64) = c.query_row("SELECT COUNT(*), COALESCE(SUM(err),0) FROM tools WHERE sess=?", [&sess], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((0, 0));
        let mut ts = c.prepare("SELECT tool, COUNT(*), SUM(err) FROM tools WHERE sess=? GROUP BY tool ORDER BY COUNT(*) DESC LIMIT 5").unwrap();
        let top: Vec<Value> = ts.query_map([&sess], |r| Ok(json!({"name": r.get::<_,String>(0)?, "n": r.get::<_,i64>(1)?, "err": r.get::<_,i64>(2)?}))).unwrap().flatten().collect();
        let mut fs_ = c.prepare("SELECT tool, COALESCE(arg,''), COALESCE(msg,'') FROM tools WHERE sess=? AND err=1 ORDER BY ts DESC LIMIT 8").unwrap();
        let fails: Vec<Value> = fs_.query_map([&sess], |r| { let m: String = r.get(2)?; Ok(json!({"tool": r.get::<_,String>(0)?, "arg": r.get::<_,String>(1)?, "kind": fail_kind(&m), "msg": m})) }).unwrap().flatten().collect();
        sessions.push(json!({"id": sess, "fails": fails, "project": aliases.get(&proj).cloned().unwrap_or(proj), "model": model, "start": t0, "end": t1,
            "tokens": tok, "calls": calls, "tools": nt, "errors": ne, "top": top}));
    }
    let mut tt = c.prepare("SELECT tool, COUNT(*), SUM(err) FROM tools WHERE ts>=? GROUP BY tool ORDER BY COUNT(*) DESC LIMIT 6").unwrap();
    let top24: Vec<Value> = tt.query_map([now - 86400], |r| Ok(json!({"name": r.get::<_,String>(0)?, "n": r.get::<_,i64>(1)?, "err": r.get::<_,i64>(2)?}))).unwrap().flatten().collect();
    let (n24, e24): (i64, i64) = c.query_row("SELECT COUNT(*), COALESCE(SUM(err),0) FROM tools WHERE ts>=?", [now - 86400], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((0, 0));
    let mut fr = c.prepare("SELECT tool, COALESCE(arg,''), COALESCE(msg,''), ts, project FROM tools WHERE err=1 AND ts>=? ORDER BY ts DESC").unwrap();
    let all: Vec<(String,String,String,i64,String)> = fr.query_map([now - 86400], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap().flatten().collect();
    let mut kinds: HashMap<&str, i64> = HashMap::new();
    for f in &all { *kinds.entry(fail_kind(&f.2)).or_insert(0) += 1; }
    let recent: Vec<Value> = all.iter().take(8).map(|f| json!({"tool": f.0, "arg": f.1, "msg": f.2, "kind": fail_kind(&f.2), "ts": f.3, "project": aliases.get(&f.4).cloned().unwrap_or(f.4.clone())})).collect();
    println!("{}", json!({"sessions": sessions, "tools_24h": n24, "errors_24h": e24, "top_24h": top24, "kinds_24h": kinds, "recent_fails": recent}));
}

fn proc_name(raw: &str) -> String {
    // nettop prints "name.pid"; Claude Code's binary is named after its version, e.g. 2.1.319
    let base = raw.rsplit_once('.').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit())).map(|(n, _)| n).unwrap_or(raw);
    if base.chars().all(|c| c.is_ascii_digit() || c == '.') && base.contains('.') { "claude".into() } else { base.to_string() }
}
fn host_label(ip: &str) -> Option<&'static str> {
    if ip.starts_with("2607:6bc0:") || ip.starts_with("160.79.104.") || ip.starts_with("160.79.105.") { Some("api.anthropic.com") } else { None }
}
/// Host belongs to a well-known provider or CDN. This means "recognised", not "safe": a CDN hosts anyone's content.
fn is_known(host: &str) -> bool {
    ["anthropic.com", "claude.ai", "datadoghq.com", "npmjs.org", "discord.gg", "discord.com", "ollama.com", "apple.com", "icloud.com", "apple-dns.net", "google.com", "1e100.net", "googleusercontent.com", "gstatic.com",
     "cloudflare.com", "akamaitechnologies.com", "akamai.net", "amazonaws.com", "cloudfront.net", "yahoo.com", "github.com", "tailscale.com"]
        .iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}
fn is_private(ip: &str) -> bool {
    ip.starts_with("127.") || ip == "::1" || ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("169.254.") || ip.starts_with("fe80:") || ip.starts_with("fd") || ip.starts_with("100.")
}

/// "python · my-app": what a process is, from the last folder of its working directory. Cached per pid.
fn pid_label(c: &Connection, pid: &str, base: &str) -> String {
    if !(base == "claude" || base == "node" || base == "ollama" || base.starts_with("python")) { return base.to_string(); }
    if let Ok(l) = c.query_row("SELECT label FROM pidlabel WHERE pid=? AND base=?", params![pid, base], |r| r.get::<_, String>(0)) { return l; }
    let cwd = std::process::Command::new("lsof").args(["-a", "-p", pid, "-d", "cwd", "-Fn"]).output().ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().find_map(|l| l.strip_prefix('n').map(String::from))).unwrap_or_default();
    let label = match Path::new(&cwd).file_name() { Some(n) if !cwd.is_empty() && cwd != "/" => format!("{base} · {}", n.to_string_lossy()), _ => base.to_string() };
    c.execute("INSERT OR REPLACE INTO pidlabel VALUES(?,?,?)", params![pid, base, label]).ok();
    label
}

/// Outbound traffic per process. Bytes come from nettop's cumulative counters (diffed between runs);
/// hosts come from lsof, resolved by reverse DNS (cached). IP-level only: macOS does not expose domain names without root.
fn net_report(c: &Connection) {
    use serde_json::json;
    let now = Local::now().timestamp();
    c.execute("DELETE FROM pidlabel WHERE rowid IN (SELECT rowid FROM pidlabel LIMIT -1 OFFSET 400)", []).ok();
    // 1) bytes
    if let Ok(o) = std::process::Command::new("nettop").args(["-P", "-L", "1", "-m", "tcp", "-J", "bytes_in,bytes_out"]).output() {
        for l in String::from_utf8_lossy(&o.stdout).lines().skip(1) {
            let f: Vec<&str> = l.split(',').collect();
            if f.len() < 3 { continue; }
            let (Ok(bi), Ok(bo)) = (f[1].parse::<i64>(), f[2].parse::<i64>()) else { continue };
            let prev: Option<(i64,i64)> = c.query_row("SELECT bin, bout FROM netprev WHERE k=?", [f[0]], |r| Ok((r.get(0)?, r.get(1)?))).ok();
            let (di, dout) = match prev { Some((pi, po)) if bi >= pi && bo >= po => (bi - pi, bo - po), Some(_) => (bi, bo), None => (0, 0) };
            if di + dout > 0 {
                let pid = f[0].rsplit_once('.').map(|x| x.1).unwrap_or("0");
                c.execute("INSERT INTO net VALUES(?,?,?,?)", params![now, pid_label(c, pid, &proc_name(f[0])), di, dout]).unwrap();
            }
            c.execute("INSERT OR REPLACE INTO netprev VALUES(?,?,?)", params![f[0], bi, bo]).unwrap();
        }
    }
    c.execute("DELETE FROM net WHERE ts<?", [now - 8*86400]).ok();
    // 2) live connections: (label, pid, ip, port)
    let mut conns: Vec<(String, String, String, String)> = vec![];
    if let Ok(o) = std::process::Command::new("lsof").args(["-iTCP", "-sTCP:ESTABLISHED", "-nP", "-Fpcn"]).output() {
        let (mut pid, mut base) = (String::new(), String::new());
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some(x) = l.strip_prefix('p') { pid = x.to_string(); }
            else if let Some(x) = l.strip_prefix('c') { base = proc_name(&format!("{x}.0")); }
            else if let Some(n) = l.strip_prefix('n') {
                let Some((_, remote)) = n.split_once("->") else { continue };
                let Some((ip, port)) = remote.rsplit_once(':') else { continue };
                let ip = ip.trim_matches(|c| c == '[' || c == ']');
                if is_private(ip) { continue; }
                conns.push((pid_label(c, &pid, &base), pid.clone(), ip.to_string(), port.to_string()));
            }
        }
    }
    // process uptimes, one ps call
    let mut up: HashMap<String, String> = HashMap::new();
    if let Ok(o) = std::process::Command::new("ps").args(["-Ao", "pid=,etime="]).output() {
        for l in String::from_utf8_lossy(&o.stdout).lines() { let mut it = l.split_whitespace(); if let (Some(p), Some(e)) = (it.next(), it.next()) { up.insert(p.to_string(), e.to_string()); } }
    }
    // forward-resolve hostnames of services these tools are known to use, so their IPs get a real name (refreshed every 6h)
    for h in ["api.anthropic.com", "claude.ai", "platform.claude.com", "mcp-proxy.anthropic.com", "downloads.claude.ai", "http-intake.logs.us5.datadoghq.com",
              "ollama.com", "registry.npmjs.org", "github.com", "api.github.com", "gateway.discord.gg", "discord.com"] {
        let fresh = c.query_row("SELECT ts FROM fwd WHERE host=?", [h], |r| r.get::<_, i64>(0)).map_or(false, |t| now - t < 6*3600);
        if fresh { continue; }
        let mut ips = vec![];
        for rt in ["A", "AAAA"] {
            if let Ok(o) = std::process::Command::new("dig").args(["+short", "+time=1", "+tries=1", rt, h]).output() {
                ips.extend(String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.chars().next().map_or(false, |c| c.is_ascii_hexdigit()) && l.contains(|c| c == '.' || c == ':')).map(String::from));
            }
        }
        if !ips.is_empty() { c.execute("INSERT OR REPLACE INTO fwd VALUES(?,?,?)", params![h, format!(" {} ", ips.join(" ")), now]).ok(); }
    }
    // reverse DNS, a few new lookups per run, cached
    let mut budget = 6;
    let mut name_of = |ip: &str| -> String {
        if let Some(h) = host_label(ip) { return h.to_string(); }
        if let Ok(h) = c.query_row("SELECT host FROM fwd WHERE ips LIKE '% ' || ? || ' %' LIMIT 1", [ip], |r| r.get::<_, String>(0)) { return h; }
        if let Ok(n) = c.query_row("SELECT name FROM rdns WHERE ip=?", [ip], |r| r.get::<_, String>(0)) { return if n.is_empty() { ip.to_string() } else { n }; }
        if budget == 0 { return ip.to_string(); }
        budget -= 1;
        let n = std::process::Command::new("host").args(["-W", "1", ip]).output().ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).split("domain name pointer ").nth(1).map(|s| s.trim().trim_end_matches('.').to_string()))
            .unwrap_or_default();
        c.execute("INSERT OR REPLACE INTO rdns VALUES(?,?)", params![ip, n]).ok();
        if n.is_empty() { ip.to_string() } else { n }
    };
    let mut st = c.prepare("SELECT proc, SUM(din), SUM(dout) FROM net WHERE ts>=? GROUP BY proc").unwrap();
    let rows: Vec<(String,i64,i64)> = st.query_map([now - 86400], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().flatten().collect();
    let mut names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    for k in conns.iter().map(|x| &x.0) { if !names.contains(k) { names.push(k.clone()); } }
    let is_ai = |n: &str| ["claude", "ollama"].iter().any(|a| n.starts_with(a)) || n.starts_with("python") || n.starts_with("node") || n.starts_with("aimon");
    let mut procs: Vec<Value> = names.iter().map(|n| {
        let (bi, bo) = rows.iter().find(|r| &r.0 == n).map(|r| (r.1, r.2)).unwrap_or((0, 0));
        let mine: Vec<&(String,String,String,String)> = conns.iter().filter(|x| &x.0 == n).collect();
        let mut hs: Vec<(String, i64)> = vec![];
        for x in &mine { let h = name_of(&x.2); match hs.iter_mut().find(|e| e.0 == h) { Some(e) => e.1 += 1, None => hs.push((h, 1)) } }
        hs.sort_by(|a, b| b.1.cmp(&a.1));
        let detail: Vec<Value> = mine.iter().take(12).map(|x| json!({"host": name_of(&x.2), "ip": x.2, "port": x.3, "known": is_known(&name_of(&x.2)), "pid": x.1, "up": up.get(&x.1).cloned().unwrap_or_default()})).collect();
        json!({"name": n, "in": bi, "out": bo, "ai": is_ai(n), "conns": mine.len(), "unknown": mine.iter().filter(|x| !is_known(&name_of(&x.2))).count(), "hosts": hs.iter().take(4).map(|h| h.0.clone()).collect::<Vec<_>>(), "detail": detail})
    }).filter(|p| p["in"].as_i64().unwrap_or(0) + p["out"].as_i64().unwrap_or(0) > 0 || p["conns"].as_i64().unwrap_or(0) > 0).collect();
    procs.sort_by_key(|p| (!p["ai"].as_bool().unwrap_or(false), -(p["in"].as_i64().unwrap_or(0) + p["out"].as_i64().unwrap_or(0))));
    println!("{}", json!({"procs": procs.into_iter().take(12).collect::<Vec<_>>(), "since": now - 86400}));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("config") { config_cmd(&a); return; }
    let mut c = open_db();
    let (nf, nc) = scan(&mut c);
    scan_inbox(&c);
    eprintln!("scanned {} files, {} usage rows", nf, nc);
    let days: i64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    match a.get(1).map(String::as_str) {
        Some("today") | None => report(&c, day_start(0), "project"),
        Some("by-project") => report(&c, day_start(days - 1), "project"),
        Some("by-model") => report(&c, day_start(days - 1), "model"),
        Some("scan") => {}
        Some("log") => log_cmd(&a),
        Some("snapshot") => snapshot(&c),
        Some("trace") => trace_report(&c),
        Some("net") => net_report(&c),
        Some("month") => month_report(&c, a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0)),
        _ => eprintln!("usage: aimon [today|by-project N|by-model N|scan]"),
    }
}
