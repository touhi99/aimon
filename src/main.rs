mod pace;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

type CallRow = (
    i64,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    String,
    Option<f64>,
);

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME is set for a login user"))
}
fn data_dir() -> PathBuf {
    let d = home().join("Library/Application Support/aimon");
    fs::create_dir_all(&d).ok();
    d
}

fn open_db() -> Connection {
    let c = Connection::open(data_dir().join("aimon.db")).expect("open aimon.db");
    let ver: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);
    if ver < 2 {
        c.execute_batch(
            "DROP TABLE IF EXISTS calls; DROP TABLE IF EXISTS files; PRAGMA user_version=2;",
        )
        .expect("write to database");
    }
    if ver < 3 {
        c.execute_batch("DROP TABLE IF EXISTS files; PRAGMA user_version=3;")
            .expect("write to database");
    }
    if ver < 4 {
        c.execute_batch("DROP TABLE IF EXISTS files; PRAGMA user_version=4;")
            .expect("write to database");
    }
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
        CREATE TABLE IF NOT EXISTS uses(ts INTEGER, key TEXT, via TEXT);
        CREATE INDEX IF NOT EXISTS uses_key ON uses(key, ts);
        CREATE TABLE IF NOT EXISTS spent(day TEXT, key TEXT, secs INTEGER, PRIMARY KEY(day, key));
    ").expect("write to database");
    c.execute("ALTER TABLE tools ADD COLUMN arg TEXT", []).ok();
    c.execute("ALTER TABLE tools ADD COLUMN msg TEXT", []).ok();
    c
}

fn scan_inbox(c: &Connection) {
    let dir = data_dir().join("inbox");
    fs::create_dir_all(&dir).ok();
    let Ok(rd) = fs::read_dir(&dir) else { return };
    let tx = c.unchecked_transaction().expect("begin transaction");
    for f in rd.flatten() {
        let path = f.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let Ok(txt) = fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in txt.lines().enumerate() {
            let Ok(d) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let g = |k: &str| d.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0);
            let ts = d
                .get("ts")
                .and_then(|v| {
                    v.as_i64().or_else(|| {
                        v.as_str()
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|t| t.timestamp())
                    })
                })
                .unwrap_or_else(|| {
                    f.metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
                });
            let project = d
                .get("project")
                .and_then(|v| v.as_str())
                .unwrap_or("unassigned");
            let model = d.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = if d
                .get("local")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                "local"
            } else {
                "api"
            };
            tx.execute(
                "INSERT OR REPLACE INTO calls VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    format!("inbox:{name}:{n}"),
                    ts,
                    "log",
                    project,
                    model,
                    g("in"),
                    g("out"),
                    g("cache_read"),
                    g("cache_write"),
                    0,
                    "",
                    kind,
                    d.get("cost").and_then(serde_json::Value::as_f64)
                ],
            )
            .expect("write to database");
        }
    }
    tx.commit().expect("commit transaction");
}

fn log_cmd(a: &[String]) {
    use std::io::Write;
    let arg = |k: &str| {
        a.iter()
            .position(|x| x == k)
            .and_then(|i| a.get(i + 1))
            .cloned()
    };
    let (Some(project), Some(model)) = (arg("--project"), arg("--model")) else {
        eprintln!(
            "usage: aimon log --project NAME --model MODEL --in N --out N [--cost USD] [--local]"
        );
        return;
    };
    let num = |k: &str| arg(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let mut line = serde_json::json!({"ts": Local::now().timestamp(), "project": project, "model": model, "in": num("--in"), "out": num("--out"), "cache_read": num("--cache-read"), "cache_write": num("--cache-write")});
    if let Some(c) = arg("--cost").and_then(|v| v.parse::<f64>().ok()) {
        line["cost"] = c.into();
    }
    if a.iter().any(|x| x == "--local") {
        line["local"] = true.into();
    }
    let dir = data_dir().join("inbox");
    fs::create_dir_all(&dir).ok();
    let path = dir.join("manual.jsonl");
    let written = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = written {
        eprintln!("cannot write {}: {e}", path.display());
        std::process::exit(1);
    }
    eprintln!("logged to {}", dir.join("manual.jsonl").display());
}

fn project_for(cwd: &str, aliases: &HashMap<String, String>) -> String {
    for (k, name) in aliases {
        let base = match k.strip_prefix("~/") {
            Some(r) => home().join(r),
            None if k.starts_with('/') => PathBuf::from(k),
            None => continue,
        };
        if !cwd.is_empty() && Path::new(cwd).starts_with(&base) {
            return name.clone();
        }
    }
    repo_name(cwd)
}

fn repo_name(cwd: &str) -> String {
    let last = |p: &Path| {
        p.file_name()
            .map_or_else(|| "?".into(), |s| s.to_string_lossy().to_string())
    };
    if cwd.is_empty() {
        return "?".into();
    }
    if let Some(root) = Path::new(cwd).ancestors().find(|a| a.join(".git").exists()) {
        return last(root);
    }
    let holds_repos = fs::read_dir(cwd)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.path().join(".git").exists());
    if holds_repos {
        format!("{} (workspace)", last(Path::new(cwd)))
    } else {
        last(Path::new(cwd))
    }
}

fn scan(c: &mut Connection) -> (usize, usize) {
    let aliases = load_aliases();
    let root = home().join(".claude/projects");
    let mut files = vec![];
    if let Ok(dirs) = fs::read_dir(&root) {
        for d in dirs.flatten() {
            if let Ok(fs_) = fs::read_dir(d.path()) {
                for f in fs_.flatten() {
                    if f.path().extension().is_some_and(|e| e == "jsonl") {
                        files.push(f.path());
                    }
                }
            }
        }
    }
    let tx = c.transaction().expect("begin transaction");
    let (mut nfiles, mut ncalls) = (0, 0);
    for p in files {
        let key = p.to_string_lossy().to_string();
        let len = fs::metadata(&p).map_or(0, |m| m.len());
        let off: u64 = tx
            .query_row("SELECT off FROM files WHERE path=?", [&key], |r| {
                r.get::<_, i64>(0)
            })
            .map_or(0, |v| u64::try_from(v).unwrap_or(0));
        if len <= off {
            continue;
        }
        let Ok(mut f) = File::open(&p) else { continue };
        if f.seek(SeekFrom::Start(off)).is_err() {
            continue;
        }
        nfiles += 1;
        let mut rd = BufReader::new(f);
        let mut pos = off;
        let mut line = String::new();
        loop {
            line.clear();
            let n = rd.read_line(&mut line).unwrap_or(0);
            if n == 0 {
                break;
            }
            if !line.ends_with('\n') {
                break;
            }
            pos += n as u64;
            if (line.contains("\"tool_use\"") || line.contains("\"is_error\":true"))
                && !line.contains("api-request-blob")
            {
                note_tools(&tx, &line);
            }
            if !line.contains("\"usage\"") {
                continue;
            }
            let Ok(d) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(m) = d.get("message") else { continue };
            let Some(u) = m.get("usage") else { continue };
            let Some(id) = m.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let model = m.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            if model == "<synthetic>" {
                continue;
            }
            let g = |v: &Value, k: &str| v.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0);
            let (w5, w1) = match u.get("cache_creation") {
                Some(cc) if cc.is_object() => (
                    g(cc, "ephemeral_5m_input_tokens"),
                    g(cc, "ephemeral_1h_input_tokens"),
                ),
                _ => (g(u, "cache_creation_input_tokens"), 0),
            };
            let ts = d
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(0, |t| t.timestamp());
            let cwd = d.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
            let project = project_for(cwd, &aliases);
            let sess = d.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
            let kind = if model.starts_with("claude-") {
                "claude_sub"
            } else {
                "ollama_sub"
            };
            tx.execute("INSERT INTO calls VALUES(?,?,?,?,?,?,?,?,?,?,?,?,NULL)
                ON CONFLICT(id) DO UPDATE SET project=excluded.project,inp=excluded.inp,out=excluded.out,cread=excluded.cread,cw5=excluded.cw5,cw1=excluded.cw1
                WHERE excluded.out>=calls.out",
                params![id, ts, "claude-code", project, model, g(u,"input_tokens"), g(u,"output_tokens"), g(u,"cache_read_input_tokens"), w5, w1, sess, kind]).expect("write to database");
            ncalls += 1;
        }
        tx.execute(
            "INSERT OR REPLACE INTO files VALUES(?,?)",
            params![key, i64::try_from(pos).unwrap_or(i64::MAX)],
        )
        .expect("write to database");
    }
    tx.commit().expect("commit transaction");
    (nfiles, ncalls)
}

