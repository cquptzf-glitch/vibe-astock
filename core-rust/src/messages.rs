//! 消息流(「我的」里的盘前提醒、收盘摘要、自选异动)。
//!
//! 🔴 没有后台常驻服务:消息是**打开 App(或点「刷新」)时按当天行情生成**的,不是推送;没开 App 的那段时间不会有消息,
//!    也补不回过去没生成的盘中异动。收盘摘要和盘前提醒按 (类型, 日期) 去重,每天各一条;自选异动每天一条,每次生成覆盖成最新一份。
//! 🔴 内容只列**已经发生的读数**(涨停家数、炸板率、昨日涨停票今日表现、自选里涨跌幅 ≥ 5% 或触及涨跌停的),不评价、不预测、
//!    不给买卖建议。只用自选,**不含持仓、成本、盈亏**。
//! 🔴 取不到的块不生成(不编、不写 0),在返回里的 skipped 写原因。
//! 🔴 消息可以由行情重新生成,不进备份;「清除全部本地数据」会一起清掉。

use crate::{discovery::DiscoveryError, emotion, monitor};
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

pub const MOVE_PCT: f64 = 5.0;
const MESSAGES_MAX: i64 = 400;

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile messages sqlite")
}

pub fn list(db: &Path) -> Result<Value> {
    let conn = open(db)?;
    let mut stmt = conn.prepare("SELECT id, kind, day, title, body, route, read, created_at, updated_at FROM mobile_messages ORDER BY day DESC, updated_at DESC, id DESC LIMIT 200")?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({"id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "day": r.get::<_, String>(2)?, "title": r.get::<_, String>(3)?, "body": r.get::<_, String>(4)?,
            "route": r.get::<_, String>(5)?, "read": r.get::<_, i64>(6)? != 0, "created_at": r.get::<_, String>(7)?, "updated_at": r.get::<_, String>(8)?}))
    })?;
    let messages = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let unread = messages.iter().filter(|m| m["read"] == false).count();
    Ok(json!({"messages": messages, "unread": unread}))
}

/// `{action:"read", id}`、`{action:"read_all"}`、`{action:"delete", id}`、`{action:"clear"}`
pub fn act(db: &Path, body: &Value) -> Result<Value> {
    let conn = open(db)?;
    match body.get("action").and_then(Value::as_str) {
        Some("read") => {
            let id = body.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow::anyhow!("要给出消息 id"))?;
            conn.execute("UPDATE mobile_messages SET read = 1 WHERE id = ?1", [id])?;
        }
        Some("read_all") => {
            conn.execute("UPDATE mobile_messages SET read = 1", [])?;
        }
        Some("delete") => {
            let id = body.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow::anyhow!("要给出消息 id"))?;
            conn.execute("DELETE FROM mobile_messages WHERE id = ?1", [id])?;
        }
        Some("clear") => {
            conn.execute("DELETE FROM mobile_messages", [])?;
        }
        _ => return Err(anyhow::anyhow!("action 只能是 read / read_all / delete / clear")),
    }
    list(db)
}

/// 写一条消息:replace=false 时同 (kind, day) 已有就不动(返回 false);replace=true 覆盖正文并重新标未读
pub(crate) fn put(conn: &Connection, kind: &str, day: &str, title: &str, body: &str, route: &str, replace: bool) -> Result<bool> {
    let now = Utc::now().to_rfc3339();
    let exists: i64 = conn.query_row("SELECT COUNT(*) FROM mobile_messages WHERE kind = ?1 AND day = ?2", params![kind, day], |r| r.get(0))?;
    if exists > 0 {
        if !replace {
            return Ok(false);
        }
        conn.execute("UPDATE mobile_messages SET title = ?3, body = ?4, route = ?5, read = 0, updated_at = ?6 WHERE kind = ?1 AND day = ?2", params![kind, day, title, body, route, now])?;
        return Ok(true);
    }
    conn.execute("INSERT INTO mobile_messages(kind, day, title, body, route, read, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)", params![kind, day, title, body, route, now])?;
    // 只留最近 MESSAGES_MAX 条
    conn.execute("DELETE FROM mobile_messages WHERE id NOT IN (SELECT id FROM mobile_messages ORDER BY day DESC, id DESC LIMIT ?1)", [MESSAGES_MAX])?;
    Ok(true)
}

