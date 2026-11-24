use chrono::{Local, NaiveDateTime, TimeZone};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const ROTATE_BYTES: u64 = 5 * 1024 * 1024;
const TAIL_BYTES: u64 = 64 * 1024;
const TAIL_LINES: usize = 300;
const RECENT_SECS: i64 = 86400;

struct Mark {
    off: i64,
    ts: i64,
    pid: Option<i64>,
    base: bool,
}

fn log_dir() -> PathBuf {
    crate::home().join("Library/Logs/dev")
}

fn rotate(path: &Path) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() < ROTATE_BYTES {
        return;
    }
    let mut old = path.as_os_str().to_owned();
    old.push(".1");
    if fs::copy(path, &old).is_ok() {
        if let Ok(f) = OpenOptions::new().write(true).open(path) {
            f.set_len(0).ok();
        }
    }
}

fn tail(path: &Path) -> Vec<(i64, String)> {
    let Ok(mut f) = File::open(path) else {
        return vec![];
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let from = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return vec![];
    }
    let mut buf = vec![];
    f.read_to_end(&mut buf).ok();
    let start = i64::try_from(from).unwrap_or(0);
    let mut lines = vec![];
    let mut pos = 0usize;
    for chunk in buf.split_inclusive(|b| *b == b'\n') {
        pos += chunk.len();
        let end = start + i64::try_from(pos).unwrap_or(0);
        let text = String::from_utf8_lossy(chunk).trim_end().to_string();
        lines.push((end, text));
    }
    if from > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(TAIL_LINES);
    lines.split_off(skip)
}

fn own_time(line: &str) -> Option<i64> {
    let head: String = line.trim_start_matches('[').chars().take(19).collect();
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(&head, f).ok())
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map(|t| t.timestamp())
}

fn update_marks(c: &Connection, path: &str, size: i64, pid: Option<i64>, now: i64) {
    let last: Option<(i64, Option<i64>)> = c
        .query_row(
            "SELECT off, pid FROM logmark WHERE path = ? ORDER BY off DESC, ts DESC LIMIT 1",
            [path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let insert = |base: bool| {
        c.execute(
            "INSERT INTO logmark VALUES(?, ?, ?, ?, ?)",
            params![path, size, now, pid, base],
        )
        .expect("write to database");
    };
    match last {
        None => insert(true),
        Some((off, _)) if size < off => {
            c.execute("DELETE FROM logmark WHERE path = ?", [path])
                .expect("write to database");
            insert(true);
        }
        Some((off, _)) if size > off => insert(false),
        _ => {}
    }
    let keep_from = size - i64::try_from(TAIL_BYTES).unwrap_or(0);
    c.execute(
        "DELETE FROM logmark WHERE path = ?1 AND off < (SELECT MAX(off) FROM logmark WHERE path = ?1 AND off < ?2)",
        params![path, keep_from],
    )
    .expect("write to database");
}

fn marks(c: &Connection, path: &str) -> Vec<Mark> {
    let mut st = c
        .prepare("SELECT off, ts, pid, base FROM logmark WHERE path = ? ORDER BY off, ts")
        .expect("prepare query");
    st.query_map([path], |r| {
        Ok(Mark {
            off: r.get(0)?,
            ts: r.get(1)?,
            pid: r.get(2)?,
            base: r.get(3)?,
        })
    })
    .expect("run query")
    .flatten()
    .collect()
}

fn dated(lines: Vec<(i64, String)>, marks: &[Mark], now: i64) -> Vec<Value> {
    let mut prev_own = None;
    lines
        .into_iter()
        .map(|(end, text)| {
            let m = marks.iter().find(|m| m.off >= end);
            let (t, pid) = match m {
                Some(m) if m.base => (None, None),
                Some(m) => (Some(m.ts), m.pid),
                None => (Some(now), None),
            };
            let own = own_time(&text);
            if own.is_some() {
                prev_own = own;
            }
            let t = own.or(t).or(prev_own);
            json!({"t": t, "own": own.is_some(), "pid": pid, "err": is_error(&text), "text": text})
        })
        .collect()
}

fn is_error(line: &str) -> bool {
    let l = line.to_lowercase();
    [
        "error",
        "exception",
        "traceback",
        "panicked",
        "fatal",
        "failed",
        "critical",
    ]
    .iter()
    .any(|w| l.contains(w))
}

fn launchd_state() -> HashMap<String, (Option<i64>, i64)> {
    crate::out_of("launchctl", &["list"])
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split('\t');
            let pid = f.next()?.parse().ok();
            let status = f.next()?.parse().unwrap_or(0);
            Some((f.next()?.to_string(), (pid, status)))
        })
        .collect()
}

