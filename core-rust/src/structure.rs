//! 结构变化:图上这个断点,是市场变了、数据源变了,还是规则变了(移植自 orchestrator/src/finance/shortline/structure.ts,
//! 口径来自 Vibe AStock v2.0.0 duanxian/drift.py)。三类分开看:
//!   ① 数据源:四张池子的上游字段清单变没变(变了 = 源改了,跟市场无关);
//!   ② 市场:最近 10 个交易日 vs 之前 20 个交易日的涨停结构(涨停家数、各涨跌幅制度占比),一律比中位数;
//!   ③ 制度:**只收人工登记的**。从数据反推「这天大概改了规则」是猜,猜错会把市场波动当成制度事件。
//! 只报「这里不连续」,不报「所以该怎么改打法」。
//!
//! 与桌面版的差异:历史靠本机缓存越积越长(东财只留一个月左右);窗口按交易日固定,取不到或没收盘的日子从各自窗口里剔除;
//! 字段清单变了会列出新增和消失的字段(清单直接存全,不做摘要);制度事件可以登记未来日期,标「还没施行」。
//! 制度事件是用户自己登记的口径,进备份;每日结构读数是公开行情缓存,不进备份。

use crate::{
    discovery::DiscoveryError,
    emotion::{self, PoolKind, Row},
    emotion_history,
};
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use futures_util::{stream, StreamExt};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

const RECENT: usize = 10;
const PRIOR: usize = 20;
/// 计数类:相对变化到这个比例才算移位(宁可少报)
const SHIFT_RATIO: f64 = 0.35;
/// 占比类(0~1):绝对变化到这个值才算移位,免得小基数被放大
const SHIFT_ABS: f64 = 0.1;
/// 两个窗口各自至少要这么多天才比
const MIN_DAYS: usize = 5;
const CONCURRENCY: usize = 3;
pub(crate) const EVENTS_MAX: usize = 1000;
const TITLE_MAX: usize = 60;
const NOTE_MAX: usize = 300;
const SOURCE: &str =
    "东财涨停 / 炸板 / 跌停 / 昨日涨停池(每天一次,已收盘的日子存本机)+ 人工登记的制度事件";
const NOTE: &str = "结构移位不等于制度变化 —— 也可能是数据源改了字段(看字段清单),或就是市场真的变了。三类分开看,别把前两类读成第三类。";
const REGIME_NOTE: &str = "制度变化只收人工登记的条目,系统不从数据反推「这天大概改了规则」—— 猜错会把市场波动当成制度事件,宁可留空。";
const METRICS: [(&str, &str, &str); 4] = [
    ("limit_up", "涨停家数", "count"),
    ("share_20cm", "20% 制度占比", "share"),
    ("share_10cm", "10% 制度占比", "share"),
    ("share_bj", "北交所占比", "share"),
];
const POOLS: [(&str, &str, PoolKind); 4] = [
    ("zt", "涨停池", PoolKind::LimitUp),
    ("zb", "炸板池", PoolKind::Broken),
    ("dt", "跌停池", PoolKind::LimitDown),
    ("yzt", "昨日涨停池", PoolKind::Yesterday),
];

// ---------------------------------------------------------------- 一天的结构

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct DayStructure {
    pub date: String,
    pub values: BTreeMap<String, Option<f64>>,
    /// 池子 → 字段清单(逗号连接)
    pub fields: BTreeMap<String, String>,
    /// 池子 → 看不出字段的原因
    #[serde(default)]
    pub unknown: BTreeMap<String, String>,
}

/// 这只票在 `day` 属于哪一类:北交所 / 20% / 10%(含主板 ST 的 5%)
pub(crate) fn mix_of(row: &Row, day: &str) -> &'static str {
    if emotion::board_of(&row.code, &row.name) == "北交所" {
        "北交所"
    } else if emotion::limit_pct(&row.code, &row.name, day) >= 20.0 {
        "20%"
    } else {
        "10%"
    }
}

