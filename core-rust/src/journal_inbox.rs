//! 交易日志 · 复查清单(移植自 orchestrator/src/finance/journal/inbox.ts,口径来自 Vibe AStock v2.0.0 `duanxian/inbox.py`「异常交易收件箱」)。
//!
//! 按客观条件筛出少数几笔值得回头看的交易,附上为什么进来。
//!
//! ⚠️ 判定基准全部来自用户自己:「仓位偏大」相对他自己的中位仓位,「持有偏久」相对他自己的中位持有天数,
//!    亏损上限用他写下的规矩。不塞行业标准,不排「严重程度」(按日期倒序),不给建议。
//! ⚠️ 最大浮盈浮亏类判定(扛过大幅浮亏、没吃到行情)只用**已缓存**的行情,一次网络请求都不发;
//!    没缓存的笔数如实报出来,提示先去「最大浮盈浮亏」里取一次行情。
//! 与桌面版的差别:没有送转流水,「期间疑似送转」的整轮看日线判断(见 journal_excursion);手机端只有清仓后才算已实现盈亏。
//! ⛔ 这些数据只留本机,不进任何 AI。

use crate::journal_excursion::{self, Lite};
use crate::journal_risk::{self, entry_day, money0, Row};
use crate::journal_settings::Rules;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// 偏离自己中位数多少倍算不寻常:不是行业标准,只是宽松的筛子,宁可少报
pub(crate) const DEVIATION_X: f64 = 2.0;
/// 少于这么多笔不做「相对自己习惯」的判定(3 笔的中位数不是习惯)
pub(crate) const MIN_HISTORY: usize = 8;
/// 中途浮亏深到这个程度(相对成本 %)算扛过大幅浮亏
pub(crate) const DEEP_MAE: f64 = -8.0;
/// 捕获率低于这个值且行情本来有肉 = 看对了没吃到
pub(crate) const LOW_CAPTURE: f64 = 0.3;
/// 只看最近这么多笔(桌面版同):几百笔之后再往前翻的意义不大
pub(crate) const INBOX_LIMIT: usize = 500;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn median(vals: &[f64]) -> f64 {
    crate::emotion::median(vals)
}

/// JS 的 String(number):整数不带小数点
fn g(x: f64) -> String {
    format!("{x}")
}

fn flag(key: &str, text: String) -> Value {
    json!({"key": key, "text": text})
}

