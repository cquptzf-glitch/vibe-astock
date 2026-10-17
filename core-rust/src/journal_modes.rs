//! 交易日志 · 模式卡(移植自 orchestrator/src/finance/journal/modes.ts 与 view.ts 的 mode_save / mode_delete,口径来自 Vibe AStock v2.0.0)。
//!
//! 把自己的打法写下来,**带版本**,按版本分段统计业绩。打法改了以后,改之前和改之后的交易不能混在一起算平均:
//! 每个版本带生效日(`since`),交易按**入场日落在哪个版本的区间**归属,回答「上次调整之后表现有没有变化」。
//!
//! 🔴 三条约束(桌面版原样,改代码时别破坏):
//!    1. 不预填任何规则内容 —— 卡片文字全由用户写,系统只存,不校验内容、不给建议。
//!    2. 开新版本不覆盖旧版本 —— 已发生的交易永远归它当时生效的版本,历史不能被追溯改写。
//!       打法(playbook)也进版本:它决定哪些交易归这张卡,只放卡片层的话改一次就把历史全部重新归属了。
//!    3. 样本不够不比较 —— 每个版本少于 6 笔标「不够」,不给倾向性结论。
//! 🔴 生效日:建卡当天就是第一版;之后改动从**次日**生效(当天的交易不做日内先后归因)。
//! 🔴 超长直接拒收,不截断(截断等于替用户改了规则)。字数按「字」数,不是字节。
//! ⛔ 模式卡是用户自己的打法,只留在本机,不进任何 AI;进备份(备份 v7)。
//! 与桌面版的差别:手机端只有清仓后才算已实现盈亏,所以进版本统计的是已平仓的交易和补记;卡片 id 是数字不是字符串。

use crate::emotion;
use crate::journal_risk::{self, Row};
use anyhow::{anyhow, Context, Result};
use chrono::{Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

pub(crate) const PLAYBOOKS: [&str; 6] = ["打板", "低吸", "接力", "半路", "套利", "其它"];
/// 每个版本至少这么多笔,才单独给业绩读数、才拿来比较
pub(crate) const MIN_PER_VERSION: usize = 6;
/// 只看最近这么多笔(桌面版同);超出时整个暂停比较(样本被截断了)
const MODES_LIMIT: usize = 1000;
pub(crate) const TEXT_FIELDS: [(&str, &str); 5] = [("setup", "什么情况下做"), ("entry", "怎么进"), ("exit", "怎么出"), ("sizing", "仓位"), ("phase", "适用的情绪阶段")];
pub(crate) const TEXT_MAX: usize = 800;
pub(crate) const NAME_MAX: usize = 60;
pub(crate) const CHANGES_MAX: usize = 300;
pub(crate) const VERSIONS_MAX: usize = 200;
pub(crate) const CARDS_MAX: usize = 1000;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------- 数据

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Version {
    pub version: i64,
    pub since: String,
    #[serde(default)]
    pub changes: String,
    pub playbook: String,
    #[serde(default)]
    pub setup: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub exit: String,
    #[serde(default)]
    pub sizing: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub saved_at: String,
}

impl Version {
    fn text(&self, key: &str) -> &str {
        match key {
            "setup" => &self.setup,
            "entry" => &self.entry,
            "exit" => &self.exit,
            "sizing" => &self.sizing,
            _ => &self.phase,
        }
    }
}

/// 一张模式卡(备份里的一行也是它)
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Card {
    pub id: i64,
    pub name: String,
    pub playbook: String,
    pub versions: Vec<Version>,
    pub created_at: String,
    pub updated_at: String,
}

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile journal modes sqlite")
}

pub(crate) fn read_cards(conn: &Connection) -> Result<Vec<Card>> {
    let mut stmt = conn.prepare("SELECT id, name, playbook, versions, created_at, updated_at FROM mobile_journal_modes ORDER BY created_at, id")?;
    let raw = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(id, name, playbook, versions, created_at, updated_at)| {
            let versions = serde_json::from_str::<Vec<Version>>(&versions).with_context(|| format!("模式卡 {id} 的版本记录损坏"))?;
            Ok(Card { id, name, playbook, versions, created_at, updated_at })
        })
        .collect()
}