pub(crate) fn day_structure(
    date: &str,
    zt: &[Row],
    fields: &BTreeMap<String, Option<Vec<String>>>,
    unknown: BTreeMap<String, String>,
) -> DayStructure {
    let n = zt.len() as f64;
    let count = |k: &str| zt.iter().filter(|r| mix_of(r, date) == k).count() as f64;
    let share = |k: &str| (n > 0.0).then(|| (count(k) / n * 10_000.0).round() / 10_000.0);
    let mut values = BTreeMap::new();
    values.insert("limit_up".to_string(), Some(n));
    values.insert("share_20cm".into(), share("20%"));
    values.insert("share_10cm".into(), share("10%"));
    values.insert("share_bj".into(), share("北交所"));
    DayStructure {
        date: date.into(),
        values,
        fields: fields
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|f| (k.clone(), f.join(","))))
            .collect(),
        unknown,
    }
}

async fn fetch_day(client: &Client, date: &str) -> Result<DayStructure, String> {
    let compact = date.replace('-', "");
    let roots = futures_util::future::join_all(
        POOLS
            .iter()
            .map(|(_, _, kind)| emotion::fetch_pool_root(client, *kind, &compact)),
    )
    .await;
    let mut fields: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();
    let mut unknown: BTreeMap<String, String> = BTreeMap::new();
    let mut zt: Option<Vec<Row>> = None;
    for ((key, label, kind), root) in POOLS.iter().zip(roots) {
        match root {
            Err(e) => {
                unknown.insert(
                    key.to_string(),
                    format!("没取到({})", emotion::error_message(label, &e)),
                );
                fields.insert(key.to_string(), None);
                if *key == "zt" {
                    return Err(format!(
                        "东财涨停池没取到({})",
                        emotion::error_message(label, &e)
                    ));
                }
            }
            Ok(root) => {
                if *key == "zt" {
                    match emotion::parse_pool(&root, *kind, &compact) {
                        Ok(rows) if !rows.is_empty() => zt = Some(rows),
                        Ok(_) => return Err("东财涨停池为空(数据还没出,或超过东财保留期)".into()),
                        Err(e) => return Err(format!("东财涨停池解析失败({e})")),
                    }
                }
                match emotion::raw_fields(&root) {
                    Some(f) => {
                        fields.insert(key.to_string(), Some(f));
                    }
                    None => {
                        unknown.insert(key.to_string(), "池子为空,看不出字段".into());
                        fields.insert(key.to_string(), None);
                    }
                }
            }
        }
    }
    let zt = zt.ok_or_else(|| "东财涨停池没取到".to_string())?;
    Ok(day_structure(date, &zt, &fields, unknown))
}

fn cache_load(db: &Path, date: &str) -> Option<DayStructure> {
    let conn = Connection::open(db).ok()?;
    let payload: String = conn
        .query_row(
            "SELECT payload FROM mobile_structure_days WHERE date = ?1",
            [date],
            |r| r.get(0),
        )
        .ok()?;
    serde_json::from_str::<DayStructure>(&payload)
        .ok()
        .filter(|d| d.date == date)
}