struct Price {
    i: f64,
    o: f64,
    r: f64,
    w5: f64,
    w1: f64,
}
fn load_prices() -> HashMap<String, Price> {
    let mut m = HashMap::new();
    let user = fs::read_to_string(data_dir().join("prices.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let built_in: Value = serde_json::from_str(include_str!("../prices.json"))
        .expect("bundled prices.json is valid JSON");
    for v in std::iter::once(&built_in).chain(user.as_ref()) {
        for (k, p) in v["models"].as_object().into_iter().flatten() {
            let f = |n: &str| p[n].as_f64().unwrap_or(0.0);
            m.insert(
                k.clone(),
                Price {
                    i: f("input"),
                    o: f("output"),
                    r: f("cache_read"),
                    w5: f("cache_write_5m"),
                    w1: f("cache_write_1h"),
                },
            );
        }
    }
    m
}

#[derive(Default, Clone)]
struct Agg {
    calls: i64,
    inp: i64,
    out: i64,
    cread: i64,
    cw: i64,
    cost: f64,
    unpriced: i64,
}

fn report(c: &Connection, since: i64, group: &str) {
    let prices = load_prices();
    let aliases = load_aliases();
    let mut st = c.prepare("SELECT project, model, COUNT(*), SUM(inp), SUM(out), SUM(cread), SUM(cw5), SUM(cw1), SUM(cost_override), cost_override IS NOT NULL FROM calls WHERE ts>=? AND kind='api' GROUP BY project, model, cost_override IS NOT NULL").expect("prepare query");
    let mut agg: HashMap<String, Agg> = HashMap::new();
    let mut total = Agg::default();
    let rows = st
        .query_map([since], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<f64>>(8)?,
                r.get::<_, bool>(9)?,
            ))
        })
        .expect("run query");
    for r in rows.flatten() {
        let (proj, model, n, i, o, cr, w5, w1, over, has_over) = r;
        let (cost, un) = match prices.get(&model) {
            _ if has_over => (over.unwrap_or(0.0), 0),
            Some(p) => (
                (i as f64 * p.i
                    + o as f64 * p.o
                    + cr as f64 * p.r
                    + w5 as f64 * p.w5
                    + w1 as f64 * p.w1)
                    / 1e6,
                0,
            ),
            None => (0.0, i + o + cr + w5 + w1),
        };
        let key = if group == "model" {
            model
        } else {
            aliases.get(&proj).cloned().unwrap_or(proj)
        };
        for a in [agg.entry(key).or_default(), &mut total] {
            a.calls += n;
            a.inp += i;
            a.out += o;
            a.cread += cr;
            a.cw += w5 + w1;
            a.cost += cost;
            a.unpriced += un;
        }
    }
    let mut rows: Vec<_> = agg.into_iter().collect();
    rows.sort_by(|a, b| {
        b.1.cost
            .total_cmp(&a.1.cost)
            .then((b.1.inp + b.1.out).cmp(&(a.1.inp + a.1.out)))
            .then_with(|| a.0.cmp(&b.0))
    });
    println!(
        "{:<28}{:>8}{:>11}{:>11}{:>12}{:>11}{:>10}",
        group, "calls", "input", "output", "cache read", "cache wr", "cost"
    );
    for (k, a) in rows
        .iter()
        .chain(std::iter::once(&("TOTAL".to_string(), total.clone())))
    {
        let cost = if a.unpriced > 0 && a.cost == 0.0 {
            "no price".to_string()
        } else {
            format!("${:.2}", a.cost)
        };
        println!(
            "{:<28}{:>8}{:>11}{:>11}{:>12}{:>11}{:>10}",
            k,
            a.calls,
            h(a.inp),
            h(a.out),
            h(a.cread),
            h(a.cw),
            cost
        );
    }
    if total.unpriced > 0 {
        println!(
            "\n{} tokens on models without a price (see prices.json).",
            h(total.unpriced)
        );
    }
}
fn h(n: i64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.2}B", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.2}M", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.1}K", f / 1e3)
    } else {
        n.to_string()
    }
}
fn day_start(days_back: i64) -> i64 {
    let d: NaiveDate = Local::now().date_naive() - Duration::days(days_back);
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight is a valid time"))
        .earliest()
        .expect("local midnight exists")
        .timestamp()
}

fn cost_of(
    prices: &HashMap<String, Price>,
    model: &str,
    i: i64,
    o: i64,
    cr: i64,
    w5: i64,
    w1: i64,
) -> Option<f64> {
    prices.get(model).map(|p| {
        (i as f64 * p.i + o as f64 * p.o + cr as f64 * p.r + w5 as f64 * p.w5 + w1 as f64 * p.w1)
            / 1e6
    })
}