pub(crate) fn write_card(conn: &Connection, card: &Card) -> Result<()> {
    conn.execute(
        "INSERT INTO mobile_journal_modes(id, name, playbook, versions, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![card.id, card.name, card.playbook, serde_json::to_string(&card.versions)?, card.created_at, card.updated_at],
    )?;
    Ok(())
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn valid_day(day: &str) -> bool {
    day.len() == 10 && NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok()
}

fn is_playbook(text: &str) -> bool {
    PLAYBOOKS.contains(&text)
}

/// 备份 / 读取共用的检查:返回第一个问题
pub(crate) fn card_problem(card: &Card) -> Option<String> {
    let label = if card.name.is_empty() { format!("#{}", card.id) } else { card.name.clone() };
    if card.id <= 0 {
        return Some(format!("模式卡 id 无效:{}", card.id));
    }
    if card.name.trim().is_empty() || char_len(&card.name) > NAME_MAX {
        return Some(format!("模式卡 {label} 的名字要 1~{NAME_MAX} 字"));
    }
    if !is_playbook(&card.playbook) {
        return Some(format!("模式卡 {label} 的打法不认识:{}", card.playbook));
    }
    if card.versions.is_empty() || card.versions.len() > VERSIONS_MAX {
        return Some(format!("模式卡 {label} 的版本数要在 1~{VERSIONS_MAX} 之间"));
    }
    let mut last = 0;
    for v in &card.versions {
        if v.version <= last {
            return Some(format!("模式卡 {label} 的版本号要从小到大、不重复"));
        }
        last = v.version;
        if !valid_day(&v.since) {
            return Some(format!("模式卡 {label} v{} 的生效日不是有效日期:{}", v.version, v.since));
        }
        if !is_playbook(&v.playbook) {
            return Some(format!("模式卡 {label} v{} 的打法不认识:{}", v.version, v.playbook));
        }
        if char_len(&v.changes) > CHANGES_MAX {
            return Some(format!("模式卡 {label} v{} 的「改了什么」超过 {CHANGES_MAX} 字", v.version));
        }
        for (key, name) in TEXT_FIELDS {
            if char_len(v.text(key)) > TEXT_MAX {
                return Some(format!("模式卡 {label} v{} 的「{name}」超过 {TEXT_MAX} 字", v.version));
            }
        }
    }
    None
}

// ---------------------------------------------------------------- 业绩分段

/// 这笔交易发生时哪个版本在生效:since ≤ 入场日里最晚的(同一天以版本号大的为准);早于第一版返回 None
fn version_at<'a>(versions: &'a [Version], date: &str) -> Option<&'a Version> {
    let mut best: Option<&Version> = None;
    for v in versions {
        if v.since.as_str() > date {
            continue;
        }
        if best.map_or(true, |b| v.since > b.since || (v.since == b.since && v.version > b.version)) {
            best = Some(v);
        }
    }
    best
}

/// 胜率不算持平(和交易日志分组统计同一口径);平均数照桌面版写法 sum / 笔数
pub(crate) fn mode_stats(pnls: &[f64]) -> Value {
    if pnls.is_empty() {
        return json!({"trades": 0, "win_rate": null, "median_pct": null, "avg_pct": null, "best": null, "worst": null});
    }
    let wins = pnls.iter().filter(|v| **v > 0.0).count();
    let decided = wins + pnls.iter().filter(|v| **v < 0.0).count();
    json!({
        "trades": pnls.len(),
        "win_rate": if decided > 0 { json!(r3(wins as f64 / decided as f64)) } else { Value::Null },
        "median_pct": r2(emotion::median(pnls)),
        "avg_pct": r2(pnls.iter().sum::<f64>() / pnls.len() as f64),
        "best": r2(pnls.iter().cloned().fold(f64::MIN, f64::max)),
        "worst": r2(pnls.iter().cloned().fold(f64::MAX, f64::min)),
    })
}

fn version_value(v: &Version) -> Value {
    json!({"version": v.version, "since": v.since, "changes": v.changes, "playbook": v.playbook, "setup": v.setup, "entry": v.entry,
        "exit": v.exit, "sizing": v.sizing, "phase": v.phase, "saved_at": v.saved_at})
}

