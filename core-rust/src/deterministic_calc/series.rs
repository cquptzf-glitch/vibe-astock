use super::*;
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;
pub(super) fn date(s: &str) -> R<NaiveDate> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| "日期格式或日历日非法")?;
    if s.len() != 10 || d.to_string() != s || d.year() < 1 || d.year() > 9999 {
        return Err("日期须为严格 YYYY-MM-DD".into());
    }
    Ok(d)
}
fn quarter(s: &str) -> R<NaiveDate> {
    let d = date(s)?;
    if !matches!((d.month(), d.day()), (3, 31) | (6, 30) | (9, 30) | (12, 31)) {
        return Err("报告期不是季末日".into());
    }
    Ok(d)
}
fn previous(d: NaiveDate) -> R<NaiveDate> {
    let (y, m, day) = match d.month() {
        3 => (d.year() - 1, 12, 31),
        6 => (d.year(), 3, 31),
        9 => (d.year(), 6, 30),
        _ => (d.year(), 9, 30),
    };
    if y < 1 {
        return Err("报告期超出支持范围".into());
    }
    NaiveDate::from_ymd_opt(y, m, day).ok_or_else(|| "报告期非法".into())
}
fn backwards(d: NaiveDate, count: usize) -> R<Vec<NaiveDate>> {
    let mut p = d;
    let mut rows = Vec::new();
    for i in 0..count {
        rows.push(p);
        if i + 1 < count {
            p = previous(p)?;
        }
    }
    Ok(rows)
}
fn unit(a: &Value) -> R<&str> {
    let u = text(a, "unit", "元")?;
    if u.trim().is_empty() {
        return Err("unit 必须是非空字符串".into());
    }
    if boolean(a, "money", false)? {
        money_unit(u)?;
    }
    Ok(u)
}
fn items(a: &Value, key: &str) -> R<BTreeMap<NaiveDate, Option<f64>>> {
    let list = a[key]
        .as_array()
        .ok_or_else(|| format!("{key} 必须是列表"))?;
    let mut rows = BTreeMap::new();
    for row in list {
        let d = quarter(row["period"].as_str().ok_or("缺 period")?)?;
        let value = opt(row, "value")?;
        if rows.insert(d, value).is_some() {
            return Err("报告期重复".into());
        }
    }
    Ok(rows)
}
type Span = (NaiveDate, NaiveDate);
type Spans = BTreeMap<Span, (i64, Option<f64>)>;
fn span(s: &str) -> R<(Span, i64)> {
    let (s, e) = s.split_once("..").ok_or("period 须为 起始日..截止日")?;
    let s = date(s)?;
    let e = date(e)?;
    if e <= s {
        return Err("截止日不晚于起始日".into());
    }
    let months = (((e - s).num_days() + 1) as f64 / 30.44).round() as i64;
    Ok(((s, e), months))
}
fn span_text(p: Span) -> String {
    format!("{}..{}", p.0, p.1)
}
fn span_items(a: &Value) -> R<Spans> {
    let mut map = BTreeMap::new();
    for row in a["cumulative"].as_array().ok_or("cumulative 必须是列表")? {
        let (p, months) = span(row["period"].as_str().ok_or("缺 period")?)?;
        if map.insert(p, (months, opt(row, "value")?)).is_some() {
            return Err("累计期间重复".into());
        }
    }
    Ok(map)
}
fn ttm_at(m: &Spans, key: Span, u: &str) -> (Value, Option<Span>) {
    let (months, cur) = m[&key];
    let Some(cur) = cur else {
        return (
            nm(u, format!("{} 累计值缺失", span_text(key)), json!({})),
            None,
        );
    };
    let last = key.0.pred_opt();
    let fy: Vec<_> = m
        .iter()
        .filter(|(k, v)| Some(k.1) == last && v.0 == 12)
        .map(|(k, _)| *k)
        .collect();
    if months == 12 {
        return (
            ok(
                cur,
                u,
                json!({"method":"full_year","periods":{"current":span_text(key)}}),
            ),
            if fy.len() == 1 { Some(fy[0]) } else { None },
        );
    }
    if ![3, 6, 9].contains(&months) {
        return (
            nm(
                u,
                format!(
                    "{} 跨 {months} 个月,不是 3 / 6 / 9 / 12 个月的累计期",
                    span_text(key)
                ),
                json!({}),
            ),
            None,
        );
    }
    if fy.len() != 1 {
        return (nm(u, "缺上一完整财年或上一财年不唯一", json!({})), None);
    }
    let same: Vec<_> = m
        .iter()
        .filter(|(k, v)| k.0 == fy[0].0 && v.0 == months)
        .map(|(k, _)| *k)
        .collect();
    if same.len() != 1 {
        return (nm(u, "缺上年同期累计或上年同期不唯一", json!({})), None);
    }
    let (Some(fv), Some(sv)) = (m[&fy[0]].1, m[&same[0]].1) else {
        return (nm(u, "上一财年或上年同期累计值缺失", json!({})), None);
    };
    (
        ok(
            cur + fv - sv,
            u,
            json!({"method":"ytd_bridge","periods":{"current":span_text(key),"prior_full_year":span_text(fy[0]),"prior_same_period":span_text(same[0])},"values":{"current":cur,"prior_full_year":fv,"prior_same_period":sv},"formula":"当期累计 + 上一财年 − 上年同期累计"}),
        ),
        Some(same[0]),
    )
}
fn yoy(cur: Value, base: Value, extra: Value) -> Value {
    if base["status"] != "ok" {
        return out(
            base["status"].as_str().unwrap_or("error"),
            None,
            "小数",
            format!("基期 TTM:{}", base["reason"].as_str().unwrap_or("")),
            json!({"current":cur,"base":base}),
        );
    }
    if base["value"].as_f64().unwrap_or(0.) <= 0. {
        return nm(
            "小数",
            "基期 TTM ≤ 0,同比无意义",
            json!({"current":cur,"base":base}),
        );
    }
    let value = cur["value"].as_f64().unwrap() / base["value"].as_f64().unwrap() - 1.;
    let mut details = json!({"current":cur,"base":base});
    if let Some(map) = extra.as_object() {
        for (k, v) in map {
            details[k] = v.clone();
        }
    }
    ok(value, "小数", details)
}
pub(super) fn run(function: &str, a: &Value) -> R<Value> {
    if function == "ttm_yoy" {
        let cur = run("ttm_sum", a).unwrap_or_else(error);
        if cur["status"] != "ok" {
            return Ok(out(
                cur["status"].as_str().unwrap_or("error"),
                None,
                "小数",
                format!("当期 TTM:{}", cur["reason"].as_str().unwrap_or("")),
                json!({"current":cur}),
            ));
        }
        let end = quarter(text(a, "end_period", "")?)?;
        let base_end = backwards(end, 5)?[4];
        let mut arg = a.clone();
        arg["end_period"] = json!(base_end.to_string());
        return Ok(yoy(
            cur,
            run("ttm_sum", &arg).unwrap_or_else(error),
            json!({}),
        ));
    }
    let u = unit(a)?;
    if matches!(function, "ttm_from_ytd" | "ttm_yoy_from_ytd") {
        let m = span_items(a)?;
        let end = text(a, "end_period", "")?;
        let (key, _) = span(end)?;
        if !m.contains_key(&key) {
            return Err("end_period 不在 cumulative 里".into());
        }
        let (mut cur, prior) = ttm_at(&m, key, u);
        if function == "ttm_from_ytd" {
            if cur["status"] == "ok" {
                cur["details"]["end_period"] = json!(end);
            }
            return Ok(cur);
        }
        if cur["status"] != "ok" {
            return Ok(out(
                cur["status"].as_str().unwrap_or("error"),
                None,
                "小数",
                format!("当期 TTM:{}", cur["reason"].as_str().unwrap_or("")),
                json!({"current":cur}),
            ));
        }
        let Some(prior) = prior else {
            return Ok(nm("小数", "找不到一年前的同一期", json!({"current":cur})));
        };
        let (base, _) = ttm_at(&m, prior, u);
        return Ok(yoy(
            cur,
            base,
            json!({"end_period":end,"base_period":span_text(prior)}),
        ));
    }
    let key = if function == "quarterize" {
        "cumulative"
    } else {
        "single_quarters"
    };
    let rows = items(a, key)?;
    Ok(match function {
        "quarterize" => {
            if rows.is_empty() {
                return Err("空序列".into());
            }
            let mut series = Vec::new();
            let mut count = 0;
            for (d, value) in &rows {
                let q = d.month() / 3;
                let result = match value {
                    None => {
                        json!({"period":d.to_string(),"quarter":q,"value":null,"reason":"累计值缺失"})
                    }
                    Some(value) if q == 1 => {
                        count += 1;
                        json!({"period":d.to_string(),"quarter":q,"value":value})
                    }
                    Some(value) => {
                        let prev = previous(*d)?;
                        if let Some(Some(p)) = rows.get(&prev) {
                            count += 1;
                            json!({"period":d.to_string(),"quarter":q,"value":v(value-p)})
                        } else {
                            json!({"period":d.to_string(),"quarter":q,"value":null,"reason":format!("上一报告期 {prev} 累计值缺失,无法拆单季")})
                        }
                    }
                };
                series.push(result);
            }
            out(
                if count > 0 { "ok" } else { "not_meaningful" },
                Some(count as f64),
                "期",
                if count > 0 {
                    ""
                } else {
                    "无任何可拆分单季"
                },
                json!({"series":series,"value_unit":u}),
            )
        }
        "latest_quarter" => {
            if let Some((d, value)) = rows.last_key_value() {
                let det = json!({"period":d.to_string(),"quarter":d.month()/3});
                if let Some(value) = value {
                    ok(*value, u, det)
                } else {
                    nm(
                        u,
                        format!("最新一季 {d} 单季值缺失,不拿更早的季度顶替"),
                        det,
                    )
                }
            } else {
                nm(u, "无可用单季值", json!({}))
            }
        }
        "ttm_sum" => {
            let end = quarter(text(a, "end_period", "")?)?;
            let periods = backwards(end, 4)?;
            let values: Vec<_> = periods
                .iter()
                .map(|d| rows.get(d).copied().flatten())
                .collect();
            let names: Vec<_> = periods.iter().map(ToString::to_string).collect();
            if values.iter().any(Option::is_none) {
                nm(
                    u,
                    "近四季有缺失，不拿少于四季冒充",
                    json!({"periods":names}),
                )
            } else {
                let values: Vec<f64> = values.into_iter().flatten().collect();
                ok(
                    values.iter().sum(),
                    u,
                    json!({"periods":names,"values":values}),
                )
            }
        }
        "qoq" => {
            let end = quarter(text(a, "end_period", "")?)?;
            let prev = previous(end)?;
            match (
                rows.get(&end).copied().flatten(),
                rows.get(&prev).copied().flatten(),
            ) {
                (Some(c), Some(p)) if p > 0. => ok(
                    c / p - 1.,
                    "小数",
                    json!({"current":c,"prev":p,"note":"拐点/动量信号,不作增速分母"}),
                ),
                (Some(c), Some(p)) => {
                    nm("小数", "前季 ≤ 0,环比无意义", json!({"current":c,"prev":p}))
                }
                _ => nm(
                    "小数",
                    "当季或前季缺失",
                    json!({"end":end.to_string(),"prev":prev.to_string()}),
                ),
            }
        }
        _ => return Err("未知序列函数".into()),
    })
}