fn option_on(key: &str) -> bool {
    fs::read_to_string(data_dir().join("config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v[key].as_bool().unwrap_or(false))
}
fn config_cmd(a: &[String]) {
    let (Some(k), Some(v)) = (a.get(2), a.get(3)) else {
        eprintln!("usage: aimon config <claude_limits|ollama> <on|off>");
        return;
    };
    if !["claude_limits", "ollama"].contains(&k.as_str()) || !["on", "off"].contains(&v.as_str()) {
        eprintln!("usage: aimon config <claude_limits|ollama> <on|off>");
        return;
    }
    let path = data_dir().join("config.json");
    let mut cfg = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or(serde_json::json!({}));
    cfg[k.as_str()] = serde_json::json!(v == "on");
    fs::write(&path, cfg.to_string()).ok();
    fs::remove_file(data_dir().join(if k == "ollama" {
        "ollama.json"
    } else {
        "limits.json"
    }))
    .ok();
    fs::remove_file(data_dir().join("no_keychain")).ok();
    println!("{k} is {v}");
}

fn fetch_limits() -> Value {
    use serde_json::json;
    if !option_on("claude_limits") {
        return json!({"source": "off"});
    }
    let cache = data_dir().join("limits.json");
    let now = Local::now().timestamp();
    let last: Option<Value> = fs::read_to_string(&cache)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    if let Some(v) = &last {
        if v["fetched"].as_i64().is_some_and(|f| now - f < 300) && v["source"] == "live" {
            return v.clone();
        }
    }
    let backoff = data_dir().join("limits_backoff");
    let backing_off = fs::metadata(&backoff)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|e| e.as_secs() < 300);
    let marker = data_dir().join("no_keychain");
    let skip = fs::metadata(&marker)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|e| e.as_secs() < 86400);
    let live = (|| -> Option<Value> {
        if skip || backing_off {
            return None;
        }
        let o = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .output()
            .ok()?;
        if !o.status.success() {
            fs::write(&marker, "").ok();
            return None;
        }
        let cred: Value = serde_json::from_slice(&o.stdout).ok()?;
        let tok = cred["claudeAiOauth"]["accessToken"].as_str()?.to_string();
        let o = std::process::Command::new("curl")
            .args([
                "-s",
                "-m",
                "8",
                "https://api.anthropic.com/api/oauth/usage",
                "-H",
                &format!("Authorization: Bearer {tok}"),
                "-H",
                "anthropic-beta: oauth-2025-04-20",
            ])
            .output()
            .ok()?;
        let u: Value = serde_json::from_slice(&o.stdout).ok()?;
        u.get("five_hour")?;
        Some(u)
    })();
    if live.is_none() && !skip && !backing_off {
        fs::write(&backoff, "").ok();
    }
    let (u, source, fetched) = match live {
        Some(u) => (u, "live", now),
        None if let Some(mut v) = last => {
            v["source"] = json!("stale");
            return drop_reset_windows(v, now);
        }
        None => {
            let cj = fs::read_to_string(home().join(".claude.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok());
            match cj.as_ref().and_then(|v| v.get("cachedUsageUtilization")) {
                Some(c) => (
                    c["utilization"].clone(),
                    "stale",
                    c["fetchedAtMs"].as_i64().unwrap_or(0) / 1000,
                ),
                None => return json!({"source": "none"}),
            }
        }
    };
    let pick = |k: &str| json!({"pct": u[k]["utilization"], "resets_at": u[k]["resets_at"]});
    let out = json!({"source": source, "fetched": fetched, "session": pick("five_hour"), "weekly": pick("seven_day"),
        "weekly_sonnet": if u["seven_day_sonnet"].is_null() { Value::Null } else { pick("seven_day_sonnet") },
        "weekly_opus": if u["seven_day_opus"].is_null() { Value::Null } else { pick("seven_day_opus") }});
    if source == "live" {
        fs::write(&cache, out.to_string()).ok();
    }
    drop_reset_windows(out, now)
}

fn drop_reset_windows(mut v: Value, now: i64) -> Value {
    for k in ["session", "weekly", "weekly_sonnet", "weekly_opus"] {
        let past = v[k]["resets_at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .is_some_and(|t| t.timestamp() < now);
        if past {
            v[k] = Value::Null;
        }
    }
    v
}

fn load_aliases() -> HashMap<String, String> {
    let mut m: HashMap<String, String> = HashMap::new();
    if let Some(v) = fs::read_to_string(data_dir().join("aliases.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        if let Some(o) = v.as_object() {
            for (k, x) in o {
                if let Some(x) = x.as_str() {
                    m.insert(k.clone(), x.to_string());
                }
            }
        }
    }
    m
}

fn ed25519_seed(pem: &str) -> Option<[u8; 32]> {
    fn string(b: &[u8]) -> Option<(&[u8], &[u8])> {
        let n = u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as usize;
        Some((b.get(4..4 + n)?, b.get(4 + n..)?))
    }
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let raw = B64.decode(body.trim()).ok()?;
    let rest = raw.strip_prefix(b"openssh-key-v1\0")?;
    let (cipher, rest) = string(rest)?;
    if cipher != b"none" {
        return None;
    }
    let (_kdf, rest) = string(rest)?;
    let (_kdfopts, rest) = string(rest)?;
    let rest = rest.get(4..)?;
    let (_pubkey, rest) = string(rest)?;
    let (private, _) = string(rest)?;
    let p = private.get(8..)?;
    let (kind, p) = string(p)?;
    if kind != b"ssh-ed25519" {
        return None;
    }
    let (_pub, p) = string(p)?;
    let (sk, _) = string(p)?;
    sk.get(..32)?.try_into().ok()
}

fn fetch_ollama() -> Value {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    if !option_on("ollama") {
        return json!({"source": "off"});
    }
    let cache = data_dir().join("ollama.json");
    let now = Local::now().timestamp();
    if let Some(v) = fs::read_to_string(&cache)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        if v["fetched"].as_i64().is_some_and(|f| now - f < 90) && v["source"] == "live" {
            return v;
        }
    }
    let dir = home().join(".ollama");
    let (Ok(sk), Ok(pk)) = (
        fs::read_to_string(dir.join("id_ed25519")),
        fs::read_to_string(dir.join("id_ed25519.pub")),
    ) else {
        return json!({"source": "nokey"});
    };
    let res = (|| -> Option<Value> {
        let seed = ed25519_seed(&sk)?;
        let signing = SigningKey::from_bytes(&seed);
        let uri = format!("/api/usage?range=30d&ts={now}");
        let sig = signing.sign(format!("GET,{uri}").as_bytes());
        let auth = format!(
            "{}:{}",
            pk.split_whitespace().nth(1)?,
            B64.encode(sig.to_bytes())
        );
        let o = std::process::Command::new("curl")
            .args([
                "-s",
                "-m",
                "8",
                &format!("https://ollama.com{uri}"),
                "-H",
                &format!("Authorization: {auth}"),
            ])
            .output()
            .ok()?;
        let d: Value = serde_json::from_slice(&o.stdout).ok()?;
        d.get("totals")?;
        Some(d)
    })();
    let Some(d) = res else {
        return json!({"source": "error", "fetched": now});
    };
    let daily: Vec<f64> = d["buckets"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|b| b["usage_usd"].as_f64().unwrap_or(0.0))
                .collect()
        })
        .unwrap_or_default();
    let first = d["buckets"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|b| b["from"].as_str())
        .unwrap_or("")
        .to_string();
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
    let month0 = {
        let d = Local::now()
            .date_naive()
            .with_day(1)
            .expect("day 1 exists in every month");
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight is a valid time"))
            .earliest()
            .expect("local midnight exists")
            .timestamp()
    };
    let since = day_start(13).min(month0);
    let mut st = c.prepare("SELECT ts, source, project, model, inp, out, cread, cw5, cw1, kind, cost_override FROM calls WHERE ts>=?").expect("prepare query");
    let rows: Vec<CallRow> = st
        .query_map([since], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();

    let mut hourly = vec![[0i64; 3]; 24];
    let mut daily_tokens = vec![0i64; 14];
    let mut api_daily = vec![0f64; 14];
    let (mut api_today, mut api_week, mut api_month) = (0f64, 0f64, 0f64);
    let mut api_by: HashMap<String, (f64, i64)> = HashMap::new();
    let mut proj: HashMap<String, (i64, f64)> = HashMap::new();
    let mut proj_daily: HashMap<String, Vec<i64>> = HashMap::new();
    let limits = fetch_limits();
    let no_plan = limits["source"] == "none";
    let mut models: HashMap<String, (i64, String)> = HashMap::new();
    let (mut c_tok, mut o_tok, mut c_in, mut c_out, mut c_cr, mut c_cw) =
        (0i64, 0i64, 0i64, 0i64, 0i64, 0i64);
    let (mut c_week, mut o_week) = (0i64, 0i64);
    for (ts, source, pr, m, i, o, cr, w5, w1, kind, over) in &rows {
        let tok = i + o + cr + w5 + w1;
        let didx = if *ts >= today0 {
            13
        } else {
            13 - usize::try_from(((today0 - ts + 86399) / 86400).min(13)).unwrap_or(0)
        };
        daily_tokens[didx] += tok;
        let cost = over
            .or_else(|| cost_of(&prices, m, *i, *o, *cr, *w5, *w1))
            .unwrap_or(0.0);
        if kind == "api" || (no_plan && kind == "claude_sub") {
            api_daily[didx] += cost;
            if *ts >= today0 {
                api_today += cost;
            }
            if *ts >= week0 {
                api_week += cost;
            }
            if *ts >= month0 {
                api_month += cost;
            }
            if *ts >= month0 {
                let e = api_by
                    .entry(if source == "log" {
                        pr.clone()
                    } else {
                        source.clone()
                    })
                    .or_insert((0.0, 0));
                e.0 += cost;
                e.1 += tok;
            }
        }
        if source == "claude-code" || source == "log" {
            proj_daily
                .entry(aliases.get(pr).cloned().unwrap_or_else(|| pr.clone()))
                .or_insert_with(|| vec![0; 14])[didx] += i + o;
        }
        if *ts >= week0 {
            if source == "claude-code" || source == "log" {
                let e = proj
                    .entry(aliases.get(pr).cloned().unwrap_or_else(|| pr.clone()))
                    .or_insert((0, 0.0));
                e.0 += i + o;
                e.1 += cost;
            }
            models
                .entry(format!("{m}|{kind}"))
                .or_insert((0, kind.clone()))
                .0 += tok;
            match kind.as_str() {
                "claude_sub" => c_week += tok,
                "ollama_sub" => o_week += tok,
                _ => {}
            }
        }
        if now - ts < 86400 {
            let h = usize::try_from(23 - ((now - ts) / 3600).min(23)).unwrap_or(23);
            let k = match kind.as_str() {
                "claude_sub" => 0,
                "ollama_sub" => 1,
                _ => 2,
            };
            hourly[h][k] += tok;
        }
        if *ts >= today0 && kind == "claude_sub" {
            c_tok += tok;
            c_in += i;
            c_out += o;
            c_cr += cr;
            c_cw += w5 + w1;
        }
        if *ts >= today0 && kind == "ollama_sub" {
            o_tok += tok;
        }
    }
    let mut pv: Vec<_> = proj.into_iter().collect();
    pv.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    let mut mv: Vec<_> = models.into_iter().collect();
    mv.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    let mut av: Vec<_> = api_by.into_iter().collect();
    av.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
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
        "projects": pv.iter().map(|(k,v)| json!({"name": k, "tokens": v.0, "cost": v.1, "daily": proj_daily.get(k)})).collect::<Vec<_>>(),
        "models": mv.iter().map(|(k,v)| json!({"name": k.split('|').next().expect("split yields at least one part"), "tokens": v.0, "kind": v.1})).collect::<Vec<_>>(),
    });
    println!("{out}");
}