/// 每张卡、每个版本的业绩分段。`rows` 要是桌面版顺序(入场日新的在前);只取最近 MODES_LIMIT 笔,超出时整个暂停比较
pub(crate) fn performance(cards: &[Card], rows: &[Row]) -> Value {
    if cards.is_empty() {
        return json!({"available": false, "reason": "还没有模式卡 —— 先把「我这套打法」写下来,统计才有分段依据"});
    }
    let truncated = rows.len() > MODES_LIMIT;
    let scored: Vec<(String, Option<&str>, f64)> = rows.iter().take(MODES_LIMIT).filter_map(|r| r.pnl_pct.map(|p| (journal_risk::entry_day(r), r.e.playbook.as_deref(), p))).collect();
    let out: Vec<Value> = cards
        .iter()
        .map(|c| {
            let vers = &c.versions;
            let mut buckets: std::collections::BTreeMap<i64, Vec<f64>> = std::collections::BTreeMap::new();
            let mut before: Vec<f64> = Vec::new();
            let mut matched = 0usize;
            // 卡片诞生之前的交易:用第一版的打法判断算不算这张卡的历史,单独列、不进任何版本
            let first_pb = vers.first().map(|v| v.playbook.as_str()).filter(|p| !p.is_empty()).unwrap_or(&c.playbook);
            for (day, playbook, pnl) in &scored {
                match version_at(vers, day) {
                    None => {
                        if *playbook == Some(first_pb) {
                            matched += 1;
                            before.push(*pnl);
                        }
                    }
                    Some(v) => {
                        // 归属看「那天生效的那个版本的打法」,不是卡片现在的打法
                        if *playbook != Some(v.playbook.as_str()) {
                            continue;
                        }
                        matched += 1;
                        buckets.entry(v.version).or_default().push(*pnl);
                    }
                }
            }
            let by_version: Vec<Value> = vers
                .iter()
                .map(|v| {
                    let mut st = mode_stats(buckets.get(&v.version).map(Vec::as_slice).unwrap_or(&[]));
                    let enough = !truncated && st["trades"].as_u64().unwrap_or(0) as usize >= MIN_PER_VERSION;
                    let mut out = version_value(v);
                    out.as_object_mut().unwrap().append(st.as_object_mut().unwrap());
                    out["enough"] = json!(enough);
                    out
                })
                .collect();
            // 同一天生效的几个版本只认最后那个(当天改了两次)
            let effective: Vec<&Value> = by_version.iter().enumerate().filter(|(i, v)| !by_version[i + 1..].iter().any(|later| later["since"] == v["since"])).map(|(_, v)| v).collect();
            let latest = effective.last().copied();
            let prev = if effective.len() >= 2 { Some(effective[effective.len() - 2]) } else { None };
            let cmp = match (latest, prev) {
                (Some(l), Some(p)) if l["enough"] == true && p["enough"] == true && l["win_rate"].is_f64() && p["win_rate"].is_f64() => Some(json!({
                    "win_rate_delta": r3(l["win_rate"].as_f64().unwrap() - p["win_rate"].as_f64().unwrap()),
                    "median_delta": r2(l["median_pct"].as_f64().unwrap_or(0.0) - p["median_pct"].as_f64().unwrap_or(0.0)),
                    "from_version": p["version"], "to_version": l["version"],
                })),
                _ => None,
            };
            let before_stats = if before.is_empty() {
                Value::Null
            } else {
                let mut st = mode_stats(&before);
                st["enough"] = json!(!truncated && before.len() >= MIN_PER_VERSION);
                st
            };
            json!({
                "id": c.id, "name": c.name, "playbook": c.playbook, "version_count": vers.len(), "by_version": by_version,
                "matched_trades": matched, "before_card": before_stats,
                "compare_blocked": latest.is_some() && prev.is_some() && cmp.is_none(),
                "latest_vs_prev": cmp,
            })
        })
        .collect();
    json!({
        "available": true,
        "cards": out,
        "min_per_version": MIN_PER_VERSION,
        "truncated": truncated,
        "note": format!("{}交易按打法归到卡片,再按入场日落在哪个版本的生效区间分段。建卡当天算第一版,之后的修改从次日生效;不做日内先后归因。建卡之前的交易单独列出、不进任何版本 —— 否则等于拿现在的规则评价更早的操作。",
            if truncated { format!("交易超过 {MODES_LIMIT} 笔,只看最近 {MODES_LIMIT} 笔,暂停版本比较。") } else { String::new() }),
    })
}

/// 卡片文字是否改了(决定要不要开新版本)
fn changed(last: Option<&Version>, playbook: &str, texts: &[String; 5]) -> bool {
    let Some(last) = last else { return true };
    last.playbook != playbook || TEXT_FIELDS.iter().zip(texts).any(|((key, _), t)| last.text(key) != t)
}

// ---------------------------------------------------------------- 读与写

fn china_today() -> String {
    emotion::now_china().0.format("%Y-%m-%d").to_string()
}

pub fn view(db: &Path) -> Result<Value> {
    view_on(db, &china_today())
}

