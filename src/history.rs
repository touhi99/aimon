use chrono::{Local, NaiveDate};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;

fn window(days: i64) -> (i64, usize, NaiveDate) {
    let days = days.clamp(1, 366);
    (
        days,
        usize::try_from(days).expect("days is positive"),
        Local::now().date_naive(),
    )
}

fn slot(day: &str, today: NaiveDate, len: usize) -> Option<usize> {
    let d = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    let back = usize::try_from((today - d).num_days()).ok()?;
    (back < len).then(|| len - 1 - back)
}

fn day_ts(len: usize, k: usize) -> i64 {
    crate::day_start(i64::try_from(len - 1 - k).unwrap_or(0))
}

pub fn fails(c: &Connection, days: i64) {
    let (days, len, today) = window(days);
    let from = crate::day_start(days - 1);
    let mut out = vec![[0i64; 4]; len];
    let mut st = c
        .prepare(
            "SELECT date(ts, 'unixepoch', 'localtime'), COUNT(*) FROM tools WHERE ts >= ? GROUP BY 1",
        )
        .expect("prepare query");
    for (day, n) in st
        .query_map([from], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .expect("run query")
        .flatten()
    {
        if let Some(i) = slot(&day, today, len) {
            out[i][0] += n;
        }
    }
    let mut st = c
        .prepare("SELECT date(ts, 'unixepoch', 'localtime'), COALESCE(msg, '') FROM tools WHERE err = 1 AND ts >= ?")
        .expect("prepare query");
    for (day, msg) in st
        .query_map([from], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .expect("run query")
        .flatten()
    {
        if let Some(i) = slot(&day, today, len) {
            let k = match crate::fail_kind(&msg) {
                "exit" => 1,
                "blocked" => 2,
                _ => 3,
            };
            out[i][k] += 1;
        }
    }
    let daily: Vec<Value> = out
        .iter()
        .enumerate()
        .map(|(k, d)| json!({"day": day_ts(len, k), "calls": d[0], "exit": d[1], "blocked": d[2], "error": d[3]}))
        .collect();
    println!("{}", json!({ "daily": daily }));
}

pub fn sessions(c: &Connection, day: i64) {
    let aliases = crate::load_aliases();
    let list = crate::sessions_in(c, &aliases, day, day + 86400);
    println!("{}", json!({ "sessions": list }));
}

pub fn net_days(c: &Connection, days: i64) {
    let (days, len, today) = window(days);
    let from = (today - chrono::Duration::days(days - 1)).to_string();
    let mut daily = vec![(0i64, 0i64); len];
    let mut procs: HashMap<String, (i64, i64)> = HashMap::new();
    let mut st = c
        .prepare("SELECT day, proc, din, dout FROM netday WHERE day >= ?")
        .expect("prepare query");
    for (day, proc, din, dout) in st
        .query_map([&from], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .expect("run query")
        .flatten()
    {
        if let Some(i) = slot(&day, today, len) {
            daily[i].0 += din;
            daily[i].1 += dout;
            let p = procs.entry(proc).or_default();
            p.0 += din;
            p.1 += dout;
        }
    }
    let mut top: Vec<(String, (i64, i64))> = procs.into_iter().collect();
    top.sort_by_key(|(n, (i, o))| (std::cmp::Reverse(i + o), n.clone()));
    let first: Option<String> = c
        .query_row("SELECT MIN(day) FROM netday", [], |r| r.get(0))
        .ok()
        .flatten();
    println!(
        "{}",
        json!({
            "first": first,
            "daily": daily.iter().enumerate().map(|(k, d)| json!({"day": day_ts(len, k), "in": d.0, "out": d.1})).collect::<Vec<_>>(),
            "procs": top.iter().take(12).map(|(n, (i, o))| json!({"name": n, "in": i, "out": o, "ai": crate::is_ai_proc(n)})).collect::<Vec<_>>(),
        })
    );
}

pub fn commits(path: &str) {
    let mut by_day: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for d in crate::out_of(
        "git",
        &[
            "-C",
            path,
            "log",
            "--since=371.days",
            "--format=%ad",
            "--date=format-local:%Y-%m-%d",
        ],
    )
    .lines()
    {
        *by_day.entry(d.to_string()).or_default() += 1;
    }
    println!(
        "{}",
        json!({"days": by_day.keys().collect::<Vec<_>>(), "counts": by_day.values().collect::<Vec<_>>()})
    );
}