fn month_report(c: &Connection, back: i64) {
    use serde_json::json;
    let prices = load_prices();
    let first = {
        let t = Local::now()
            .date_naive()
            .with_day(1)
            .expect("day 1 exists in every month");
        let mut m = t.year() * 12 + i32::try_from(t.month0()).expect("month0 is below 12")
            - i32::try_from(back).unwrap_or(i32::MAX);
        if m < 0 {
            m = 0;
        }
        NaiveDate::from_ymd_opt(m / 12, (m % 12).unsigned_abs() + 1, 1)
            .expect("the first of a month is a valid date")
    };
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
            .expect("the first of a month is a valid date")
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
            .expect("the first of a month is a valid date")
    };
    let ts_of = |d: NaiveDate| {
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight is a valid time"))
            .earliest()
            .expect("local midnight exists")
            .timestamp()
    };
    let (t0, t1) = (ts_of(first), ts_of(next));
    let ndays =
        usize::try_from((next - first).num_days()).expect("next month starts after this one");
    let mut daily = vec![0f64; ndays];
    let mut by: HashMap<String, (f64, i64)> = HashMap::new();
    let mut st = c.prepare("SELECT ts, source, model, inp, out, cread, cw5, cw1, cost_override FROM calls WHERE kind='api' AND ts>=? AND ts<?").expect("prepare query");
    let rows = st
        .query_map([t0, t1], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<f64>>(8)?,
            ))
        })
        .expect("run query");
    let mut total = 0.0;
    for (ts, src, m, i, o, cr, w5, w1, over) in rows.flatten() {
        let cost = over
            .or_else(|| cost_of(&prices, &m, i, o, cr, w5, w1))
            .unwrap_or(0.0);
        let di = usize::try_from((ts - t0) / 86400)
            .unwrap_or(0)
            .min(ndays - 1);
        daily[di] += cost;
        total += cost;
        let e = by.entry(src).or_insert((0.0, 0));
        e.0 += cost;
        e.1 += i + o + cr + w5 + w1;
    }
    let mut av: Vec<_> = by.into_iter().collect();
    av.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    let has_prev = c
        .prepare("SELECT 1 FROM calls WHERE kind='api' AND ts<? LIMIT 1")
        .expect("prepare query")
        .exists([t0])
        .expect("run query");
    println!(
        "{}",
        json!({"label": first.format("%B %Y").to_string(), "first": first.format("%Y-%m-%d").to_string(), "total": total, "daily": daily, "has_prev": has_prev,
        "by_app": av.iter().map(|(k,v)| json!({"name": k, "cost": v.0, "tokens": v.1})).collect::<Vec<_>>()})
    );
}

fn note_tools(tx: &rusqlite::Transaction<'_>, line: &str) {
    let Ok(d) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let Some(items) = d.pointer("/message/content").and_then(|v| v.as_array()) else {
        return;
    };
    let ts = d
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map_or(0, |t| t.timestamp());
    let cwd = d.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
    let project = repo_name(cwd);
    let sess = d.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    for b in items {
        match b.get("type").and_then(|v| v.as_str()) {
            Some("tool_use") if d.get("type").and_then(|v| v.as_str()) == Some("assistant") => {
                let (Some(id), Some(name)) = (
                    b.get("id").and_then(|v| v.as_str()),
                    b.get("name").and_then(|v| v.as_str()),
                ) else {
                    continue;
                };
                let tool = match name.strip_prefix("mcp__") {
                    Some(r) => r.replacen("__", ":", 1),
                    None => name.to_string(),
                };
                let arg = b.get("input").map(tool_arg).unwrap_or_default();
                tx.execute("INSERT INTO tools(id, ts, sess, project, tool, arg) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET arg=excluded.arg, project=excluded.project", params![id, ts, sess, project, tool, arg]).expect("write to database");
            }
            Some("tool_result")
                if b.get("is_error").and_then(serde_json::Value::as_bool) == Some(true) =>
            {
                if let Some(id) = b.get("tool_use_id").and_then(|v| v.as_str()) {
                    let text = match b.get("content") {
                        Some(Value::String(t)) => t.clone(),
                        Some(Value::Array(a)) => a
                            .iter()
                            .filter_map(|x| x.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    };
                    let mut ls = text.lines().map(str::trim).filter(|l| !l.is_empty());
                    let first = ls.next().unwrap_or("");
                    let joined = if first.starts_with("Exit code") {
                        match ls.next() {
                            Some(n) => format!("{first}: {n}"),
                            None => first.to_string(),
                        }
                    } else {
                        first.to_string()
                    };
                    let msg: String = joined.chars().take(160).collect();
                    tx.execute("UPDATE tools SET err=1, msg=? WHERE id=?", params![msg, id])
                        .expect("write to database");
                }
            }
            _ => {}
        }
    }
}

fn tool_arg(input: &Value) -> String {
    let pick = [
        "command",
        "file_path",
        "path",
        "query",
        "url",
        "pattern",
        "prompt",
        "description",
    ]
    .iter()
    .find_map(|k| input.get(*k).and_then(|v| v.as_str()))
    .or_else(|| {
        input
            .as_object()
            .and_then(|o| o.values().find_map(|v| v.as_str()))
    });
    pick.unwrap_or("")
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(140)
        .collect()
}
fn fail_kind(msg: &str) -> &'static str {
    let m = msg.to_lowercase();
    if m.starts_with("exit code") {
        "exit"
    } else if m.contains("classifier")
        || m.contains("denied")
        || m.contains("rejected")
        || m.contains("doesn't want")
        || m.contains("permission")
    {
        "blocked"
    } else {
        "error"
    }
}

