use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::time::SystemTime;

const WEEKS: i64 = 8;
const MAINTAINED_DAYS: usize = 5;

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(i64::from(d.weekday().num_days_from_monday()))
}

fn is_prompt(e: &Value) -> bool {
    if e["type"] != "user" || e["isMeta"] == true {
        return false;
    }
    match &e["message"]["content"] {
        Value::String(_) => true,
        Value::Array(parts) => parts.iter().any(|p| p["type"] == "text"),
        _ => false,
    }
}

fn run_lengths(from: NaiveDate) -> BTreeMap<NaiveDate, Vec<i64>> {
    let cutoff = SystemTime::now() - std::time::Duration::from_hours(24 * 7 * 9);
    let mut weeks: BTreeMap<NaiveDate, Vec<i64>> = BTreeMap::new();
    let Ok(projects) = fs::read_dir(crate::home().join(".claude/projects")) else {
        return weeks;
    };
    for f in projects
        .flatten()
        .filter_map(|p| fs::read_dir(p.path()).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .is_ok_and(|m| m > cutoff)
        })
    {
        let Ok(file) = File::open(&f) else { continue };
        let mut start: Option<DateTime<Local>> = None;
        let mut last: Option<DateTime<Local>> = None;
        let mut close = |start: Option<DateTime<Local>>, last: Option<DateTime<Local>>| {
            if let (Some(s), Some(l)) = (start, last) {
                let d = monday(s.date_naive());
                if l > s && d >= from {
                    weeks.entry(d).or_default().push((l - s).num_seconds());
                }
            }
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !line.contains("\"timestamp\"") {
                continue;
            }
            let Ok(e) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(ts) = e["timestamp"]
                .as_str()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Local))
            else {
                continue;
            };
            if is_prompt(&e) {
                close(start, last);
                start = Some(ts);
                last = None;
            } else if e["type"] == "assistant" {
                last = Some(ts);
            }
        }
        close(start, last);
    }
    weeks
}

fn percentile(sorted: &[i64], q: usize) -> i64 {
    sorted
        .get((sorted.len().saturating_sub(1)) * q / 100)
        .copied()
        .unwrap_or(0)
}

fn corrections(from: NaiveDate) -> Option<BTreeMap<NaiveDate, (i64, i64)>> {
    let c = Connection::open_with_flags(
        crate::home().join("Library/Application Support/Backtrail/backtrail.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let mut q = c
        .prepare(
            "SELECT date(u.ts, 'weekday 0', '-6 days'), COUNT(*), SUM(t.correction)
             FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id
             WHERE u.ts >= ?1 GROUP BY 1",
        )
        .ok()?;
    let rows = q
        .query_map([from.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .ok()?;
    Some(
        rows.flatten()
            .filter_map(|(w, n, y)| Some((w.parse().ok()?, (n, y))))
            .collect(),
    )
}

fn commit_days(path: &std::path::Path) -> Vec<String> {
    let mut days: Vec<String> = crate::out_of(
        "git",
        &[
            "-C",
            &path.to_string_lossy(),
            "log",
            "--since=28.days",
            "--format=%ad",
            "--date=format-local:%Y-%m-%d",
        ],
    )
    .lines()
    .map(str::to_string)
    .collect();
    days.sort();
    days.dedup();
    days
}

pub fn report() {
    let from = monday(Local::now().date_naive()) - Duration::days(7 * (WEEKS - 1));
    let runs = run_lengths(from);
    let tags = corrections(from);
    let weeks: Vec<Value> = (0..WEEKS)
        .map(|k| from + Duration::days(7 * k))
        .map(|w| {
            let mut r = runs.get(&w).cloned().unwrap_or_default();
            r.sort_unstable();
            let (turns, fixes) = tags
                .as_ref()
                .and_then(|t| t.get(&w).copied())
                .unwrap_or((0, 0));
            json!({"week": w.to_string(), "runs": r.len(), "p50_secs": percentile(&r, 50),
                   "p90_secs": percentile(&r, 90), "tagged_turns": turns, "corrections": fixes})
        })
        .collect();
    let (_, repos) = crate::workspace_repos();
    let mut projects: Vec<Value> = repos
        .iter()
        .map(|(name, path, state)| {
            let dates = commit_days(path);
            let today = Local::now().date_naive();
            let strip: Vec<bool> = (0..28)
                .rev()
                .map(|back| dates.contains(&(today - Duration::days(back)).to_string()))
                .collect();
            let d = strip.iter().filter(|x| **x).count();
            json!({"name": name, "state": state, "commit_days28": d, "maintained": d >= MAINTAINED_DAYS, "strip": strip})
        })
        .collect();
    projects.sort_by_key(|p| std::cmp::Reverse(p["commit_days28"].as_u64().unwrap_or(0)));
    let maintained = projects.iter().filter(|p| p["maintained"] == true).count();
    println!(
        "{}",
        json!({"weeks": weeks, "corrections_tracked": tags.is_some(),
               "maintained": maintained, "projects_total": projects.len(),
               "maintained_rule": format!("commits on {MAINTAINED_DAYS}+ days in the last 28"),
               "projects": projects})
    );
}