pub fn report() {
    let c = crate::open_db();
    let now = Local::now().timestamp();
    let (_, repos) = crate::workspace_repos();
    let state = launchd_state();
    let mut out: Vec<Value> = vec![];
    let Ok(rd) = fs::read_dir(crate::home().join("Library/LaunchAgents")) else {
        println!("{}", json!({ "dir": log_dir(), "services": out }));
        return;
    };
    for e in rd.flatten() {
        if e.path().extension().is_none_or(|x| x != "plist") {
            continue;
        }
        let Ok(d) = serde_json::from_str::<Value>(&crate::out_of(
            "plutil",
            &["-convert", "json", "-o", "-", &e.path().to_string_lossy()],
        )) else {
            continue;
        };
        let Some(label) = d["Label"].as_str() else {
            continue;
        };
        let mut paths: Vec<&str> = [&d["StandardOutPath"], &d["StandardErrorPath"]]
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|p| *p != "/dev/null")
            .collect();
        paths.dedup();
        let Some(path) = paths.first().map(PathBuf::from) else {
            continue;
        };
        rotate(&path);
        let prog = d["ProgramArguments"][0]
            .as_str()
            .or(d["Program"].as_str())
            .unwrap_or("");
        let wd = d["WorkingDirectory"].as_str().unwrap_or("");
        let project = repos
            .iter()
            .find(|(_, r, _)| {
                (!wd.is_empty() && Path::new(wd).starts_with(r))
                    || (!prog.is_empty() && Path::new(prog).starts_with(r))
            })
            .map(|(n, _, _)| n.clone());
        let kind = if !d["KeepAlive"].is_null() && d["KeepAlive"] != false {
            "always"
        } else if !d["StartInterval"].is_null() || !d["StartCalendarInterval"].is_null() {
            "scheduled"
        } else {
            "manual"
        };
        let (pid, status) = state.get(label).copied().unwrap_or((None, 0));
        let meta = fs::metadata(&path).ok();
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|t| t.as_secs());
        let down = kind == "always" && pid.is_none();
        let key = path.to_string_lossy().to_string();
        let size = i64::try_from(meta.as_ref().map_or(0, std::fs::Metadata::len)).unwrap_or(0);
        update_marks(&c, &key, size, pid, now);
        let ms = marks(&c, &key);
        let tracked = ms.iter().find(|m| m.base).map(|m| m.ts);
        let lines = dated(tail(&path), &ms, now);
        let recent: Vec<i64> = lines
            .iter()
            .enumerate()
            .filter(|(i, l)| l["err"] == true && (*i == 0 || lines[i - 1]["err"] != true))
            .filter_map(|(_, l)| l["t"].as_i64())
            .filter(|t| now - t < RECENT_SECS)
            .collect();
        out.push(json!({
            "label": label, "project": project, "path": path, "kind": kind,
            "loaded": state.contains_key(label), "pid": pid, "status": status, "down": down,
            "size": meta.map_or(0, |m| m.len()), "modified": modified,
            "errors": recent.len(), "last_error": recent.iter().max(), "tracked": tracked, "lines": lines,
        }));
    }
    out.sort_by_key(|s| {
        (
            s["down"] != true && s["status"] == 0,
            s["label"].as_str().unwrap_or("").to_string(),
        )
    });
    println!("{}", json!({ "dir": log_dir(), "services": out }));
}