fn trace_report(c: &Connection) {
    use serde_json::json;
    let aliases = load_aliases();
    let now = Local::now().timestamp();
    let mut st = c.prepare("SELECT session, MIN(project), MIN(ts), MAX(ts), SUM(inp+out+cread+cw5+cw1), COUNT(*) FROM calls
        WHERE source='claude-code' AND session!='' AND ts>=? GROUP BY session ORDER BY MAX(ts) DESC LIMIT 150").expect("prepare query");
    let rows: Vec<(String, String, i64, i64, i64, i64)> = st
        .query_map([now - 7 * 86400], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let mut sessions = vec![];
    for (sess, proj, t0, t1, tok, calls) in rows {
        let model: String = c.query_row("SELECT model FROM calls WHERE session=? GROUP BY model ORDER BY SUM(out) DESC LIMIT 1", [&sess], |r| r.get(0)).unwrap_or_default();
        let (nt, ne): (i64, i64) = c
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(err),0) FROM tools WHERE sess=?",
                [&sess],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));
        let mut ts = c.prepare("SELECT tool, COUNT(*), SUM(err) FROM tools WHERE sess=? GROUP BY tool ORDER BY COUNT(*) DESC LIMIT 12").expect("prepare query");
        let top: Vec<Value> = ts.query_map([&sess], |r| Ok(json!({"name": r.get::<_,String>(0)?, "n": r.get::<_,i64>(1)?, "err": r.get::<_,i64>(2)?}))).expect("run query").map(|row| row.expect("decode query row")).collect();
        let mut fs_ = c.prepare("SELECT tool, COALESCE(arg,''), COALESCE(msg,'') FROM tools WHERE sess=? AND err=1 ORDER BY ts DESC LIMIT 30").expect("prepare query");
        let fails: Vec<Value> = fs_.query_map([&sess], |r| { let m: String = r.get(2)?; Ok(json!({"tool": r.get::<_,String>(0)?, "arg": r.get::<_,String>(1)?, "kind": fail_kind(&m), "msg": m})) }).expect("run query").map(|row| row.expect("decode query row")).collect();
        sessions.push(json!({"id": sess, "fails": fails, "project": aliases.get(&proj).cloned().unwrap_or(proj), "model": model, "start": t0, "end": t1,
            "tokens": tok, "calls": calls, "tools": nt, "errors": ne, "top": top}));
    }
    let mut tt = c.prepare("SELECT tool, COUNT(*), SUM(err) FROM tools WHERE ts>=? GROUP BY tool ORDER BY COUNT(*) DESC LIMIT 20").expect("prepare query");
    let top24: Vec<Value> = tt.query_map([now - 86400], |r| Ok(json!({"name": r.get::<_,String>(0)?, "n": r.get::<_,i64>(1)?, "err": r.get::<_,i64>(2)?}))).expect("run query").map(|row| row.expect("decode query row")).collect();
    let (n24, e24): (i64, i64) = c
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(err),0) FROM tools WHERE ts>=?",
            [now - 86400],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0));
    let mut fr = c.prepare("SELECT tool, COALESCE(arg,''), COALESCE(msg,''), ts, project FROM tools WHERE err=1 AND ts>=? ORDER BY ts DESC").expect("prepare query");
    let all: Vec<(String, String, String, i64, String)> = fr
        .query_map([now - 86400], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let mut kinds: HashMap<&str, i64> = HashMap::new();
    for f in &all {
        *kinds.entry(fail_kind(&f.2)).or_insert(0) += 1;
    }
    let mut fd = c
        .prepare("SELECT ts, err, COALESCE(msg,'') FROM tools WHERE ts>=?")
        .expect("prepare query");
    let mut daily = vec![(0i64, 0i64, 0i64, 0i64); 7];
    let day0 = day_start(6);
    for (ts, err, msg) in fd
        .query_map([day0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .expect("run query")
        .flatten()
    {
        let Some(d) = (0..7).rev().find(|k| ts >= day_start(6 - k)) else {
            continue;
        };
        let e = &mut daily[usize::try_from(d).unwrap_or(0)];
        e.0 += 1;
        if err == 1 {
            match fail_kind(&msg) {
                "exit" => e.1 += 1,
                "blocked" => e.2 += 1,
                _ => e.3 += 1,
            }
        }
    }
    let fails_daily: Vec<Value> = daily
        .iter()
        .enumerate()
        .map(|(k, d)| json!({"day": day_start(6 - i64::try_from(k).unwrap_or(0)), "calls": d.0, "exit": d.1, "blocked": d.2, "error": d.3}))
        .collect();
    let recent: Vec<Value> = all.iter().take(40).map(|f| json!({"tool": f.0, "arg": f.1, "msg": f.2, "kind": fail_kind(&f.2), "ts": f.3, "project": aliases.get(&f.4).cloned().unwrap_or(f.4.clone())})).collect();
    println!(
        "{}",
        json!({"sessions": sessions, "tools_24h": n24, "errors_24h": e24, "top_24h": top24, "kinds_24h": kinds, "recent_fails": recent, "daily": fails_daily})
    );
}

fn proc_name(raw: &str) -> String {
    let base = raw
        .rsplit_once('.')
        .filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
        .map_or(raw, |(n, _)| n);
    if base.chars().all(|c| c.is_ascii_digit() || c == '.') && base.contains('.') {
        "claude".into()
    } else {
        base.to_string()
    }
}
fn host_label(ip: &str) -> Option<&'static str> {
    if ip.starts_with("2607:6bc0:")
        || ip.starts_with("160.79.104.")
        || ip.starts_with("160.79.105.")
    {
        Some("api.anthropic.com")
    } else {
        None
    }
}
fn is_known(host: &str) -> bool {
    [
        "anthropic.com",
        "claude.ai",
        "datadoghq.com",
        "npmjs.org",
        "discord.gg",
        "discord.com",
        "ollama.com",
        "apple.com",
        "icloud.com",
        "apple-dns.net",
        "google.com",
        "1e100.net",
        "googleusercontent.com",
        "gstatic.com",
        "cloudflare.com",
        "akamaitechnologies.com",
        "akamai.net",
        "amazonaws.com",
        "cloudfront.net",
        "yahoo.com",
        "github.com",
        "tailscale.com",
    ]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}
fn is_private(ip: &str) -> bool {
    ip.starts_with("127.")
        || ip == "::1"
        || ip.starts_with("192.168.")
        || ip.starts_with("10.")
        || ip.starts_with("169.254.")
        || ip.starts_with("fe80:")
        || ip.starts_with("fd")
        || ip.starts_with("100.")
}

fn pid_label(c: &Connection, pid: &str, base: &str) -> String {
    if !(base == "claude" || base == "node" || base == "ollama" || base.starts_with("python")) {
        return base.to_string();
    }
    if let Ok(l) = c.query_row(
        "SELECT label FROM pidlabel WHERE pid=? AND base=?",
        params![pid, base],
        |r| r.get::<_, String>(0),
    ) {
        return l;
    }
    let cwd = std::process::Command::new("lsof")
        .args(["-a", "-p", pid, "-d", "cwd", "-Fn"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find_map(|l| l.strip_prefix('n').map(String::from))
        })
        .unwrap_or_default();
    let label = match Path::new(&cwd).file_name() {
        Some(n) if !cwd.is_empty() && cwd != "/" => format!("{base} · {}", n.to_string_lossy()),
        _ => base.to_string(),
    };
    c.execute(
        "INSERT OR REPLACE INTO pidlabel VALUES(?,?,?)",
        params![pid, base, label],
    )
    .ok();
    label
}

fn net_report(c: &Connection) {
    use serde_json::json;
    let now = Local::now().timestamp();
    c.execute(
        "DELETE FROM pidlabel WHERE rowid IN (SELECT rowid FROM pidlabel LIMIT -1 OFFSET 400)",
        [],
    )
    .ok();
    if let Ok(o) = std::process::Command::new("nettop")
        .args(["-P", "-L", "1", "-m", "tcp", "-J", "bytes_in,bytes_out"])
        .output()
    {
        for l in String::from_utf8_lossy(&o.stdout).lines().skip(1) {
            let f: Vec<&str> = l.split(',').collect();
            if f.len() < 3 {
                continue;
            }
            let (Ok(bi), Ok(bo)) = (f[1].parse::<i64>(), f[2].parse::<i64>()) else {
                continue;
            };
            let prev: Option<(i64, i64)> = c
                .query_row("SELECT bin, bout FROM netprev WHERE k=?", [f[0]], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .ok();
            let (di, dout) = match prev {
                Some((pi, po)) if bi >= pi && bo >= po => (bi - pi, bo - po),
                Some(_) => (bi, bo),
                None => (0, 0),
            };
            if di + dout > 0 {
                let pid = f[0].rsplit_once('.').map_or("0", |x| x.1);
                c.execute(
                    "INSERT INTO net VALUES(?,?,?,?)",
                    params![now, pid_label(c, pid, &proc_name(f[0])), di, dout],
                )
                .expect("write to database");
            }
            c.execute(
                "INSERT OR REPLACE INTO netprev VALUES(?,?,?)",
                params![f[0], bi, bo],
            )
            .expect("write to database");
        }
    }
    c.execute("DELETE FROM net WHERE ts<?", [now - 8 * 86400])
        .ok();
    let mut conns: Vec<(String, String, String, String)> = vec![];
    if let Ok(o) = std::process::Command::new("lsof")
        .args(["-iTCP", "-sTCP:ESTABLISHED", "-nP", "-Fpcn"])
        .output()
    {
        let (mut pid, mut base) = (String::new(), String::new());
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some(x) = l.strip_prefix('p') {
                pid = x.to_string();
            } else if let Some(x) = l.strip_prefix('c') {
                base = proc_name(&format!("{x}.0"));
            } else if let Some(n) = l.strip_prefix('n') {
                let Some((_, remote)) = n.split_once("->") else {
                    continue;
                };
                let Some((ip, port)) = remote.rsplit_once(':') else {
                    continue;
                };
                let ip = ip.trim_matches(|c| c == '[' || c == ']');
                if is_private(ip) {
                    continue;
                }
                conns.push((
                    pid_label(c, &pid, &base),
                    pid.clone(),
                    ip.to_string(),
                    port.to_string(),
                ));
            }
        }
    }
    let mut up: HashMap<String, String> = HashMap::new();
    if let Ok(o) = std::process::Command::new("ps")
        .args(["-Ao", "pid=,etime="])
        .output()
    {
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            let mut it = l.split_whitespace();
            if let (Some(p), Some(e)) = (it.next(), it.next()) {
                up.insert(p.to_string(), e.to_string());
            }
        }
    }
    for h in [
        "api.anthropic.com",
        "claude.ai",
        "platform.claude.com",
        "mcp-proxy.anthropic.com",
        "downloads.claude.ai",
        "http-intake.logs.us5.datadoghq.com",
        "ollama.com",
        "registry.npmjs.org",
        "github.com",
        "api.github.com",
        "gateway.discord.gg",
        "discord.com",
    ] {
        let fresh = c
            .query_row("SELECT ts FROM fwd WHERE host=?", [h], |r| {
                r.get::<_, i64>(0)
            })
            .is_ok_and(|t| now - t < 6 * 3600);
        if fresh {
            continue;
        }
        let mut ips = vec![];
        for rt in ["A", "AAAA"] {
            if let Ok(o) = std::process::Command::new("dig")
                .args(["+short", "+time=1", "+tries=1", rt, h])
                .output()
            {
                ips.extend(
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter(|l| {
                            l.chars().next().is_some_and(|c| c.is_ascii_hexdigit())
                                && l.contains(['.', ':'])
                        })
                        .map(String::from),
                );
            }
        }
        if !ips.is_empty() {
            c.execute(
                "INSERT OR REPLACE INTO fwd VALUES(?,?,?)",
                params![h, format!(" {} ", ips.join(" ")), now],
            )
            .ok();
        }
    }
    let mut budget = 6;
    let mut name_of = |ip: &str| -> String {
        if let Some(h) = host_label(ip) {
            return h.to_string();
        }
        if let Ok(h) = c.query_row(
            "SELECT host FROM fwd WHERE ips LIKE '% ' || ? || ' %' LIMIT 1",
            [ip],
            |r| r.get::<_, String>(0),
        ) {
            return h;
        }
        if let Ok(n) = c.query_row("SELECT name FROM rdns WHERE ip=?", [ip], |r| {
            r.get::<_, String>(0)
        }) {
            return if n.is_empty() { ip.to_string() } else { n };
        }
        if budget == 0 {
            return ip.to_string();
        }
        budget -= 1;
        let n = std::process::Command::new("host")
            .args(["-W", "1", ip])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split("domain name pointer ")
                    .nth(1)
                    .map(|s| s.trim().trim_end_matches('.').to_string())
            })
            .unwrap_or_default();
        c.execute("INSERT OR REPLACE INTO rdns VALUES(?,?)", params![ip, n])
            .ok();
        if n.is_empty() {
            ip.to_string()
        } else {
            n
        }
    };
    let mut st = c
        .prepare("SELECT proc, SUM(din), SUM(dout) FROM net WHERE ts>=? GROUP BY proc")
        .expect("prepare query");
    let rows: Vec<(String, i64, i64)> = st
        .query_map([now - 86400], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let mut names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    for k in conns.iter().map(|x| &x.0) {
        if !names.contains(k) {
            names.push(k.clone());
        }
    }
    let is_ai = |n: &str| {
        ["claude", "ollama"].iter().any(|a| n.starts_with(a))
            || n.starts_with("python")
            || n.starts_with("node")
            || n.starts_with("aimon")
    };
    let mut procs: Vec<Value> = names.iter().map(|n| {
        let (bi, bo) = rows.iter().find(|r| &r.0 == n).map_or((0, 0), |r| (r.1, r.2));
        let mine: Vec<&(String,String,String,String)> = conns.iter().filter(|x| &x.0 == n).collect();
        let mut hs: Vec<(String, i64)> = vec![];
        for x in &mine { let h = name_of(&x.2); match hs.iter_mut().find(|e| e.0 == h) { Some(e) => e.1 += 1, None => hs.push((h, 1)) } }
        hs.sort_by_key(|e| Reverse(e.1));
        let detail: Vec<Value> = mine.iter().take(12).map(|x| json!({"host": name_of(&x.2), "ip": x.2, "port": x.3, "known": is_known(&name_of(&x.2)), "pid": x.1, "up": up.get(&x.1).cloned().unwrap_or_default()})).collect();
        json!({"name": n, "in": bi, "out": bo, "ai": is_ai(n), "conns": mine.len(), "unknown": mine.iter().filter(|x| !is_known(&name_of(&x.2))).count(), "hosts": hs.iter().take(4).map(|h| h.0.clone()).collect::<Vec<_>>(), "detail": detail})
    }).filter(|p| p["in"].as_i64().unwrap_or(0) + p["out"].as_i64().unwrap_or(0) > 0 || p["conns"].as_i64().unwrap_or(0) > 0).collect();
    procs.sort_by_key(|p| {
        (
            !p["ai"].as_bool().unwrap_or(false),
            -(p["in"].as_i64().unwrap_or(0) + p["out"].as_i64().unwrap_or(0)),
        )
    });
    println!(
        "{}",
        json!({"procs": procs.into_iter().take(12).collect::<Vec<_>>(), "since": now - 86400})
    );
}