/// `rows` 要是桌面版顺序(入场日新的在前);`excursions` = None 表示这一类判定整个没做;`excursion_skipped` = 卖光了、但行情没缓存所以跳过的笔数
pub(crate) fn build(
    rows: &[Row],
    rules: &Rules,
    rules_default: bool,
    excursions: Option<&BTreeMap<String, Lite>>,
    excursion_skipped: usize,
) -> Value {
    let recent = &rows[..rows.len().min(INBOX_LIMIT)];
    if recent.is_empty() {
        return json!({"available": false, "reason": "还没有交易记录"});
    }
    let cap_of = |r: &Row| (r.e.kind == "episode" && r.peak_cost > 0.0).then(|| r2(r.peak_cost));
    let caps: Vec<f64> = recent.iter().filter_map(cap_of).collect();
    let holds: Vec<f64> = recent
        .iter()
        .filter_map(|r| r.hold_days.map(|h| h as f64))
        .collect();
    let med_cap = (caps.len() >= MIN_HISTORY).then(|| median(&caps));
    let med_hold = (holds.len() >= MIN_HISTORY).then(|| median(&holds));
    let mut items: Vec<(String, Value)> = Vec::new();
    for r in recent {
        let mut flags: Vec<Value> = Vec::new();
        let (lim, pnl) = (rules.max_loss_per_trade_pct, r.pnl_pct);
        if let Some(p) = pnl {
            if lim > 0.0 && p < -lim {
                flags.push(flag(
                    "over_loss_limit",
                    format!("亏 {:.1}%,超过你自己写的单笔上限 {}%", p, g(lim)),
                ));
            }
        }
        if let (Some(m), Some(c)) = (med_cap, cap_of(r)) {
            if m > 0.0 && c > m * DEVIATION_X {
                flags.push(flag(
                    "oversized",
                    format!(
                        "占用 {} 元,是你中位仓位({})的 {:.1} 倍",
                        money0(c),
                        money0(m),
                        c / m
                    ),
                ));
            }
        }
        if let (Some(m), Some(h)) = (med_hold, r.hold_days) {
            if m > 0.0 && h as f64 > m * DEVIATION_X {
                flags.push(flag(
                    "held_long",
                    format!(
                        "持有 {h} 天,是你中位持有({} 天)的 {:.1} 倍",
                        g(m),
                        h as f64 / m
                    ),
                ));
            }
        }
        if r.e.as_planned == Some(false) {
            flags.push(flag("unplanned", "自己标了「计划外」".into()));
        }
        if r.e.kind == "episode" && !r.closed && r.e.planned_stop.is_none_or(|s| s == 0.0) {
            flags.push(flag(
                "no_stop",
                "还在手上,但没写计划止损 —— 最坏情况无从估计".into(),
            ));
        }
        if let Some(ex) = excursions.and_then(|m| m.get(&format!("mobile:{}", r.e.id))) {
            if ex.mae_pct <= DEEP_MAE {
                flags.push(flag(
                    "deep_mae",
                    format!(
                        "中途最多浮亏 {:.1}%({})",
                        ex.mae_pct,
                        if ex.realized_pct > 0.0 {
                            "最后还是赚的"
                        } else {
                            "最后亏了"
                        }
                    ),
                ));
            }
            if let Some(cr) = ex.capture_rate {
                if cr < LOW_CAPTURE {
                    flags.push(flag(
                        "low_capture",
                        format!(
                            "这段最多能赚 {:.1}%,实际落袋 {:.1}%(吃到 {:.0}%){}",
                            ex.mfe_pct,
                            ex.realized_pct,
                            cr * 100.0,
                            if ex.same_day {
                                ";同日进出,最大浮盈只是上界"
                            } else {
                                ""
                            }
                        ),
                    ));
                }
            }
        }
        if !flags.is_empty() {
            let day = entry_day(r);
            items.push((day.clone(), json!({
                "key": format!("mobile:{}", r.e.id), "date": day, "symbol": r.e.symbol, "name": r.e.name, "playbook": r.e.playbook,
                "pnl_pct": pnl, "note": r.e.note, "closed": if r.e.kind == "episode" { r.closed } else { true }, "flags": flags,
            })));
        }
    }
    // 按日期倒序(稳定排序),不按「我们觉得多严重」排 —— 那是替用户判断
    items.sort_by(|a, b| b.0.cmp(&a.0));
    let ordered: Vec<Value> = items.into_iter().map(|(_, v)| v).collect();
    let mut by_flag: BTreeMap<String, usize> = BTreeMap::new();
    for it in &ordered {
        for f in it["flags"].as_array().into_iter().flatten() {
            *by_flag
                .entry(f["key"].as_str().unwrap_or("").to_string())
                .or_default() += 1;
        }
    }
    json!({
        "available": true,
        "count": ordered.len(),
        "items": ordered,
        "scanned": recent.len(),
        "by_flag": by_flag,
        "baseline": {"median_capital": med_cap, "median_hold_days": med_hold, "history_enough": med_cap.is_some(), "min_history": MIN_HISTORY},
        "rules_is_default": rules_default,
        "excursion_available": excursions.is_some_and(|m| !m.is_empty()),
        "excursion_skipped": excursion_skipped,
        "excursion_hint": if excursion_skipped > 0 { json!(format!("{excursion_skipped} 笔的行情还没缓存,扛大幅浮亏 / 没吃到行情这两类判定暂时跳过了 —— 在「最大浮盈浮亏」里点「取行情并计算」会把行情存下来,之后刷新这里就有了。")) } else { Value::Null },
        "note": "判定基准全部来自你自己:仓位大不大、持有久不久是相对你自己的中位数,亏损上限用的是你写下的规矩。这里只说哪里不寻常,不说该怎么做。",
    })
}

