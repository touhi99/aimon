use chrono::{Local, NaiveDate};
use rusqlite::Connection;
use serde_json::json;

pub fn daily(c: &Connection, days: i64) -> Vec<i64> {
    let days = days.clamp(1, 366);
    let len = usize::try_from(days).expect("days is positive");
    let today = Local::now().date_naive();
    let mut out = vec![0i64; len];
    let mut st = c
        .prepare(
            "SELECT date(ts, 'unixepoch', 'localtime'), SUM(inp + out + cread + cw5 + cw1)
             FROM calls WHERE ts >= ? GROUP BY 1",
        )
        .expect("prepare query");
    let rows = st
        .query_map([crate::day_start(days - 1)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .expect("run query");
    for (day, tok) in rows.map(|r| r.expect("decode query row")) {
        let Ok(d) = NaiveDate::parse_from_str(&day, "%Y-%m-%d") else {
            continue;
        };
        let Ok(back) = usize::try_from((today - d).num_days()) else {
            continue;
        };
        if back < len {
            out[len - 1 - back] += tok;
        }
    }
    out
}

pub fn report(c: &Connection, days: i64) {
    println!("{}", json!({ "daily": daily(c, days) }));
}