pub(crate) fn view_on(db: &Path, today: &str) -> Result<Value> {
    let a = journal_risk::analysis(db)?;
    let cards = read_cards(&open(db)?)?;
    let mut rows = a.rows;
    journal_risk::va_order(&mut rows);
    Ok(json!({
        "available": true,
        "source": "mobile_sqlite",
        "playbooks": PLAYBOOKS,
        "today": today,
        "limits": {"text": TEXT_MAX, "name": NAME_MAX, "changes": CHANGES_MAX, "versions": VERSIONS_MAX},
        "text_fields": TEXT_FIELDS.iter().map(|(k, l)| json!({"key": k, "label": l})).collect::<Vec<_>>(),
        "modes": performance(&cards, &rows),
        "excluded_non_stock": a.excluded.len(),
        "notes": [
            "不预填任何规则内容:卡片文字全由你自己写,系统只存、不校验内容、不给建议",
            "模式卡是你自己的打法,只留在本机、进备份,不进任何 AI",
            "只有清仓后的交易(和补记)才有盈亏 %,进版本统计;没卖完的交易不算",
        ],
    }))
}

fn text_of(body: &Value, key: &str, label: &str, max: usize) -> Result<String> {
    let text = match body.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(t)) => t.trim().to_string(),
        Some(_) => return Err(anyhow!("「{label}」要是文字")),
    };
    let n = char_len(&text);
    if n > max {
        return Err(anyhow!("「{label}」最多 {max} 字(现在 {n} 字)"));
    }
    Ok(text)
}

fn add_day(day: &str) -> Result<String> {
    let d = NaiveDate::parse_from_str(day, "%Y-%m-%d").context("今天的日期格式不对")?;
    Ok((d + Duration::days(1)).format("%Y-%m-%d").to_string())
}

/// 新建或修改模式卡。带 `card_id` 是修改:打法或五段文字有变化就开新版本(次日生效),旧版本原样保留;只改名字不开新版本
fn save(db: &Path, body: &Value, today: &str, now: &str) -> Result<Value> {
    let name = text_of(body, "name", "名字", NAME_MAX)?;
    if name.is_empty() {
        return Err(anyhow!("模式卡要有名字"));
    }
    let playbook = body.get("playbook").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if !is_playbook(&playbook) {
        return Err(anyhow!("打法只能是 {}", PLAYBOOKS.join(" / ")));
    }
    let mut texts: [String; 5] = Default::default();
    for (i, (key, label)) in TEXT_FIELDS.iter().enumerate() {
        texts[i] = text_of(body, key, label, TEXT_MAX)?;
    }
    let changes = text_of(body, "changes", "改了什么", CHANGES_MAX)?;
    let make = |version: i64, since: String, changes: String| Version {
        version, since, changes, playbook: playbook.clone(),
        setup: texts[0].clone(), entry: texts[1].clone(), exit: texts[2].clone(), sizing: texts[3].clone(), phase: texts[4].clone(), saved_at: now.to_string(),
    };
    let mut conn = open(db)?;
    let tx = conn.transaction()?;
    let (id, created, new_version) = match body.get("card_id") {
        None | Some(Value::Null) => {
            if read_cards(&tx)?.len() >= CARDS_MAX {
                return Err(anyhow!("模式卡最多 {CARDS_MAX} 张"));
            }
            // 第一版从今天生效,不回溯到更早
            let v1 = make(1, today.to_string(), "初版".into());
            tx.execute(
                "INSERT INTO mobile_journal_modes(name, playbook, versions, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![name, playbook, serde_json::to_string(&[v1])?, now],
            )?;
            (tx.last_insert_rowid(), true, false)
        }
        Some(v) => {
            let id = v.as_i64().ok_or_else(|| anyhow!("card_id 要是数字"))?;
            let prev = read_cards(&tx)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("没找到这张模式卡,可能已经删了;请刷新后重试"))?;
            let last = prev.versions.last();
            let is_changed = changed(last, &playbook, &texts);
            let mut versions = prev.versions.clone();
            if is_changed {
                let n = last.map_or(0, |l| l.version) + 1;
                versions.push(make(n, add_day(today)?, if changes.is_empty() { "(没写改了什么)".into() } else { changes }));
                if versions.len() > VERSIONS_MAX {
                    return Err(anyhow!("一张卡最多 {VERSIONS_MAX} 个版本;换了一套打法就新建一张卡"));
                }
            } else if let Some(l) = versions.last_mut() {
                l.saved_at = now.to_string();
            }
            tx.execute("UPDATE mobile_journal_modes SET name = ?2, playbook = ?3, versions = ?4, updated_at = ?5 WHERE id = ?1", params![id, name, playbook, serde_json::to_string(&versions)?, now])?;
            (id, false, is_changed)
        }
    };
    tx.commit()?;
    Ok(json!({"result": {"card_id": id, "created": created, "new_version": new_version}}))
}