/// `GET /mobile/journal/inbox`:复查清单(只读缓存行情,一次网络请求都不发)
pub fn view(db: &Path) -> Result<Value> {
    let a = journal_risk::analysis(db)?;
    let mut rows = a.rows;
    journal_risk::va_order(&mut rows);
    let recent = &rows[..rows.len().min(INBOX_LIMIT)];
    let (map, skipped) = journal_excursion::cached_excursions(db, recent)?;
    let mut v = build(
        &rows,
        &a.settings.rules,
        a.settings.rules_default,
        Some(&map),
        skipped,
    );
    v["settings"] =
        json!({"rules": a.settings.rules.to_value(), "rules_default": a.settings.rules_default});
    v["generated_at"] = json!(chrono::Utc::now().to_rfc3339());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_risk::{Entry, Fill};
    use crate::journal_settings::Fees;

    const ZERO: Fees = Fees {
        commission_rate: 0.0,
        commission_min: 0.0,
        stamp_tax_rate: 0.0,
        transfer_fee_rate: 0.0,
        fifo: false,
    };

    fn rules() -> Rules {
        crate::journal_settings::settings_of(None).rules
    }

    /// 买入 shares 股 @ 10,卖出 @ sell_price,持有 hold 天(入场日 2026-09-01 起按 id 往后挪一天)
    fn closed(id: i64, shares: f64, sell_price: f64, hold: i64) -> Row {
        let buy = chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap() + chrono::Duration::days(id);
        let sell = buy + chrono::Duration::days(hold);
        let fmt = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();
        let e = Entry {
            id,
            kind: "episode".into(),
            symbol: format!("600{id:03}"),
            name: format!("N{id}"),
            date: fmt(buy),
            created_at: format!("{}T09:00:00Z", fmt(buy)),
            fills: vec![
                Fill {
                    side: "buy".into(),
                    date: fmt(buy),
                    price: 10.0,
                    shares,
                    fee: Some(0.0),
                },
                Fill {
                    side: "sell".into(),
                    date: fmt(sell),
                    price: sell_price,
                    shares,
                    fee: Some(0.0),
                },
            ],
            ..Entry::default()
        };
        journal_risk::settle(e, &ZERO, id as usize)
    }

    fn ordered(mut rows: Vec<Row>) -> Vec<Row> {
        journal_risk::va_order(&mut rows);
        rows
    }

    fn flags_of(v: &Value, id: i64) -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["key"] == format!("mobile:{id}"))
            .flat_map(|i| {
                i["flags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| f["key"].as_str().unwrap().to_string())
            })
            .collect()
    }

    #[test]
    fn flags_are_relative_to_the_users_own_median_and_rules_and_need_enough_history() {
        // 9 笔都是 100 股、持有 1 天、不亏;第 10 笔 1000 股(10 倍仓位)、持有 5 天、亏 8%
        let mut rows: Vec<Row> = (1..=9).map(|i| closed(i, 100.0, 10.2, 1)).collect();
        rows.push(closed(10, 1000.0, 9.2, 5));
        let v = build(&ordered(rows.clone()), &rules(), true, None, 0);
        assert_eq!(v["baseline"]["median_capital"], 1000.0);
        assert_eq!(v["baseline"]["median_hold_days"], 1.0);
        assert_eq!(
            flags_of(&v, 10),
            vec!["over_loss_limit", "oversized", "held_long"]
        );
        let text = v["items"][0]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["text"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(text[0], "亏 -8.0%,超过你自己写的单笔上限 5%");
        assert_eq!(text[1], "占用 10,000 元,是你中位仓位(1,000)的 10.0 倍");
        assert_eq!(text[2], "持有 5 天,是你中位持有(1 天)的 5.0 倍");
        assert_eq!(v["count"], 1);
        assert_eq!(v["by_flag"]["oversized"], 1);
        // 不到 8 笔:不做「相对自己习惯」的判定,只剩规矩类
        let few = build(&ordered(rows[5..].to_vec()), &rules(), true, None, 0);
        assert_eq!(
            (
                few["baseline"]["history_enough"].clone(),
                flags_of(&few, 10)
            ),
            (json!(false), vec!["over_loss_limit".to_string()])
        );
        assert_eq!(build(&[], &rules(), true, None, 0)["available"], false);
    }

    #[test]
    fn unplanned_and_unprotected_open_positions_and_date_ordering() {
        let mut a = closed(1, 100.0, 10.1, 1);
        a.e.as_planned = Some(false);
        let open = journal_risk::settle(
            Entry {
                id: 2,
                kind: "episode".into(),
                symbol: "600002".into(),
                date: "2026-10-16".into(),
                fills: vec![Fill {
                    side: "buy".into(),
                    date: "2026-10-16".into(),
                    price: 10.0,
                    shares: 100.0,
                    fee: Some(0.0),
                }],
                ..Entry::default()
            },
            &ZERO,
            1,
        );
        let mut stopped = open.clone();
        stopped.e.id = 3;
        stopped.e.planned_stop = Some(9.0);
        let v = build(&ordered(vec![a, open, stopped]), &rules(), true, None, 0);
        assert_eq!(flags_of(&v, 1), vec!["unplanned"]);
        assert_eq!(flags_of(&v, 2), vec!["no_stop"]);
        assert!(flags_of(&v, 3).is_empty(), "写了止损的持仓不进清单");
        assert_eq!(v["items"][0]["date"], "2026-10-16", "按日期倒序");
        assert_eq!(v["items"][0]["closed"], false);
    }

    #[test]
    fn excursion_flags_use_cached_bars_only_and_report_what_was_skipped() {
        let rows = ordered(vec![closed(1, 100.0, 10.2, 1), closed(2, 100.0, 9.9, 1)]);
        let mut map = BTreeMap::new();
        map.insert(
            "mobile:1".to_string(),
            Lite {
                mfe_pct: 12.0,
                mae_pct: -9.0,
                realized_pct: 2.0,
                capture_rate: Some(0.167),
                same_day: false,
            },
        );
        map.insert(
            "mobile:2".to_string(),
            Lite {
                mfe_pct: 10.0,
                mae_pct: -1.0,
                realized_pct: -1.0,
                capture_rate: Some(-0.1),
                same_day: true,
            },
        );
        let v = build(&rows, &rules(), true, Some(&map), 3);
        assert_eq!(flags_of(&v, 1), vec!["deep_mae", "low_capture"]);
        assert_eq!(flags_of(&v, 2), vec!["low_capture"]);
        let texts: Vec<String> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|i| {
                i["flags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| f["text"].as_str().unwrap().to_string())
            })
            .collect();
        assert!(
            texts.contains(&"中途最多浮亏 -9.0%(最后还是赚的)".to_string()),
            "{texts:?}"
        );
        assert!(texts.contains(&"这段最多能赚 12.0%,实际落袋 2.0%(吃到 17%)".to_string()));
        assert!(texts
            .iter()
            .any(|t| t.ends_with(";同日进出,最大浮盈只是上界")));
        assert_eq!(
            (
                v["excursion_available"].clone(),
                v["excursion_skipped"].clone()
            ),
            (json!(true), json!(3))
        );
        assert!(v["excursion_hint"]
            .as_str()
            .unwrap()
            .contains("3 笔的行情还没缓存"));
        let none = build(&rows, &rules(), true, None, 0);
        assert_eq!(
            (
                none["excursion_available"].clone(),
                none["excursion_hint"].clone()
            ),
            (json!(false), Value::Null)
        );
    }

    #[test]
    fn view_reads_the_database_without_network() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-jinbox-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        assert_eq!(view(&db).unwrap()["available"], false);
        crate::journal::act(&db, &json!({"action": "record", "symbol": "600519", "name": "贵州茅台", "as_planned": false,
            "fills": [{"type": "buy", "date": "2026-10-12", "price": 100, "shares": 100, "fee": 0}, {"type": "sell", "date": "2026-10-13", "price": 90, "shares": 100, "fee": 0}]})).unwrap();
        let v = view(&db).unwrap();
        assert_eq!(flags_of(&v, 1), vec!["over_loss_limit", "unplanned"]);
        assert_eq!(v["excursion_skipped"], 1, "卖光了但没取过行情:如实报出来");
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