fn out_of(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn all_repos() -> (Option<PathBuf>, Vec<(String, PathBuf, String)>) {
    let dir = home().join("Library/Application Support/Backtrail");
    let Some(cfg) = fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return (None, vec![]);
    };
    let Some(ws) = cfg["workspace"].as_str().map(PathBuf::from) else {
        return (None, vec![]);
    };
    let Ok(c) = Connection::open_with_flags(
        dir.join("backtrail.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return (Some(ws), vec![]);
    };
    let states: HashMap<String, String> = c
        .prepare("SELECT name, state FROM threads")
        .and_then(|mut q| {
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map(|r| r.flatten().collect())
        })
        .unwrap_or_default();
    let repos = cfg["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let t = r[1].as_str()?.to_string();
            let s = states.get(&t).cloned().unwrap_or_default();
            Some((t, ws.join(r[0].as_str()?), s))
        })
        .filter(|(_, p, _)| p.join(".git").exists())
        .collect();
    (Some(ws), repos)
}

fn workspace_repos() -> (Option<PathBuf>, Vec<(String, PathBuf, String)>) {
    let (ws, mut repos) = all_repos();
    repos.retain(|r| r.2 != "dropped");
    let known: Vec<PathBuf> = repos.iter().map(|r| r.1.clone()).collect();
    if let Some(dir) = ws.as_ref().and_then(|w| fs::read_dir(w).ok()) {
        for p in dir.flatten().map(|e| e.path()) {
            if p.join(".git").exists() && !known.contains(&p) {
                let name = p
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().to_string());
                repos.push((name, p, String::new()));
            }
        }
    }
    (ws, repos)
}

const USE_GAP: i64 = 600;

fn use_cmd(a: &[String]) {
    let Some(key) = a.get(2).filter(|k| !k.starts_with("--")) else {
        eprintln!("usage: aimon use <app path|repo folder|thread> [--via app|web|cli]");
        return;
    };
    let via = a
        .iter()
        .position(|x| x == "--via")
        .and_then(|i| a.get(i + 1))
        .map_or("cli", |s| s.as_str());
    let c = open_db();
    let now = Local::now().timestamp();
    let recent: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM uses WHERE key=? AND ts>?",
            params![key, now - USE_GAP],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if recent == 0 {
        c.execute("INSERT INTO uses VALUES(?,?,?)", params![now, key, via])
            .ok();
    }
}