fn cache_store(db: &Path, day: &DayStructure) -> Result<()> {
    Connection::open(db)?.execute(
        "INSERT INTO mobile_structure_days(date, payload, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![day.date, serde_json::to_string(day)?, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- 两个窗口比较(纯计算)

fn median(values: &[f64]) -> f64 {
    emotion::median(values)
}

fn round(v: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (v * m).round() / m
}

/// 两个窗口逐项比中位数;任一窗口不足 MIN_DAYS 天的那项不比(列进 skipped)。一律中位数:一两天涨停潮或大面就能把均值拉飞
pub(crate) fn compare_windows(
    recent: &[&DayStructure],
    prior: &[&DayStructure],
) -> (Vec<Value>, Vec<String>) {
    let (mut metrics, mut skipped) = (Vec::new(), Vec::new());
    for (key, label, kind) in METRICS {
        let pick = |days: &[&DayStructure]| -> Vec<f64> {
            days.iter()
                .filter_map(|d| d.values.get(key).copied().flatten())
                .collect()
        };
        let (r, p) = (pick(recent), pick(prior));
        if r.len() < MIN_DAYS || p.len() < MIN_DAYS {
            skipped.push(format!(
                "{label}(最近 {} 天 / 之前 {} 天)",
                r.len(),
                p.len()
            ));
            continue;
        }
        let (rv, pv) = (median(&r), median(&p));
        let rel = (kind == "count" && pv != 0.0).then(|| (rv - pv) / pv);
        let shifted = if kind == "share" {
            (rv - pv).abs() >= SHIFT_ABS - 1e-9
        } else {
            rel.is_some_and(|x| x.abs() >= SHIFT_RATIO - 1e-9)
        };
        metrics.push(json!({
            "metric": key, "label": label, "kind": kind, "recent": round(rv, 4), "prior": round(pv, 4), "delta": round(rv - pv, 4),
            "rel_change": rel.map(|x| round(x, 3)), "shifted": shifted, "recent_days": r.len(), "prior_days": p.len(),
        }));
    }
    (metrics, skipped)
}

/// 按日期顺序看每张池子的字段清单变过几次
pub(crate) fn field_drift(days: &[&DayStructure]) -> Vec<Value> {
    let mut sorted: Vec<&&DayStructure> = days.iter().collect();
    sorted.sort_by(|a, b| a.date.cmp(&b.date));
    POOLS
        .iter()
        .map(|(pool, label, _)| {
            let seen: Vec<(&str, &str)> = sorted.iter().filter_map(|d| d.fields.get(*pool).map(|f| (d.date.as_str(), f.as_str()))).collect();
            let mut versions: Vec<(String, String, String)> = Vec::new(); // since, until, fields
            for (date, f) in &seen {
                match versions.last_mut() {
                    Some(last) if last.2 == *f => last.1 = date.to_string(),
                    _ => versions.push((date.to_string(), date.to_string(), f.to_string())),
                }
            }
            let changed = versions.len() > 1;
            let split = |s: &str| s.split(',').filter(|x| !x.is_empty()).map(String::from).collect::<Vec<_>>();
            let (mut added, mut removed): (Option<Vec<String>>, Option<Vec<String>>) = (None, None);
            if changed {
                let a = split(&versions[versions.len() - 2].2);
                let b = split(&versions[versions.len() - 1].2);
                added = Some(b.iter().filter(|x| !a.contains(x)).cloned().collect());
                removed = Some(a.iter().filter(|x| !b.contains(x)).cloned().collect());
            }
            let span = match (seen.first(), seen.last()) {
                (Some(f), Some(l)) => format!(";只覆盖 {} 至 {},不能证明之外的日子字段没变", f.0, l.0),
                _ => String::new(),
            };
            let note = if changed {
                format!("字段清单在 {} 变过 —— 跨这一天的统计要小心,断点可能来自数据源而不是市场", versions.last().map(|v| v.0.as_str()).unwrap_or(""))
            } else if seen.len() >= 2 {
                format!("字段清单稳定({} 天)", seen.len())
            } else {
                format!("还检不出:只有 {} 天看得到字段,至少要 2 天", seen.len())
            };
            json!({
                "pool": pool, "label": label, "days": seen.len(), "changed": changed, "added": added, "removed": removed, "detectable": seen.len() >= 2,
                "versions": versions.iter().map(|(s, u, f)| json!({"since": s, "until": u, "count": f.split(',').filter(|x| !x.is_empty()).count(), "fields": f})).collect::<Vec<_>>(),
                "note": format!("{note}{span}"),
            })
        })
        .collect()
}

pub(crate) fn summarize(field_changed: &[String], structure: &Value, events: &[Value]) -> String {
    let mut bits: Vec<String> = Vec::new();
    if !field_changed.is_empty() {
        bits.push(format!(
            "{} 张池子的上游字段变过({})—— 跨那些日期的统计里可能有一个跟市场无关的断点",
            field_changed.len(),
            field_changed.join("、")
        ));
    }
    if structure["available"] == true {
        let shifted: Vec<&str> = structure["shifted"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["label"].as_str())
            .collect();
        bits.push(if shifted.is_empty() {
            "结构比较:没有明显移位".into()
        } else {
            format!("结构比较:{}明显移位", shifted.join("、"))
        });
    } else {
        bits.push(format!(
            "结构比较暂不可用({})",
            structure["reason"].as_str().unwrap_or("")
        ));
    }
    let upcoming = events.iter().filter(|e| e["upcoming"] == true).count();
    let past = events.len() - upcoming;
    bits.push(if events.is_empty() {
        "还没登记制度事件(要人工登记,系统不自动推断)".into()
    } else {
        format!(
            "已登记 {past} 条制度事件{}",
            if upcoming > 0 {
                format!(",另有 {upcoming} 条还没施行")
            } else {
                String::new()
            }
        )
    });
    format!("{}。", bits.join(";"))
}

// ---------------------------------------------------------------- 制度事件(用户数据,进备份)

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Event {
    pub id: i64,
    pub date: String,
    pub title: String,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile structure sqlite")
}

pub(crate) fn read_events(conn: &Connection) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare("SELECT id, date, title, note, created_at, updated_at FROM mobile_regime_events ORDER BY date, title, id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Event {
            id: r.get(0)?,
            date: r.get(1)?,
            title: r.get(2)?,
            note: r.get(3)?,
            created_at: r.get(4)?,
            updated_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub(crate) fn write_event(conn: &Connection, e: &Event) -> Result<()> {
    conn.execute(
        "INSERT INTO mobile_regime_events(id, date, title, note, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![e.id, e.date, e.title, e.note, e.created_at, e.updated_at],
    )?;
    Ok(())
}

/// 一条制度事件哪里不对;没问题返回 None。超长直接拒收,不截断(截断等于替用户改了登记内容)
pub(crate) fn event_problem(e: &Event) -> Option<String> {
    if NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").is_err() {
        return Some(format!("制度事件日期无效:{}", e.date));
    }
    if e.title.trim().is_empty() {
        return Some("制度事件要有标题".into());
    }
    if e.title.chars().count() > TITLE_MAX {
        return Some(format!(
            "制度事件标题最多 {TITLE_MAX} 字(现在 {} 字)",
            e.title.chars().count()
        ));
    }
    if e.note.chars().count() > NOTE_MAX {
        return Some(format!(
            "制度事件说明最多 {NOTE_MAX} 字(现在 {} 字)",
            e.note.chars().count()
        ));
    }
    None
}

fn events_view(db: &Path, today: &str) -> Result<Vec<Value>> {
    Ok(read_events(&open(db)?)?.into_iter().map(|e| json!({"id": e.id, "date": e.date, "title": e.title, "note": e.note, "upcoming": e.date.as_str() > today})).collect())
}

/// 登记 / 修改 / 删除制度事件。`{action:"save", id?, date, title, note?}`、`{action:"delete", id}`
pub fn act(db: &Path, body: &Value) -> Result<Value, DiscoveryError> {
    let now = Utc::now().to_rfc3339();
    let today = emotion::now_china().0.format("%Y-%m-%d").to_string();
    let conn = open(db).map_err(DiscoveryError::Upstream)?;
    let bad = |m: String| DiscoveryError::BadRequest(m);
    match body.get("action").and_then(Value::as_str) {
        Some("save") => {
            let text = |k: &str| {
                body.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            let mut event = Event {
                id: 0,
                date: text("date"),
                title: text("title"),
                note: text("note"),
                created_at: now.clone(),
                updated_at: now,
            };
            if let Some(p) = event_problem(&event) {
                return Err(bad(p));
            }
            let id = body.get("id").and_then(Value::as_i64);
            match id {
                Some(id) => {
                    let n = conn
                        .execute("UPDATE mobile_regime_events SET date = ?2, title = ?3, note = ?4, updated_at = ?5 WHERE id = ?1", params![id, event.date, event.title, event.note, event.updated_at])
                        .map_err(|e| DiscoveryError::Upstream(e.into()))?;
                    if n == 0 {
                        return Err(bad("没有这条制度事件(可能已经删了)".into()));
                    }
                }
                None => {
                    let count: i64 = conn
                        .query_row("SELECT COUNT(*) FROM mobile_regime_events", [], |r| {
                            r.get(0)
                        })
                        .map_err(|e| DiscoveryError::Upstream(e.into()))?;
                    if count as usize >= EVENTS_MAX {
                        return Err(bad(format!("制度事件最多 {EVENTS_MAX} 条")));
                    }
                    conn.execute(
                        "INSERT INTO mobile_regime_events(date, title, note, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![event.date, event.title, event.note, event.created_at, event.updated_at],
                    )
                    .map_err(|e| DiscoveryError::Upstream(e.into()))?;
                    event.id = conn.last_insert_rowid();
                }
            }
        }
        Some("delete") => {
            let id = body
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| bad("要给出制度事件的 id".into()))?;
            if conn
                .execute("DELETE FROM mobile_regime_events WHERE id = ?1", [id])
                .map_err(|e| DiscoveryError::Upstream(e.into()))?
                == 0
            {
                return Err(bad("没有这条制度事件(可能已经删了)".into()));
            }
        }
        _ => return Err(bad("action 只能是 save 或 delete".into())),
    }
    Ok(json!({"events": events_view(db, &today).map_err(DiscoveryError::Upstream)?}))
}

// ---------------------------------------------------------------- 入口

pub async fn fetch(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let refresh = match crate::discovery::parameter(path, "refresh").as_deref() {
        None => false,
        Some("1") => true,
        Some(_) => return Err(DiscoveryError::BadRequest("refresh 只能是 1".into())),
    };
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, _) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    let today = now.0.format("%Y-%m-%d").to_string();
    let mut gaps: Vec<String> = Vec::new();
    let upto: Vec<&String> = calendar
        .iter()
        .filter(|d| d.as_str() <= date.as_str())
        .collect();
    let window: Vec<String> = upto[upto.len().saturating_sub(RECENT + PRIOR)..]
        .iter()
        .map(|d| (*d).clone())
        .collect();
    if window.len() < RECENT + PRIOR {
        gaps.push(format!(
            "交易日历只列得出 {} 个交易日(要 {})",
            window.len(),
            RECENT + PRIOR
        ));
    }
    let floor = emotion_history::retention_floor(now);
    let fetched: Vec<(String, Option<DayStructure>, bool, String)> = stream::iter(window.clone())
        .map(|d| {
            let (calendar, floor, target) = (&calendar, &floor, &date);
            async move {
                let settled = emotion::is_settled(&d, calendar, now);
                if !(refresh && d == *target) {
                    if let Some(day) = cache_load(db, &d) {
                        return (d, Some(day), true, String::new());
                    }
                }
                if d.as_str() < floor.as_str() {
                    return (
                        d,
                        None,
                        settled,
                        "超过东财保留期(约一个月),当时也没缓存".into(),
                    );
                }
                match fetch_day(client, &d).await {
                    Ok(day) => {
                        if settled {
                            let _ = cache_store(db, &day);
                        }
                        (d, Some(day), settled, String::new())
                    }
                    Err(e) => (d, None, settled, e),
                }
            }
        })
        .buffered(CONCURRENCY)
        .collect()
        .await;

    let mut usable: BTreeMap<String, DayStructure> = BTreeMap::new();
    let (mut missing, mut unsettled): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for (d, day, settled, _) in &fetched {
        match day {
            None => missing.push(d.clone()),
            Some(_) if !settled => unsettled.push(d.clone()),
            Some(day) => {
                usable.insert(d.clone(), day.clone());
            }
        }
    }
    if let Some((_, _, _, why)) = fetched
        .iter()
        .find(|(d, day, _, _)| *d == date && day.is_none())
    {
        gaps.push(format!(
            "{date} 的涨停结构没取到{}",
            if why.is_empty() {
                String::new()
            } else {
                format!("({why})")
            }
        ));
    }
    let older = missing.iter().filter(|d| **d != date).count();
    if older > 0 {
        gaps.push(format!("{older} 个交易日的涨停结构没取到(东财只留最近一个月左右,更早又没缓存过的日子取不到),已剔除"));
    }
    for d in &unsettled {
        gaps.push(format!("{d} 还没收盘,不进比较"));
    }
    let split = window.len().saturating_sub(RECENT);
    let (prior_dates, recent_dates) = window.split_at(split);
    let pick =
        |ds: &[String]| -> Vec<&DayStructure> { ds.iter().filter_map(|d| usable.get(d)).collect() };
    let (recent, prior) = (pick(recent_dates), pick(prior_dates));
    let info = |dates: &[String], used: &[&DayStructure]| json!({"days": dates.len(), "used": used.len(), "from": used.first().map(|d| d.date.clone()), "to": used.last().map(|d| d.date.clone())});
    let structure = if recent.len() < MIN_DAYS || prior.len() < MIN_DAYS {
        json!({"available": false, "reason": format!("最近 {} 个交易日取到 {} 天、之前 {} 个交易日取到 {} 天,每个窗口至少要 {MIN_DAYS} 天", recent_dates.len(), recent.len(), prior_dates.len(), prior.len()), "metrics": [], "shifted": [], "skipped": []})
    } else {
        let (metrics, skipped) = compare_windows(&recent, &prior);
        if metrics.is_empty() {
            json!({"available": false, "reason": "每一项都不够天数比较", "metrics": [], "shifted": [], "skipped": skipped})
        } else {
            let shifted: Vec<&Value> = metrics.iter().filter(|m| m["shifted"] == true).collect();
            json!({"available": true, "reason": null, "metrics": metrics, "shifted": shifted, "skipped": skipped})
        }
    };
    let all: Vec<&DayStructure> = usable.values().collect();
    let drift = field_drift(&all);
    let changed: Vec<String> = drift
        .iter()
        .filter(|d| d["changed"] == true)
        .filter_map(|d| d["label"].as_str().map(String::from))
        .collect();
    let events = events_view(db, &today).map_err(DiscoveryError::Upstream)?;
    Ok(json!({
        "date": date, "available": structure["available"] == true || drift.iter().any(|d| d["days"].as_u64().unwrap_or(0) > 0) || !events.is_empty(),
        "recent_window": info(recent_dates, &recent), "prior_window": info(prior_dates, &prior), "structure": structure,
        "thresholds": {"share_abs": SHIFT_ABS, "count_rel": SHIFT_RATIO, "min_days": MIN_DAYS},
        "field_drift": drift, "field_changed": changed, "regime_events": events, "regime_note": REGIME_NOTE,
        "missing_days": missing, "unsettled_days": unsettled, "summary": summarize(&changed, &structure, &events), "note": NOTE,
        "generated_at": Utc::now().to_rfc3339(), "gaps": gaps, "source": SOURCE,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, name: &str) -> Row {
        Row {
            code: code.into(),
            name: name.into(),
            ..Row::default()
        }
    }

    fn day(date: &str, limit_up: f64, s20: f64, fields: &[(&str, &str)]) -> DayStructure {
        let mut values = BTreeMap::new();
        values.insert("limit_up".to_string(), Some(limit_up));
        values.insert("share_20cm".into(), Some(s20));
        values.insert("share_10cm".into(), Some(1.0 - s20));
        values.insert("share_bj".into(), Some(0.0));
        DayStructure {
            date: date.into(),
            values,
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn regime_mix_follows_the_rule_of_that_day() {
        // 创业板 2020-08-24 起 20%、之前 10%;主板 ST 2026-07-06 起 10%、之前 5%(都算 10% 制度类);北交所单独一类
        assert_eq!(mix_of(&row("300001", "特锐德"), "2026-10-16"), "20%");
        assert_eq!(mix_of(&row("300001", "特锐德"), "2020-01-02"), "10%");
        assert_eq!(mix_of(&row("688001", "华兴源创"), "2026-10-16"), "20%");
        assert_eq!(mix_of(&row("600001", "ST某某"), "2026-10-16"), "10%");
        assert_eq!(mix_of(&row("920001", "北交样本"), "2026-10-16"), "北交所");
        let zt = vec![
            row("300001", "A"),
            row("300002", "B"),
            row("600001", "C"),
            row("920001", "D"),
        ];
        let d = day_structure("2026-10-16", &zt, &BTreeMap::new(), BTreeMap::new());
        assert_eq!(
            (
                d.values["limit_up"],
                d.values["share_20cm"],
                d.values["share_10cm"],
                d.values["share_bj"]
            ),
            (Some(4.0), Some(0.5), Some(0.25), Some(0.25))
        );
    }

    #[test]
    fn windows_compare_medians_and_skip_short_windows() {
        let mk = |n: usize, lu: f64, s20: f64| -> Vec<DayStructure> {
            (0..n)
                .map(|i| {
                    day(
                        &format!("2026-09-{:02}", i + 1),
                        lu + (i % 2) as f64,
                        s20,
                        &[],
                    )
                })
                .collect()
        };
        let recent_days = mk(10, 60.0, 0.5);
        let prior_days = mk(20, 40.0, 0.35);
        let (r, p): (Vec<&DayStructure>, Vec<&DayStructure>) =
            (recent_days.iter().collect(), prior_days.iter().collect());
        let (metrics, skipped) = compare_windows(&r, &p);
        assert!(skipped.is_empty());
        let by = |k: &str| metrics.iter().find(|m| m["metric"] == k).unwrap();
        // 涨停家数中位数 60.5 vs 40.5 → 相对 +49%,≥ 35% 算移位;20% 制度占比 +0.15 ≥ 0.1 算移位;10% 制度 -0.15 同理
        assert_eq!(
            (
                by("limit_up")["shifted"].clone(),
                by("share_20cm")["shifted"].clone(),
                by("share_10cm")["delta"].clone(),
                by("share_bj")["shifted"].clone()
            ),
            (json!(true), json!(true), json!(-0.15), json!(false))
        );
        assert_eq!(by("limit_up")["rel_change"], round((60.5 - 40.5) / 40.5, 3));
        // 刚好等于门槛算移位(宁可含边界的一端:1e-9 容差);少于 MIN_DAYS 的窗口整项跳过并说明
        let short: Vec<&DayStructure> = recent_days.iter().take(4).collect();
        let (m2, s2) = compare_windows(&short, &p);
        assert!(m2.is_empty() && s2.len() == 4 && s2[0].contains("最近 4 天 / 之前 20 天"));
        // 均值会被一天涨停潮拉飞,中位数不会
        let mut spike = mk(10, 50.0, 0.4);
        spike[0].values.insert("limit_up".into(), Some(400.0));
        let calm = mk(20, 50.0, 0.4);
        let (sp, ca): (Vec<&DayStructure>, Vec<&DayStructure>) =
            (spike.iter().collect(), calm.iter().collect());
        assert_eq!(
            compare_windows(&sp, &ca)
                .0
                .iter()
                .find(|m| m["metric"] == "limit_up")
                .unwrap()["shifted"],
            false
        );
    }

    #[test]
    fn field_drift_reports_when_upstream_fields_changed_and_what_changed() {
        let days = [
            day("2026-10-12", 50.0, 0.3, &[("zt", "c,n,p"), ("zb", "c,n")]),
            day("2026-10-13", 50.0, 0.3, &[("zt", "c,n,p"), ("zb", "c,n")]),
            day(
                "2026-10-14",
                50.0,
                0.3,
                &[("zt", "c,n,p,zbc"), ("zb", "c,n")],
            ),
            day("2026-10-15", 50.0, 0.3, &[("zt", "c,n,p,zbc")]),
        ];
        let refs: Vec<&DayStructure> = days.iter().collect();
        let drift = field_drift(&refs);
        let zt = drift.iter().find(|d| d["pool"] == "zt").unwrap();
        assert_eq!(
            (
                zt["changed"].clone(),
                zt["added"].clone(),
                zt["removed"].clone(),
                zt["versions"].as_array().unwrap().len()
            ),
            (json!(true), json!(["zbc"]), json!([]), 2)
        );
        assert!(zt["note"].as_str().unwrap().contains("2026-10-14 变过"));
        let zb = drift.iter().find(|d| d["pool"] == "zb").unwrap();
        assert_eq!(
            (
                zb["changed"].clone(),
                zb["days"].clone(),
                zb["detectable"].clone()
            ),
            (json!(false), json!(3), json!(true))
        );
        let dt = drift.iter().find(|d| d["pool"] == "dt").unwrap();
        assert_eq!(
            (dt["days"].clone(), dt["detectable"].clone()),
            (json!(0), json!(false))
        );
        assert!(dt["note"].as_str().unwrap().contains("还检不出"));
        assert!(zb["note"]
            .as_str()
            .unwrap()
            .contains("不能证明之外的日子字段没变"));
    }

    #[test]
    fn summary_keeps_the_three_kinds_apart() {
        let unavailable = json!({"available": false, "reason": "最近 3 天"});
        let s = summarize(&["涨停池".to_string()], &unavailable, &[]);
        assert!(
            s.contains("1 张池子的上游字段变过(涨停池)")
                && s.contains("结构比较暂不可用(最近 3 天)")
                && s.contains("还没登记制度事件")
        );
        let ok = json!({"available": true, "shifted": [{"label": "涨停家数"}]});
        let ev = vec![json!({"upcoming": false}), json!({"upcoming": true})];
        let s = summarize(&[], &ok, &ev);
        assert!(
            s.contains("涨停家数明显移位") && s.contains("已登记 1 条制度事件,另有 1 条还没施行")
        );
    }

    #[test]
    fn raw_fields_flatten_nested_keys_and_empty_pools_are_unknown() {
        let root = json!({"data": {"pool": [{"c": "1", "n": "a", "zttj": {"days": 3, "ct": 2}}, {"c": "2", "extra": 1}]}});
        assert_eq!(
            emotion::raw_fields(&root),
            Some(vec![
                "c".to_string(),
                "extra".into(),
                "n".into(),
                "zttj.ct".into(),
                "zttj.days".into()
            ])
        );
        assert_eq!(emotion::raw_fields(&json!({"data": {"pool": []}})), None);
        assert_eq!(emotion::raw_fields(&json!({"data": null})), None);
    }

    #[test]
    fn regime_events_validate_and_round_trip_through_sqlite() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-regime-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let save = |body: Value| act(&db, &body);
        assert!(save(json!({"action": "save", "date": "2026-13-40", "title": "x"})).is_err());
        assert!(format!(
            "{:?}",
            save(json!({"action": "save", "date": "2026-10-16", "title": "  "})).unwrap_err()
        )
        .contains("标题"));
        assert!(format!(
            "{:?}",
            save(json!({"action": "save", "date": "2026-10-16", "title": "字".repeat(61)}))
                .unwrap_err()
        )
        .contains("最多 60 字"));
        assert!(format!("{:?}", save(json!({"action": "save", "date": "2026-10-16", "title": "t", "note": "字".repeat(301)})).unwrap_err()).contains("最多 300 字"));
        let v = save(json!({"action": "save", "date": "2026-07-06", "title": "主板 ST 涨跌幅放宽到 10%", "note": "交易所公告"})).unwrap();
        let id = v["events"][0]["id"].as_i64().unwrap();
        save(json!({"action": "save", "date": "2999-01-01", "title": "未来的规则"})).unwrap();
        let events = events_view(&db, "2026-10-17").unwrap();
        assert_eq!(
            (events[0]["upcoming"].clone(), events[1]["upcoming"].clone()),
            (json!(false), json!(true)),
            "未来日期标「还没施行」"
        );
        let v =
            save(json!({"action": "save", "id": id, "date": "2026-07-06", "title": "改后的标题"}))
                .unwrap();
        assert_eq!(v["events"][0]["title"], "改后的标题");
        assert!(
            save(json!({"action": "save", "id": 9999, "date": "2026-07-06", "title": "x"}))
                .is_err()
        );
        assert_eq!(
            save(json!({"action": "delete", "id": id})).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            save(json!({"action": "delete", "id": id})).is_err(),
            "删两次要说明没有这条"
        );
        assert!(save(json!({"action": "nope"})).is_err());
        let day = day("2026-10-15", 50.0, 0.3, &[("zt", "c,n")]);
        cache_store(&db, &day).unwrap();
        assert_eq!(cache_load(&db, "2026-10-15"), Some(day));
        assert_eq!(cache_load(&db, "2026-10-14"), None);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 对真实上游:cargo test --locked structure::tests::live_structure_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_structure_probe() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-structure-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let started = std::time::Instant::now();
        let get = |path: &str| crate::MobileApiRequest {
            path: path.into(),
            method: "GET".into(),
            body: Value::Null,
        };
        let r = app.handle(get("/mobile/review/structure")).await;
        println!(
            "status {} in {:?}; recent {} prior {} structure {} drift-changed {}",
            r.status,
            started.elapsed(),
            r.body["recent_window"],
            r.body["prior_window"],
            r.body["structure"]["available"],
            r.body["field_changed"]
        );
        println!("summary {}", r.body["summary"]);
        println!("gaps {}", r.body["gaps"]);
        for d in r.body["field_drift"].as_array().into_iter().flatten() {
            println!(
                "  {} days {} changed {}",
                d["label"], d["days"], d["changed"]
            );
        }
        if let Ok(out) = std::env::var("VIBE_STRUCTURE_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&r.body).unwrap()).unwrap();
        }
        let again = std::time::Instant::now();
        let r2 = app.handle(get("/mobile/review/structure")).await;
        println!(
            "second call {:?}, same summary: {}",
            again.elapsed(),
            r2.body["summary"] == r.body["summary"]
        );
        assert_eq!(r.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
