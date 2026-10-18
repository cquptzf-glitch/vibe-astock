use super::*;
use std::collections::{BTreeMap, HashSet};
fn surprise(a: &Value) -> R<Value> {
    let actual = n(a, "actual")?;
    let mut estimate = opt(a, "estimate")?;
    let low = opt(a, "low")?;
    let high = opt(a, "high")?;
    if low.is_some() != high.is_some() {
        return Err("low 和 high 要么都给，要么都不给".into());
    }
    if let (Some(l), Some(h)) = (low, high) {
        if l > h {
            return Err("low 大于 high".into());
        }
    }
    let midpoint = estimate.is_none();
    if estimate.is_none() {
        estimate = Some((low.ok_or("estimate 和区间至少给一个")? + high.unwrap()) / 2.);
    }
    let estimate = estimate.unwrap();
    let unit = text(a, "unit", "")?;
    let mut details = json!({"actual":actual,"estimate":v(estimate),"estimate_is_midpoint":midpoint,"difference":v(actual-estimate),"low":low,"high":high,"unit_in":unit});
    if let (Some(lo), Some(hi)) = (low, high) {
        let position = if actual < lo {
            "below"
        } else if actual > hi {
            "above"
        } else {
            "within"
        };
        let point = lo == hi;
        let label = match (position, point) {
            ("below", true) => "低于预告值",
            ("within", true) => "等于预告值",
            ("above", true) => "高于预告值",
            ("below", false) => "低于区间下限",
            ("above", false) => "高于区间上限",
            _ => "落在区间内",
        };
        details["position"] = json!(position);
        details["point"] = json!(point);
        details["position_label"] = json!(label);
    }
    Ok(if estimate == 0. {
        nm("小数", "预期为 0,偏差比例无意义", details)
    } else {
        ok((actual - estimate) / estimate.abs(), "小数", details)
    })
}
fn reaction(a: &Value) -> R<Value> {
    let rows = super::indicators::rows(&a["klines"], &["close"], &[])?;
    let events = a["events"]
        .as_array()
        .filter(|s| !s.is_empty() && s.len() <= 40)
        .ok_or("events 必须为一至四十项的列表")?;
    let mut ids = HashSet::new();
    let mut evs = Vec::new();
    for e in events {
        let id = e["id"]
            .as_str()
            .filter(|id| !id.trim().is_empty() && id.chars().count() <= 80)
            .ok_or("事件 id 须为一至八十字")?;
        if !ids.insert(id) {
            return Err("事件 id 重复".into());
        }
        let day = super::indicators::day(&e["date"])?;
        let timing = e["timing"]
            .as_str()
            .filter(|s| ["before_open", "during", "after_close"].contains(s))
            .ok_or("事件 timing 非法")?;
        evs.push(json!({"id":id,"date":day,"timing":timing}));
    }
    evs.sort_by(|a, b| {
        a["date"]
            .as_str()
            .cmp(&b["date"].as_str())
            .then(a["id"].as_str().cmp(&b["id"].as_str()))
    });
    let lookback = integer(a, "lookback", 4, 1, 12)?;
    if rows.len() < 2 {
        return Err("klines 至少要两根".into());
    }
    let mut groups = BTreeMap::<String, Value>::new();
    let mut reactions = Vec::new();
    for e in evs {
        let day = e["date"].as_str().unwrap();
        let idx = rows.partition_point(|r| {
            if e["timing"] == "after_close" {
                r.day.as_str() <= day
            } else {
                r.day.as_str() < day
            }
        });
        if idx >= rows.len() || idx == 0 {
            let pending = idx >= rows.len();
            let reason = if pending {
                format!(
                    "反应日还没到:K 线只到 {}(反应待定)",
                    rows.last().unwrap().day
                )
            } else {
                format!("K 线不够早:反应日 {} 之前没有日 K 可比", rows[0].day)
            };
            reactions.push(json!({"status":"not_meaningful","value":null,"unit":"小数","pending":pending,"reason":reason,"id":e["id"],"ids":[e["id"]],"date":day,"events":[e],"reaction_day":if pending{Value::Null}else{json!(rows[0].day)},"prev_day":null,"prev_close":null,"close":null,"gap_days":null}));
            continue;
        }
        let (rd, prev) = (&rows[idx], &rows[idx - 1]);
        if let Some(g) = groups.get_mut(&rd.day) {
            g["events"].as_array_mut().unwrap().push(e.clone());
            g["ids"].as_array_mut().unwrap().push(e["id"].clone());
            let joined = g["ids"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" + ");
            g["id"] = json!(joined);
            continue;
        }
        groups.insert(rd.day.clone(),json!({"status":"ok","value":v(rd.values["close"]/prev.values["close"]-1.),"unit":"小数","reason":"","pending":false,"id":e["id"],"ids":[e["id"]],"date":day,"events":[e],"reaction_day":rd.day,"prev_day":prev.day,"prev_close":prev.values["close"],"close":rd.values["close"],"gap_days":(super::series::date(&rd.day)?-super::series::date(day)?).num_days()}));
    }
    reactions.extend(groups.into_values());
    if reactions.iter().any(nonfinite) {
        return Err("财报反应计算溢出".into());
    }
    reactions.sort_by(|a, b| {
        b["date"]
            .as_str()
            .cmp(&a["date"].as_str())
            .then(b["id"].as_str().cmp(&a["id"].as_str()))
    });
    let latest = &reactions[0];
    let prior: Vec<_> = reactions[1..]
        .iter()
        .filter(|r| r["status"] == "ok")
        .take(lookback)
        .collect();
    let n = prior.len();
    let mean = if n > 0 {
        Some(
            prior
                .iter()
                .map(|r| r["value"].as_f64().unwrap().abs())
                .sum::<f64>()
                / n as f64,
        )
    } else {
        None
    };
    let (reason, ratio) = if latest["status"] != "ok" {
        ("最近一次没有算出涨跌".to_string(), None)
    } else if n < 2 {
        (format!("此前算得出的只有 {n} 次,少于 2 次不比"), None)
    } else if mean == Some(0.) {
        ("此前平均绝对涨跌为 0".into(), None)
    } else {
        (
            "".into(),
            Some(latest["value"].as_f64().unwrap().abs() / mean.unwrap()),
        )
    };
    let ratio = json!({"status":if ratio.is_some(){"ok"}else{"not_meaningful"},"value":ratio.map(v),"unit":"倍","reason":reason});
    let details = json!({"reactions":reactions,"latest_id":latest["id"],"prior_ids":prior.iter().map(|r|r["id"].clone()).collect::<Vec<_>>(),"prior_n":n,"lookback":lookback,"latest_abs_over_mean":ratio,"bars":{"first":rows[0].day,"last":rows.last().unwrap().day,"count":rows.len()},"note":"涨跌 = 反应日收盘 ÷ 前一交易日收盘 − 1,含当天大盘涨跌;只描述过去,不预测下一次"});
    Ok(if n < 2 {
        nm(
            "小数",
            format!("此前算得出的只有 {n} 次,少于 2 次不算平均"),
            details,
        )
    } else {
        out("ok", mean, "小数", "", details)
    })
}
pub(super) fn run(function: &str, args: &Value) -> R<Value> {
    if function == "earnings_surprise" {
        surprise(args)
    } else {
        reaction(args)
    }
}