fn time_cmd(a: &[String]) {
    let (Some(key), Some(secs)) = (
        a.get(2),
        a.get(3)
            .and_then(|x| x.parse::<i64>().ok())
            .filter(|n| (1..=600).contains(n)),
    ) else {
        eprintln!("usage: aimon time <app path|repo folder|thread> <seconds>");
        return;
    };
    let day = Local::now().date_naive().to_string();
    open_db()
        .execute(
            "INSERT INTO spent VALUES(?,?,?) ON CONFLICT(day, key) DO UPDATE SET secs = secs + excluded.secs",
            params![day, key, secs],
        )
        .ok();
}

fn app_bundles(repos: &[(String, PathBuf, String)]) -> Vec<PathBuf> {
    let mut out = vec![];
    for d in [PathBuf::from("/Applications"), home().join("Applications")] {
        if let Ok(rd) = fs::read_dir(d) {
            out.extend(
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "app")),
            );
        }
    }
    for (_, r, _) in repos {
        let found = out_of(
            "find",
            &[
                &r.to_string_lossy(),
                "-maxdepth",
                "3",
                "-name",
                "*.app",
                "-type",
                "d",
                "-not",
                "-path",
                "*/.build/*",
                "-not",
                "-path",
                "*/node_modules/*",
                "-not",
                "-path",
                "*/target/*",
            ],
        );
        out.extend(found.lines().map(PathBuf::from));
    }
    out
}

fn repo_of_app(
    app: &Path,
    repos: &[(String, PathBuf, String)],
    ws: &Path,
    apps: &Value,
) -> Option<usize> {
    if let Some(i) = repos.iter().position(|r| app.starts_with(&r.1)) {
        return Some(i);
    }
    let file = app.file_name()?.to_string_lossy().to_string();
    if let Some(rel) = apps[file.as_str()].as_str() {
        return repos.iter().position(|r| r.1 == ws.join(rel));
    }
    let stem = file.trim_end_matches(".app").to_lowercase();
    repos.iter().position(|r| {
        r.1.file_name()
            .is_some_and(|n| n.to_string_lossy().to_lowercase() == stem)
    })
}

fn usage_report() {
    let (ws, repos) = all_repos();
    let Some(ws) = ws else {
        println!(
            "{}",
            serde_json::json!({"since": null, "rows": [], "untracked": []})
        );
        return;
    };
    let repos: Vec<_> = repos.into_iter().filter(|r| r.2 != "dropped").collect();
    let cfg = fs::read_to_string(data_dir().join("config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    let mut apps: HashMap<usize, Vec<String>> = HashMap::new();
    for b in app_bundles(&repos) {
        if let Some(i) = repo_of_app(&b, &repos, &ws, &cfg["apps"]) {
            apps.entry(i)
                .or_default()
                .push(b.to_string_lossy().to_string());
        }
    }
    let c = open_db();
    let now = Local::now().timestamp();
    let since: Option<i64> = c
        .query_row("SELECT MIN(ts) FROM uses", [], |r| r.get(0))
        .ok()
        .flatten();
    let rows_of = |keys: &[String]| -> Vec<i64> {
        let mut v = vec![];
        for k in keys {
            if let Ok(mut q) = c.prepare("SELECT ts FROM uses WHERE key=?") {
                v.extend(
                    q.query_map([k], |r| r.get::<_, i64>(0))
                        .map(|r| r.flatten().collect::<Vec<_>>())
                        .unwrap_or_default(),
                );
            }
        }
        v
    };
    let days_in = |ts: &[i64], n: i64| -> usize {
        let from = day_start(n - 1);
        let mut d: Vec<NaiveDate> = ts
            .iter()
            .filter(|t| **t >= from)
            .filter_map(|t| Local.timestamp_opt(*t, 0).single())
            .map(|t| t.date_naive())
            .collect();
        d.sort();
        d.dedup();
        d.len()
    };
    let mut rows = vec![];
    let mut untracked = vec![];
    for (i, (name, path, state)) in repos.iter().enumerate() {
        let rel = path
            .strip_prefix(&ws)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let app_list = apps.remove(&i).unwrap_or_default();
        let mut keys = app_list.clone();
        keys.push(rel.clone());
        keys.push(name.clone());
        let mut ts = rows_of(&keys);
        let web = cfg["web"]
            .as_array()
            .is_some_and(|w| w.iter().any(|x| x.as_str() == Some(rel.as_str())));
        if app_list.is_empty() && ts.is_empty() && !web {
            untracked.push(name.clone());
            continue;
        }
        let seen = app_list
            .iter()
            .filter_map(|a| {
                DateTime::parse_from_str(
                    out_of("mdls", &["-raw", "-name", "kMDItemLastUsedDate", a]).trim(),
                    "%Y-%m-%d %H:%M:%S %z",
                )
                .ok()
            })
            .map(|d| d.timestamp())
            .max();
        let last = ts.iter().copied().max().into_iter().chain(seen).max();
        ts.extend(seen);
        let commits = |days: i64| {
            out_of(
                "git",
                &[
                    "-C",
                    &path.to_string_lossy(),
                    "rev-list",
                    "--count",
                    &format!("--since={days}.days"),
                    "HEAD",
                ],
            )
            .trim()
            .parse::<i64>()
            .unwrap_or(0)
        };
        let (c7, c28) = (commits(7), commits(28));
        let idle = last.map(|l| (now - l) / 86400);
        let tracked_days = since.map_or(0, |s| (now - s) / 86400);
        let status = match idle {
            Some(d) if d <= 7 => "used",
            Some(d) if d <= 14 => "quiet",
            None if tracked_days < 14 => "new",
            _ if c28 > 0 => "built_unused",
            _ => "idle",
        };
        let mut per_day: std::collections::BTreeMap<NaiveDate, (usize, i64)> =
            std::collections::BTreeMap::new();
        for d in ts
            .iter()
            .filter(|t| **t >= day_start(370))
            .filter_map(|t| Local.timestamp_opt(*t, 0).single())
            .map(|t| t.date_naive())
        {
            per_day.entry(d).or_default().0 += 1;
        }
        for k in &keys {
            if let Ok(mut q) = c.prepare("SELECT day, secs FROM spent WHERE key=?") {
                for (d, n) in q
                    .query_map([k], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                    .map(|r| r.flatten().collect::<Vec<_>>())
                    .unwrap_or_default()
                {
                    if let Ok(d) = d.parse::<NaiveDate>() {
                        per_day.entry(d).or_default().1 += n;
                    }
                }
            }
        }
        let today = Local::now().date_naive();
        let sum_back = |from: i64, to: i64| -> (usize, i64) {
            per_day
                .iter()
                .filter(|(d, _)| {
                    let back = (today - **d).num_days();
                    back >= from && back < to
                })
                .fold((0, 0), |a, (_, v)| (a.0 + v.0, a.1 + v.1))
        };
        let wk: Vec<(usize, i64)> = (0..12).rev().map(|k| sum_back(7 * k, 7 * k + 7)).collect();
        let weeks: Vec<usize> = wk.iter().map(|w| w.0).collect();
        let week_secs: Vec<i64> = wk.iter().map(|w| w.1).collect();
        let (uses7, secs7) = sum_back(0, 7);
        let (uses28, secs28) = sum_back(0, 28);
        rows.push(serde_json::json!({"name": name, "path": path.to_string_lossy(), "state": state, "apps": app_list,
            "days7": days_in(&ts, 7), "days28": days_in(&ts, 28), "uses7": uses7, "uses28": uses28, "secs7": secs7, "secs28": secs28, "last": last, "commits7": c7, "commits28": c28, "status": status,
            "weeks": weeks, "days": per_day.keys().map(std::string::ToString::to_string).collect::<Vec<_>>(),
            "counts": per_day.values().map(|v| v.0).collect::<Vec<_>>(),
            "day_secs": per_day.values().map(|v| v.1).collect::<Vec<_>>(), "week_secs": week_secs}));
    }
    let rank = |s: &str| match s {
        "built_unused" => 0,
        "used" => 1,
        "quiet" => 2,
        "new" => 3,
        _ => 4,
    };
    rows.sort_by_key(|r| {
        (
            rank(r["status"].as_str().unwrap_or("")),
            -r["uses28"].as_i64().unwrap_or(0),
        )
    });
    println!(
        "{}",
        serde_json::json!({"since": since, "rows": rows, "untracked": untracked})
    );
}

fn projects_report() {
    let (root, found) = workspace_repos();
    let Some(root) = root else {
        println!("{}", serde_json::json!({"root": null, "projects": []}));
        return;
    };
    let repos: Vec<PathBuf> = found.iter().map(|f| f.1.clone()).collect();
    let owner = |p: &str| {
        repos
            .iter()
            .position(|r| !p.is_empty() && Path::new(p).starts_with(r))
    };

    let mut agents: HashMap<usize, (i64, Vec<String>)> = HashMap::new();
    let listed = out_of("launchctl", &["list"]);
    let pids: HashMap<&str, &str> = listed
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let pid = f.next()?;
            f.next()?;
            Some((f.next()?, pid))
        })
        .collect();
    let procs = out_of("ps", &["-axo", "command="]);
    if let Ok(rd) = fs::read_dir(home().join("Library/LaunchAgents")) {
        for e in rd.flatten() {
            if e.path().extension().is_none_or(|x| x != "plist") {
                continue;
            }
            let Ok(d) = serde_json::from_str::<Value>(&out_of(
                "plutil",
                &["-convert", "json", "-o", "-", &e.path().to_string_lossy()],
            )) else {
                continue;
            };
            let Some(label) = d["Label"].as_str() else {
                continue;
            };
            if d["KeepAlive"].is_null() || d["KeepAlive"] == false {
                continue;
            }
            let prog = d["ProgramArguments"][0]
                .as_str()
                .or(d["Program"].as_str())
                .unwrap_or("");
            let Some(i) =
                owner(d["WorkingDirectory"].as_str().unwrap_or("")).or_else(|| owner(prog))
            else {
                continue;
            };
            let a = agents.entry(i).or_default();
            let alive = pids.get(label).is_some_and(|p| *p != "-")
                || (!prog.is_empty() && procs.lines().any(|l| l.starts_with(prog)));
            if alive {
                a.0 += 1;
            } else {
                a.1.push(label.to_string());
            }
        }
    }

    let mut by_pid: HashMap<String, Vec<i64>> = HashMap::new();
    let mut pid = String::new();
    for l in out_of("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"]).lines() {
        if let Some(p) = l.strip_prefix('p') {
            pid = p.to_string();
        } else if let Some(port) = l
            .strip_prefix('n')
            .and_then(|n| n.rsplit(':').next())
            .and_then(|x| x.parse().ok())
        {
            by_pid.entry(pid.clone()).or_default().push(port);
        }
    }
    let mut ports: HashMap<usize, Vec<i64>> = HashMap::new();
    if !by_pid.is_empty() {
        let list = by_pid.keys().cloned().collect::<Vec<_>>().join(",");
        for l in out_of("lsof", &["-a", "-d", "cwd", "-Fpn", "-p", &list]).lines() {
            if let Some(p) = l.strip_prefix('p') {
                pid = p.to_string();
            } else if let Some(i) = l.strip_prefix('n').and_then(owner) {
                ports
                    .entry(i)
                    .or_default()
                    .extend(by_pid.get(&pid).cloned().unwrap_or_default());
            }
        }
    }

    let security: HashMap<String, Value> =
        fs::read_to_string(home().join("Library/Application Support/aimon/security.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|v| {
                v["repos"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|mut r| {
                        let path = r["path"].as_str()?.to_string();
                        r["checked"] = v["checked"].clone();
                        Some((path, r))
                    })
                    .collect()
            })
            .unwrap_or_default();

    let mut rows: Vec<Value> = repos.iter().enumerate().map(|(i, r)| {
        let dir = r.to_string_lossy();
        let status = out_of("git", &["--no-optional-locks", "-C", &dir, "status", "--porcelain=v2", "--branch"]);
        let ahead = status.lines().find_map(|l| l.strip_prefix("# branch.ab +")).and_then(|x| x.split(' ').next()).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        let (running, down) = agents.remove(&i).unwrap_or_default();
        let mut p = ports.remove(&i).unwrap_or_default();
        p.sort_unstable(); p.dedup();
        serde_json::json!({
            "name": found[i].0, "state": found[i].2,
            "path": dir, "running": running, "down": down, "ports": p,
            "remote": !out_of("git", &["-C", &dir, "remote"]).trim().is_empty(),
            "upstream": status.contains("# branch.upstream "),
            "ahead": ahead,
            "changed": status.lines().filter(|l| !l.starts_with('#')).count(),
            "security": security.get(&*dir).map(|s| serde_json::json!({"secrets": s["secrets"], "advisories": s["advisories"], "checked": s["checked"]})),
        })
    }).collect();
    rows.sort_by_key(|r| {
        (
            r["down"].as_array().is_none_or(std::vec::Vec::is_empty),
            r["name"].as_str().unwrap_or("").to_lowercase(),
        )
    });
    println!(
        "{}",
        serde_json::json!({"root": root.to_string_lossy(), "projects": rows})
    );
}