fn pct(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn num(v: &Value) -> String {
    match v.as_f64() {
        Some(x) if (x - x.round()).abs() < 1e-9 => format!("{}", x.round() as i64),
        Some(x) => format!("{x:.1}"),
        None => "—".into(),
    }
}

fn rate(v: &Value) -> String {
    pct(v).map(|x| format!("{:.1}%", x * 100.0)).unwrap_or_else(|| "—".into())
}

/// 自选里涨跌幅 ≥ MOVE_PCT 或触及涨跌停的(只看当天有报价的)。返回 (名称或代码, 描述)
pub(crate) fn movers(watchlist: &[Value], only_day: Option<&str>) -> Vec<(String, String)> {
    let mut rows: Vec<(f64, String, String)> = Vec::new();
    for w in watchlist {
        if w["available"] != true {
            continue;
        }
        if let Some(day) = only_day {
            if w["quote_date"].as_str() != Some(day) {
                continue;
            }
        } else if w["is_today"] != true {
            continue;
        }
        let (Some(change), Some(price)) = (w["change_pct"].as_f64(), w["price"].as_f64()) else { continue };
        let at_up = w["limit_up"].as_f64().is_some_and(|l| price >= l - 1e-6);
        let at_down = w["limit_down"].as_f64().is_some_and(|l| price <= l + 1e-6);
        if change.abs() < MOVE_PCT && !at_up && !at_down {
            continue;
        }
        let name = w["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(w["symbol"].as_str().unwrap_or("")).to_string();
        let tag = if at_up { "涨停" } else if at_down { "跌停" } else { "" };
        rows.push((change, name, format!("{} {}{:+.2}%{}", w["symbol"].as_str().unwrap_or(""), if tag.is_empty() { "" } else { "" }, change, if tag.is_empty() { String::new() } else { format!("({tag})") })));
    }
    rows.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap_or(std::cmp::Ordering::Equal));
    rows.into_iter().map(|(_, n, d)| (n, d)).collect()
}

/// 情绪复盘读数 → 几行文字(只列已发生的数)
pub(crate) fn emotion_lines(emo: &Value) -> Vec<String> {
    let s = &emo["summary"];
    let mut lines = vec![format!("涨停 {} 家 · 炸板 {} · 跌停 {} · 最高 {} 板 · 炸板率 {}", num(&s["limit_up"]), num(&s["broken"]), num(&s["limit_down"]), num(&s["highest"]), rate(&s["broken_rate"]))];
    let m = &emo["money_effect"];
    if m["available"] == true {
        lines.push(format!("昨日涨停股今日:平均 {}%、翻红率 {}、再度涨停率 {}", num(&m["avg"]), rate(&m["positive_rate"]), rate(&m["limit_up_again_rate"])));
    }
    let p = &emo["promotion"];
    if p["available"] == true {
        lines.push(format!("整体晋级率 {}", rate(&p["overall"]["rate"])));
    }
    lines
}

pub(crate) fn close_message(day: &str, emo: &Value, movers: &[(String, String)]) -> (String, String) {
    let mut lines = emotion_lines(emo);
    if !movers.is_empty() {
        lines.push(format!("自选里涨跌幅 ≥ {MOVE_PCT}% 或触及涨跌停:{}", movers.iter().take(8).map(|(n, d)| format!("{n} {d}")).collect::<Vec<_>>().join(";")));
    }
    lines.push("只列已经发生的读数,不构成买卖建议。".into());
    (format!("{day} 收盘摘要"), lines.join("\n"))
}

pub(crate) fn pre_open_message(day: &str, prev: &str, emo: &Value) -> (String, String) {
    let mut lines = vec![format!("上一交易日 {prev} 的情绪读数:")];
    lines.extend(emotion_lines(emo));
    lines.push("开盘前可以回看「明日验证」里你昨晚写下的条件,开盘后在「竞价与盘中」对照。只列已经发生的读数,不构成买卖建议。".into());
    (format!("{day} 盘前提醒"), lines.join("\n"))
}

pub(crate) fn move_message(day: &str, movers: &[(String, String)]) -> (String, String) {
    let body = movers.iter().take(12).map(|(n, d)| format!("{n} {d}")).collect::<Vec<_>>().join("\n");
    (format!("{day} 自选异动({} 只)", movers.len()), format!("{body}\n阈值:涨跌幅 ≥ {MOVE_PCT}% 或触及涨跌停;只在打开 App 时取一次行情,不是实时推送。"))
}

pub async fn generate(client: &Client, db: &Path) -> Result<Value, DiscoveryError> {
    let now = emotion::now_china();
    let today = now.0.format("%Y-%m-%d").to_string();
    let hm = now.1;
    let mut created: Vec<&str> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let calendar = match emotion::trade_dates(client).await {
        Ok(c) => c,
        Err(e) => {
            skipped.push(json!({"kind": "all", "why": format!("没取到交易日历:{e}")}));
            return Ok(json!({"created": created, "skipped": skipped, "list": list(db).map_err(DiscoveryError::Upstream)?}));
        }
    };
    let trading = calendar.iter().any(|d| *d == today);
    let (last_day, prev_day) = emotion::review_day(&calendar, None, now).map_err(|e| DiscoveryError::Upstream(anyhow::anyhow!("{e:?}")))?;
    let conn = open(db).map_err(DiscoveryError::Upstream)?;
    let wl = monitor::snapshot(client, db).await.ok();
    let watch_rows: Vec<Value> = wl.as_ref().and_then(|v| v["watchlist"].as_array().cloned()).unwrap_or_default();

    // 收盘摘要:最近一个已收盘交易日,每天一条
    let have_close: i64 = conn.query_row("SELECT COUNT(*) FROM mobile_messages WHERE kind = 'close_summary' AND day = ?1", [&last_day], |r| r.get(0)).map_err(|e| DiscoveryError::Upstream(e.into()))?;
    let mut emo_cache: Option<Value> = None;
    if have_close == 0 {
        match emotion::emotion(client, &format!("/mobile/shortline/emotion?date={last_day}")).await {
            Ok(emo) if emo["status"] != "failed" && emo["summary"].is_object() => {
                let m = movers(&watch_rows, Some(&last_day));
                let (title, body) = close_message(&last_day, &emo, &m);
                if put(&conn, "close_summary", &last_day, &title, &body, "emotion", false).map_err(DiscoveryError::Upstream)? {
                    created.push("close_summary");
                }
                emo_cache = Some(emo);
            }
            Ok(emo) => skipped.push(json!({"kind": "close_summary", "why": emo["message"].as_str().unwrap_or("情绪复盘数据没取全")})),
            Err(e) => skipped.push(json!({"kind": "close_summary", "why": format!("{e:?}")})),
        }
    }
    // 盘前提醒:交易日 09:30 前,当天一条,用上一交易日的读数
    if trading && hm < 930 {
        let have: i64 = conn.query_row("SELECT COUNT(*) FROM mobile_messages WHERE kind = 'pre_open' AND day = ?1", [&today], |r| r.get(0)).map_err(|e| DiscoveryError::Upstream(e.into()))?;
        if have == 0 {
            let emo = match emo_cache.take() {
                Some(e) => Ok(e),
                None => emotion::emotion(client, &format!("/mobile/shortline/emotion?date={last_day}")).await,
            };
            match emo {
                Ok(emo) if emo["summary"].is_object() => {
                    let (title, body) = pre_open_message(&today, &last_day, &emo);
                    if put(&conn, "pre_open", &today, &title, &body, "verify", false).map_err(DiscoveryError::Upstream)? {
                        created.push("pre_open");
                    }
                }
                Ok(emo) => skipped.push(json!({"kind": "pre_open", "why": emo["message"].as_str().unwrap_or("情绪复盘数据没取全")})),
                Err(e) => skipped.push(json!({"kind": "pre_open", "why": format!("{e:?}")})),
            }
        }
    }
    let _ = prev_day;
    // 自选异动:交易日盘中(09:30–15:05),每天一条、每次覆盖
    if trading && (930..=1505).contains(&hm) {
        if wl.is_none() {
            skipped.push(json!({"kind": "watch_move", "why": "没取到自选行情"}));
        } else {
            let m = movers(&watch_rows, Some(&today));
            if m.is_empty() {
                skipped.push(json!({"kind": "watch_move", "why": format!("自选里没有涨跌幅 ≥ {MOVE_PCT}% 或触及涨跌停的(不生成空消息)")}));
            } else {
                let (title, body) = move_message(&today, &m);
                if put(&conn, "watch_move", &today, &title, &body, "monitor", true).map_err(DiscoveryError::Upstream)? {
                    created.push("watch_move");
                }
            }
        }
    }
    Ok(json!({"created": created, "skipped": skipped, "list": list(db).map_err(DiscoveryError::Upstream)?}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-messages-{name}-{nonce}.sqlite3"));
        crate::storage::init(&path).unwrap();
        crate::journal::init(&path).unwrap();
        crate::daily_review::init(&path).unwrap();
        crate::ai_chat::init(&path).unwrap();
        crate::ai_config::init(&path).unwrap();
        crate::schema::migrate(&path).unwrap();
        path
    }

    fn emo() -> Value {
        json!({"status": "ok", "summary": {"limit_up": 61, "broken": 49, "limit_down": 10, "highest": 11, "broken_rate": 0.445},
            "money_effect": {"available": true, "avg": -1.32, "positive_rate": 0.351, "limit_up_again_rate": 0.189},
            "promotion": {"available": true, "overall": {"rate": 0.167}}})
    }

    fn row(symbol: &str, name: &str, price: f64, change: f64, day: &str) -> Value {
        json!({"symbol": symbol, "name": name, "price": price, "change_pct": change, "limit_up": 11.0, "limit_down": 9.0, "quote_date": day, "is_today": true, "available": true})
    }

    #[test]
    fn emotion_lines_only_list_readings_and_skip_unavailable_blocks() {
        let lines = emotion_lines(&emo());
        assert_eq!(lines[0], "涨停 61 家 · 炸板 49 · 跌停 10 · 最高 11 板 · 炸板率 44.5%");
        assert_eq!(lines[1], "昨日涨停股今日:平均 -1.3%、翻红率 35.1%、再度涨停率 18.9%");
        assert_eq!(lines[2], "整体晋级率 16.7%");
        let mut thin = emo();
        thin["money_effect"] = json!({"available": false, "reason": "x"});
        thin["promotion"] = json!({"available": false, "reason": "y"});
        assert_eq!(emotion_lines(&thin).len(), 1, "不可用的块不写、不写成 0");
    }

    #[test]
    fn movers_need_five_percent_or_a_limit_and_today_s_quote_and_never_include_positions() {
        let rows = vec![
            row("600519", "样本白酒", 10.5, 5.0, "2026-10-16"),
            row("000001", "样本银行", 10.0, 1.2, "2026-10-16"),
            row("300308", "样本光模块", 11.0, 10.0, "2026-10-16"),
            row("688001", "样本新股", 9.0, -10.0, "2026-10-16"),
            row("601318", "样本旧价", 10.0, 8.0, "2026-10-15"),
            json!({"symbol": "920575", "name": "无报价", "available": false}),
        ];
        let m = movers(&rows, Some("2026-10-16"));
        let names: Vec<&str> = m.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, ["样本光模块", "样本新股", "样本白酒"], "按涨跌幅绝对值从大到小;1.2% 和旧日期、无报价的都不进");
        assert!(m[0].1.contains("(涨停)") && m[1].1.contains("(跌停)") && !m[2].1.contains("("));
        assert!(m[0].1.contains("300308") && m[0].1.contains("+10.00%"));
        let (title, body) = close_message("2026-10-16", &emo(), &m);
        assert_eq!(title, "2026-10-16 收盘摘要");
        assert!(body.contains("自选里涨跌幅 ≥ 5% 或触及涨跌停:样本光模块") && body.ends_with("不构成买卖建议。"));
        let (_, none_body) = close_message("2026-10-16", &emo(), &[]);
        assert!(!none_body.contains("自选里"), "没有异动就不写这一行");
        let (t2, b2) = pre_open_message("2026-10-19", "2026-10-16", &emo());
        assert_eq!(t2, "2026-10-19 盘前提醒");
        assert!(b2.starts_with("上一交易日 2026-10-16 的情绪读数:") && b2.contains("明日验证"));
        let (t3, b3) = move_message("2026-10-16", &m);
        assert_eq!(t3, "2026-10-16 自选异动(3 只)");
        assert!(b3.contains("不是实时推送"));
    }

    #[test]
    fn put_dedupes_per_kind_and_day_replace_resets_unread_and_old_messages_are_trimmed() {
        let path = db("put");
        let conn = Connection::open(&path).unwrap();
        assert!(put(&conn, "close_summary", "2026-10-16", "t", "b1", "emotion", false).unwrap());
        assert!(!put(&conn, "close_summary", "2026-10-16", "t", "b2", "emotion", false).unwrap(), "同一天同类型不重复");
        assert_eq!(list(&path).unwrap()["messages"][0]["body"], "b1");
        act(&path, &json!({"action": "read_all"})).unwrap();
        assert_eq!(list(&path).unwrap()["unread"], 0);
        assert!(put(&conn, "watch_move", "2026-10-16", "t", "m1", "monitor", true).unwrap());
        assert!(put(&conn, "watch_move", "2026-10-16", "t", "m2", "monitor", true).unwrap(), "异动每次覆盖");
        let l = list(&path).unwrap();
        let wm = l["messages"].as_array().unwrap().iter().find(|m| m["kind"] == "watch_move").unwrap();
        assert_eq!((wm["body"].clone(), wm["read"].clone()), (json!("m2"), json!(false)), "覆盖后重新标未读");
        assert_eq!(l["unread"], 1);
        let id = wm["id"].as_i64().unwrap();
        act(&path, &json!({"action": "read", "id": id})).unwrap();
        assert_eq!(list(&path).unwrap()["unread"], 0);
        act(&path, &json!({"action": "delete", "id": id})).unwrap();
        assert_eq!(list(&path).unwrap()["messages"].as_array().unwrap().len(), 1);
        assert!(act(&path, &json!({"action": "x"})).is_err());
        assert!(act(&path, &json!({"action": "read"})).is_err());
        act(&path, &json!({"action": "clear"})).unwrap();
        assert_eq!(list(&path).unwrap()["messages"].as_array().unwrap().len(), 0);
        for d in 0..(MESSAGES_MAX + 10) {
            put(&conn, "close_summary", &format!("2020-01-{:02}-{d}", (d % 28) + 1), "t", "b", "", false).unwrap();
        }
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM mobile_messages", [], |r| r.get(0)).unwrap();
        assert_eq!(count, MESSAGES_MAX);
        let _ = std::fs::remove_file(path);
    }
}