fn delete(db: &Path, body: &Value) -> Result<Value> {
    let id = body.get("card_id").and_then(Value::as_i64).ok_or_else(|| anyhow!("要给出模式卡的 id"))?;
    let removed = open(db)?.execute("DELETE FROM mobile_journal_modes WHERE id = ?1", params![id])? > 0;
    if !removed {
        return Err(anyhow!("没找到这张模式卡,可能已经删了"));
    }
    Ok(json!({"result": {"card_id": id, "removed": true}}))
}

/// `POST /mobile/journal/modes`:`action` = save / delete;成功后连同新的视图一起返回
pub fn act(db: &Path, body: &Value) -> Result<Value> {
    act_on(db, body, &china_today(), &Utc::now().to_rfc3339())
}

pub(crate) fn act_on(db: &Path, body: &Value, today: &str, now: &str) -> Result<Value> {
    let result = match body.get("action").and_then(Value::as_str).unwrap_or("") {
        "save" => save(db, body, today, now)?,
        "delete" => delete(db, body)?,
        other => return Err(anyhow!("不认识的动作 {other:?}(只有 save / delete)")),
    };
    Ok(json!({"result": result["result"], "view": view_on(db, today)?}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_risk::{Entry, Fill};
    use crate::journal_settings::Fees;

    const ZERO: Fees = Fees { commission_rate: 0.0, commission_min: 0.0, stamp_tax_rate: 0.0, transfer_fee_rate: 0.0 };

    fn db(label: &str) -> (crate::AppCore, std::path::PathBuf) {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-jmodes-{label}-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        (app, dir)
    }

    fn v(version: i64, since: &str, playbook: &str) -> Version {
        Version { version, since: since.into(), changes: String::new(), playbook: playbook.into(), ..Version::default() }
    }

    fn card(versions: Vec<Version>) -> Card {
        Card { id: 1, name: "首板早封".into(), playbook: versions[0].playbook.clone(), versions, created_at: "2026-10-01T00:00:00Z".into(), updated_at: "2026-10-01T00:00:00Z".into() }
    }

    /// pnl_pct = pnl_per_share / 10 * 100,入场日 = 买入日
    fn row(id: i64, day: &str, playbook: &str, pnl_pct: f64) -> Row {
        let e = Entry {
            id, kind: "episode".into(), symbol: format!("60{id:04}"), date: day.into(), created_at: format!("{day}T09:00:00Z"), playbook: Some(playbook.into()),
            fills: vec![
                Fill { side: "buy".into(), date: day.into(), price: 10.0, shares: 100.0, fee: Some(0.0) },
                Fill { side: "sell".into(), date: day.into(), price: 10.0 + pnl_pct / 10.0, shares: 100.0, fee: Some(0.0) },
            ],
            ..Entry::default()
        };
        journal_risk::settle(e, &ZERO, id as usize)
    }

    #[test]
    fn version_at_picks_the_latest_effective_version_and_prefers_higher_number_on_same_day() {
        let vs = vec![v(1, "2026-10-01", "打板"), v(2, "2026-10-10", "打板"), v(3, "2026-10-10", "接力")];
        assert!(version_at(&vs, "2026-09-30").is_none(), "早于第一版没有生效版本");
        assert_eq!(version_at(&vs, "2026-10-09").unwrap().version, 1);
        assert_eq!(version_at(&vs, "2026-10-10").unwrap().version, 3, "同一天以版本号大的为准");
        assert_eq!(version_at(&vs, "2026-12-01").unwrap().version, 3);
    }

    #[test]
    fn performance_segments_by_entry_day_and_the_playbook_in_force_that_day() {
        // v1 打板 自 10-01;v2 改成 接力 自 10-08。打板的交易在 v2 之后不再归这张卡
        let c = card(vec![v(1, "2026-10-01", "打板"), v(2, "2026-10-08", "接力")]);
        let mut rows = vec![
            row(1, "2026-09-30", "打板", 4.0), // 建卡之前:单独列
            row(2, "2026-10-02", "打板", 2.0), row(3, "2026-10-03", "打板", -1.0), row(4, "2026-10-07", "接力", 9.0), // 接力在 v1 区间不归卡
            row(5, "2026-10-09", "接力", 3.0), row(6, "2026-10-10", "打板", 7.0), // 打板在 v2 区间不归卡
        ];
        journal_risk::va_order(&mut rows);
        let p = performance(&[c], &rows);
        let card = &p["cards"][0];
        assert_eq!(card["matched_trades"], 4);
        assert_eq!(card["by_version"][0]["trades"], 2);
        assert_eq!(card["by_version"][0]["win_rate"], 0.5);
        assert_eq!(card["by_version"][0]["median_pct"], 0.5);
        assert_eq!(card["by_version"][1]["trades"], 1);
        assert_eq!(card["before_card"]["trades"], 1);
        assert_eq!(card["before_card"]["enough"], false);
        assert_eq!(card["by_version"][0]["enough"], false, "不到 6 笔标不够");
        assert_eq!(card["latest_vs_prev"], Value::Null);
        assert_eq!(card["compare_blocked"], true, "样本不够不比较,并说明");
    }

    #[test]
    fn comparison_needs_enough_samples_in_both_versions_and_ignores_same_day_duplicates() {
        let c = card(vec![v(1, "2026-09-01", "打板"), v(2, "2026-09-20", "打板"), v(3, "2026-09-20", "打板")]);
        let mut rows: Vec<Row> = Vec::new();
        for i in 0..6 {
            rows.push(row(10 + i, &format!("2026-09-{:02}", 2 + i), "打板", if i < 3 { 2.0 } else { -2.0 })); // v1:3 胜 3 负
        }
        for i in 0..6 {
            rows.push(row(30 + i, &format!("2026-09-{:02}", 21 + i), "打板", if i < 5 { 2.0 } else { -2.0 })); // 同日 v2、v3,按 v3 归属:5 胜 1 负
        }
        journal_risk::va_order(&mut rows);
        let p = performance(&[c], &rows);
        let card = &p["cards"][0];
        assert_eq!(card["by_version"][1]["trades"], 0, "同一天生效的两个版本,交易归版本号大的");
        assert_eq!(card["by_version"][2]["trades"], 6);
        let cmp = &card["latest_vs_prev"];
        assert_eq!((cmp["from_version"].clone(), cmp["to_version"].clone()), (json!(1), json!(3)), "同日两版只认最后那个");
        assert_eq!(cmp["win_rate_delta"], r3(5.0 / 6.0 - 0.5));
        assert_eq!(performance(&[], &rows)["available"], false);
    }

    #[test]
    fn save_creates_versions_from_tomorrow_and_keeps_old_ones_untouched() {
        let (app, dir) = db("save");
        let path = app.db_path().to_path_buf();
        let body = json!({"action": "save", "name": " 首板早封 ", "playbook": "打板", "setup": "10 点前封", "entry": "排板", "exit": "", "sizing": "1/3", "phase": "回暖"});
        let made = act_on(&path, &body, "2026-10-16", "2026-10-16T01:00:00Z").unwrap();
        assert_eq!((made["result"]["created"].clone(), made["result"]["new_version"].clone()), (json!(true), json!(false)));
        let id = made["result"]["card_id"].as_i64().unwrap();
        let cards = read_cards(&Connection::open(&path).unwrap()).unwrap();
        assert_eq!((cards[0].name.as_str(), cards[0].versions.len(), cards[0].versions[0].since.as_str(), cards[0].versions[0].changes.as_str()), ("首板早封", 1, "2026-10-16", "初版"));
        // 只改名字:不开新版本,只更新最后一版的保存时间
        let renamed = act_on(&path, &json!({"action": "save", "card_id": id, "name": "首板 2.0", "playbook": "打板", "setup": "10 点前封", "entry": "排板", "sizing": "1/3", "phase": "回暖"}), "2026-10-16", "2026-10-16T02:00:00Z").unwrap();
        assert_eq!(renamed["result"]["new_version"], false);
        let cards = read_cards(&Connection::open(&path).unwrap()).unwrap();
        assert_eq!((cards[0].name.as_str(), cards[0].versions.len(), cards[0].versions[0].saved_at.as_str()), ("首板 2.0", 1, "2026-10-16T02:00:00Z"));
        // 改了文字:开 v2,次日生效,旧版本原样
        let before = cards[0].versions[0].clone();
        let changed_body = json!({"action": "save", "card_id": id, "name": "首板 2.0", "playbook": "打板", "setup": "9:45 前封", "entry": "排板", "sizing": "1/3", "phase": "回暖", "changes": "收紧封板时间"});
        let v2 = act_on(&path, &changed_body, "2026-10-16", "2026-10-16T03:00:00Z").unwrap();
        assert_eq!(v2["result"]["new_version"], true);
        let cards = read_cards(&Connection::open(&path).unwrap()).unwrap();
        assert_eq!(cards[0].versions[0], before, "旧版本不动");
        assert_eq!((cards[0].versions[1].version, cards[0].versions[1].since.as_str(), cards[0].versions[1].changes.as_str()), (2, "2026-10-17", "收紧封板时间"));
        // 改打法也开新版本;没写改了什么给占位说明
        let v3 = act_on(&path, &json!({"action": "save", "card_id": id, "name": "首板 2.0", "playbook": "接力", "setup": "9:45 前封", "entry": "排板", "sizing": "1/3", "phase": "回暖"}), "2026-10-17", "2026-10-17T03:00:00Z").unwrap();
        assert_eq!(v3["result"]["new_version"], true);
        let cards = read_cards(&Connection::open(&path).unwrap()).unwrap();
        assert_eq!((cards[0].versions[2].since.as_str(), cards[0].versions[2].changes.as_str(), cards[0].playbook.as_str()), ("2026-10-18", "(没写改了什么)", "接力"));
        // 视图带回版本业绩;删除
        assert_eq!(v3["view"]["modes"]["cards"][0]["version_count"], 3);
        let gone = act_on(&path, &json!({"action": "delete", "card_id": id}), "2026-10-17", "x").unwrap();
        assert_eq!(gone["view"]["modes"]["available"], false);
        assert!(act_on(&path, &json!({"action": "delete", "card_id": id}), "2026-10-17", "x").unwrap_err().to_string().contains("没找到"));
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_rejects_overlong_text_instead_of_truncating_and_counts_characters() {
        let (app, dir) = db("long");
        let path = app.db_path().to_path_buf();
        let base = |setup: String| json!({"action": "save", "name": "卡", "playbook": "低吸", "setup": setup});
        // 800 个汉字(2400 字节)可以,801 个拒收
        assert!(act_on(&path, &base("字".repeat(800)), "2026-10-16", "n").is_ok());
        let err = act_on(&path, &base("字".repeat(801)), "2026-10-16", "n").unwrap_err().to_string();
        assert!(err.contains("最多 800 字") && err.contains("801"), "{err}");
        let stored = read_cards(&Connection::open(&path).unwrap()).unwrap();
        assert_eq!((stored.len(), char_len(&stored[0].versions[0].setup)), (1, 800), "拒收不留半张卡");
        for (body, needle) in [
            (json!({"action": "save", "name": "", "playbook": "低吸"}), "要有名字"),
            (json!({"action": "save", "name": "x".repeat(61), "playbook": "低吸"}), "最多 60 字"),
            (json!({"action": "save", "name": "卡", "playbook": "追涨"}), "打法只能是"),
            (json!({"action": "save", "name": "卡", "playbook": "低吸", "entry": 5}), "要是文字"),
            (json!({"action": "save", "name": "卡", "playbook": "低吸", "changes": "改".repeat(301), "card_id": 1}), "最多 300 字"),
            (json!({"action": "save", "card_id": 99, "name": "卡", "playbook": "低吸"}), "没找到"),
            (json!({"action": "nope"}), "不认识的动作"),
        ] {
            let e = act_on(&path, &body, "2026-10-16", "n").unwrap_err().to_string();
            assert!(e.contains(needle), "{e} 应含 {needle}");
        }
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn view_counts_only_a_share_stocks_and_reports_the_rest() {
        let (app, dir) = db("view");
        let path = app.db_path().to_path_buf();
        act_on(&path, &json!({"action": "save", "name": "卡", "playbook": "低吸"}), "2026-10-01", "n").unwrap();
        for (code, pb) in [("600519", "低吸"), ("510300", "低吸")] {
            crate::journal::act(&path, &json!({"action": "record", "symbol": code, "name": code, "playbook": pb,
                "fills": [{"type": "buy", "date": "2026-10-12", "price": 10, "shares": 100, "fee": 0}, {"type": "sell", "date": "2026-10-13", "price": 11, "shares": 100, "fee": 0}]})).unwrap();
        }
        let v = view_on(&path, "2026-10-16").unwrap();
        assert_eq!(v["excluded_non_stock"], 1, "ETF 不进交易日志的统计");
        assert_eq!(v["modes"]["cards"][0]["matched_trades"], 1);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn card_problem_flags_what_a_restored_backup_must_not_contain() {
        let good = card(vec![v(1, "2026-10-01", "打板")]);
        assert_eq!(card_problem(&good), None);
        let mut c = good.clone();
        c.versions.clear();
        assert!(card_problem(&c).unwrap().contains("版本数"));
        let mut c = good.clone();
        c.versions.push(v(1, "2026-10-02", "打板"));
        assert!(card_problem(&c).unwrap().contains("从小到大"));
        let mut c = good.clone();
        c.versions[0].since = "2026-13-01".into();
        assert!(card_problem(&c).unwrap().contains("生效日"));
        let mut c = good.clone();
        c.versions[0].setup = "字".repeat(801);
        assert!(card_problem(&c).unwrap().contains("什么情况下做"));
        let mut c = good;
        c.playbook = "追涨".into();
        assert!(card_problem(&c).unwrap().contains("打法不认识"));
    }

    /// 生成前端冻结夹具(全部是合成数据,不含任何真实成交):
    /// VIBE_JOURNAL_MODES_FIXTURE=路径 cargo test --locked journal_modes::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let (app, dir) = db("fixture");
        let path = app.db_path().to_path_buf();
        let trade = |n: usize, day: &str, sell: &str, playbook: &str, bp: f64, sp: f64| {
            crate::journal::act(&path, &json!({"action": "record", "symbol": format!("6000{n:02}"), "name": format!("样本{n}"), "playbook": playbook,
                "fills": [{"type": "buy", "date": day, "price": bp, "shares": 1000, "fee": 0}, {"type": "sell", "date": sell, "price": sp, "shares": 1000, "fee": 0}]})).unwrap();
        };
        // 建卡之前一笔;卡 v1 区间(09-14~10-05)7 笔打板;v2 区间(10-06 起)6 笔打板;各夹一笔别的打法
        trade(1, "2026-09-10", "2026-09-11", "打板", 10.0, 10.4);
        let v1 = [("2026-09-14", 10.0, 10.5), ("2026-09-15", 12.0, 11.7), ("2026-09-17", 9.0, 9.6), ("2026-09-21", 15.0, 14.4), ("2026-09-24", 8.0, 8.5), ("2026-09-28", 20.0, 19.0), ("2026-10-02", 11.0, 11.4)];
        for (i, (d, bp, sp)) in v1.iter().enumerate() {
            trade(10 + i, d, d, "打板", *bp, *sp);
        }
        trade(30, "2026-09-18", "2026-09-19", "接力", 10.0, 11.0);
        let v2 = [("2026-10-06", 10.0, 10.9), ("2026-10-07", 12.0, 12.8), ("2026-10-09", 9.0, 9.1), ("2026-10-12", 15.0, 14.9), ("2026-10-14", 8.0, 8.9), ("2026-10-16", 20.0, 21.2)];
        for (i, (d, bp, sp)) in v2.iter().enumerate() {
            trade(40 + i, d, d, "打板", *bp, *sp);
        }
        // 第二张卡刚建:只有 v1、样本不够
        act_on(&path, &json!({"action": "save", "name": "低吸试验", "playbook": "低吸", "setup": "回踩 5 日线", "entry": "分两笔", "exit": "破位走", "sizing": "不超过 2 成", "phase": "震荡"}), "2026-10-14", "2026-10-14T01:00:00Z").unwrap();
        // 先用较早的「今天」建第一张卡,再改两次
        let made = act_on(&path, &json!({"action": "save", "name": "首板早封", "playbook": "打板", "setup": "10 点前封板", "entry": "排板", "exit": "次日冲高走", "sizing": "三分之一", "phase": "回暖到高潮"}), "2026-09-14", "2026-09-14T01:00:00Z").unwrap();
        let id = made["result"]["card_id"].as_i64().unwrap();
        act_on(&path, &json!({"action": "save", "card_id": id, "name": "首板早封", "playbook": "打板", "setup": "9:45 前封板", "entry": "排板", "exit": "次日冲高走", "sizing": "三分之一", "phase": "回暖到高潮", "changes": "收紧封板时间"}), "2026-10-05", "2026-10-05T01:00:00Z").unwrap();
        let v = view_on(&path, "2026-10-17").unwrap();
        if let Ok(out) = std::env::var("VIBE_JOURNAL_MODES_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        }
        let first = &v["modes"]["cards"][1];
        println!("cards {} v1 {} v2 {} cmp {}", v["modes"]["cards"].as_array().unwrap().len(), first["by_version"][0]["trades"], first["by_version"][1]["trades"], first["latest_vs_prev"]);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