fn security_report() {
    let path = home().join("Library/Application Support/aimon/security.json");
    let Some(v) = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        println!("No scan results. A scanner writes {}.", path.display());
        return;
    };
    let age = v["checked"]
        .as_str()
        .and_then(|c| DateTime::parse_from_rfc3339(c).ok())
        .map(|d| (Local::now().signed_duration_since(d)).num_days());
    println!(
        "Security scan: {}",
        match age {
            Some(0) => "today".to_string(),
            Some(d) => format!("{d} days ago"),
            None => "date unknown".into(),
        }
    );
    let repos = v["repos"].as_array().cloned().unwrap_or_default();
    let width = repos
        .iter()
        .filter_map(|r| r["path"].as_str())
        .map(|p| p.rsplit('/').next().unwrap_or(p).len())
        .max()
        .unwrap_or(0);
    for r in &repos {
        let name = r["path"]
            .as_str()
            .map_or("?", |p| p.rsplit('/').next().unwrap_or(p));
        let (s, d) = (
            r["secrets"].as_i64().unwrap_or(0),
            r["advisories"].as_i64().unwrap_or(0),
        );
        let state = if s > 0 {
            "SECRETS"
        } else if d > 0 {
            "advisories"
        } else {
            "clean"
        };
        println!("{name:<width$}  {state:<10}  secrets {s}  advisories {d}");
    }
    let (s, d): (i64, i64) = repos.iter().fold((0, 0), |a, r| {
        (
            a.0 + r["secrets"].as_i64().unwrap_or(0),
            a.1 + r["advisories"].as_i64().unwrap_or(0),
        )
    });
    println!("{} repositories, {s} secrets, {d} advisories", repos.len());
}

fn exit_quietly_on_closed_stdout() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.starts_with("failed printing to stdout") && msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        default(info);
    }));
}

fn main() {
    exit_quietly_on_closed_stdout();
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("config") {
        config_cmd(&a);
        return;
    }
    if a.get(1).map(String::as_str) == Some("projects") {
        projects_report();
        return;
    }
    if a.get(1).map(String::as_str) == Some("security") {
        security_report();
        return;
    }
    if a.get(1).map(String::as_str) == Some("use") {
        use_cmd(&a);
        return;
    }
    if a.get(1).map(String::as_str) == Some("time") {
        time_cmd(&a);
        return;
    }
    if a.get(1).map(String::as_str) == Some("pace") {
        pace::report();
        return;
    }
    if a.get(1).map(String::as_str) == Some("usage") {
        usage_report();
        return;
    }
    let mut c = open_db();
    let (nf, nc) = scan(&mut c);
    scan_inbox(&c);
    eprintln!("scanned {nf} files, {nc} usage rows");
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
        _ => eprintln!(
            "usage: aimon [today | by-project N | by-model N | month [N] | snapshot | trace | net | scan | log ...]\n       aimon usage | pace | projects | security | use <target> | config <claude_limits|ollama> <on|off>"
        ),
    }
}
