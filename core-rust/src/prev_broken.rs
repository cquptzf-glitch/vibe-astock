//! 昨日炸板股今天修复了没(试错代价的直接读数),以及反馈矩阵里的「昨日炸板」一行。
//!
//! 昨日炸板股不在东财「昨日涨停表现」定稿记录里(那份只含昨日收盘仍封着的票),所以没法和其它档一起从定稿数据里算:
//! 这里取前一交易日的炸板池名单,再用**实时行情**对它们逐只算今天的涨跌。
//! 🔴 因为用的是实时行情,只有「行情快照属于复盘日」时才算得了(周末 / 盘前取到的是上一个交易日收盘 = 复盘日也行,
//!    但复盘日若是更早的某天,今天的价格和那天对不上,明说算不了,不拿别的日子顶替)。
//! 🔴 复盘日还没收盘(盘中)时是此刻的读数,标 partial,收盘前还会变。
//! 只列已经发生的客观读数,不预测、不给个股倾向。

use crate::{
    discovery::DiscoveryError,
    emotion::{self, PoolKind, Row},
    market_panel, watch,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;

const RESULTS: [&str; 5] = ["晋级涨停", "收红", "小跌", "跌超5%", "跌停"];

pub(crate) struct Quote {
    pub pct: Option<f64>,
    pub price: Option<f64>,
    pub limit_up: Option<f64>,
    pub limit_down: Option<f64>,
    /// 行情时间是不是复盘日(不是就不算:停牌、节假日的旧价)
    pub is_day: bool,
}

/// 一只票今天的结果档:涨停价上 → 晋级涨停;跌停价上 → 跌停;其余按涨跌幅
pub(crate) fn result_of(q: &Quote) -> Option<&'static str> {
    let (pct, price) = (q.pct?, q.price?);
    if q.limit_up.is_some_and(|l| price >= l - 1e-6) {
        return Some("晋级涨停");
    }
    if q.limit_down.is_some_and(|l| price <= l + 1e-6) {
        return Some("跌停");
    }
    Some(if pct > 0.0 {
        "收红"
    } else if pct > -5.0 {
        "小跌"
    } else {
        "跌超5%"
    })
}

/// 汇总:样本是前一交易日炸板池里取到「复盘日行情」的票;没取到行情的不进分母,单独说
pub(crate) fn summarize(pool: &[Row], quotes: &HashMap<String, Quote>) -> Value {
    let mut rets: Vec<f64> = Vec::new();
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let mut rows: Vec<Value> = Vec::new();
    let mut missing = 0usize;
    for r in pool {
        let Some(q) = quotes.get(&r.code).filter(|q| q.is_day && q.pct.is_some()) else {
            missing += 1;
            continue;
        };
        let (Some(pct), Some(res)) = (q.pct, result_of(q)) else {
            continue;
        };
        rets.push(pct);
        *counts.entry(res).or_default() += 1;
        rows.push(json!({"code": r.code, "name": r.name, "pct": pct, "result": res, "broken_times": r.broken_times, "sector": r.sector}));
    }
    rows.sort_by(|a, b| {
        b["pct"]
            .as_f64()
            .partial_cmp(&a["pct"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = rets.len();
    let total = pool.len();
    if n == 0 {
        return json!({"available": false, "reason": format!("前一交易日炸板池 {total} 只,一只都没取到复盘日的行情"), "total": total, "missing": missing});
    }
    let avg = rets.iter().sum::<f64>() / n as f64;
    let positive = rets.iter().filter(|v| **v > 0.0).count();
    let mut feedback = serde_json::Map::new();
    feedback.insert("tier".into(), json!("昨日炸板"));
    feedback.insert("合计".into(), json!(n));
    for k in RESULTS {
        feedback.insert(k.into(), json!(counts.get(k).copied().unwrap_or(0)));
    }
    feedback.insert(
        "晋级率".into(),
        json!(emotion::r3(
            counts.get("晋级涨停").copied().unwrap_or(0) as f64 / n as f64
        )),
    );
    json!({
        "available": true, "total": total, "sample": n, "missing": missing, "coverage_rate": emotion::r3(n as f64 / total as f64),
        "avg": emotion::r2(avg), "median": emotion::r2(emotion::median(&rets)), "positive_rate": emotion::r3(positive as f64 / n as f64),
        "limit_up_again": counts.get("晋级涨停").copied().unwrap_or(0), "loss5": counts.get("跌超5%").copied().unwrap_or(0) + counts.get("跌停").copied().unwrap_or(0),
        "feedback_row": Value::Object(feedback), "result_order": RESULTS, "rows": rows,
    })
}

pub async fn fetch(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, prev) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    let settled = emotion::is_settled(&date, &calendar, now);
    let today = now.0.format("%Y-%m-%d").to_string();
    let snapshot_day = market_panel::snapshot_day_of(&calendar, &today, now.1);
    let mut body = json!({"date": date, "prev_date": prev, "settled": settled, "partial": !settled && date == today,
        "source": "前一交易日东财炸板池 + 腾讯实时行情(复盘日当天的涨跌)", "note": "昨日炸板股不在东财「昨日涨停表现」定稿记录里,所以用前一交易日炸板池名单加实时行情算;只在行情快照就是复盘日时才算得了。"});
    let Some(prev) = prev else {
        body["result"] = json!({"available": false, "reason": "交易日历里找不到前一交易日"});
        return Ok(body);
    };
    if snapshot_day.as_deref() != Some(date.as_str()) {
        body["result"] = json!({"available": false, "reason": format!("实时行情现在属于 {},和复盘日 {date} 不是同一天,今天的价格对不上那天的炸板股,不拿别的日子顶替", snapshot_day.as_deref().unwrap_or("未知"))});
        return Ok(body);
    }
    let pool = match emotion::fetch_pool(client, PoolKind::Broken, &prev.replace('-', "")).await {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => {
            body["result"] = json!({"available": false, "reason": format!("{prev} 的炸板池是空的(超过东财保留期或数据还没出)")});
            return Ok(body);
        }
        Err(e) => {
            body["result"] = json!({"available": false, "reason": emotion::error_message(&format!("{prev} 炸板池"), &e)});
            return Ok(body);
        }
    };
    let codes: Vec<String> = pool.iter().map(|r| r.code.clone()).collect();
    let (raw, failed, batches) = watch::fetch_quotes(client, &codes).await;
    let quotes: HashMap<String, Quote> = raw
        .into_iter()
        .map(|(code, q)| {
            (
                code,
                Quote {
                    pct: q.pct,
                    price: q.price,
                    limit_up: q.limit_up,
                    limit_down: q.limit_down,
                    is_day: q.quote_time.len() == 14
                        && q.quote_time.starts_with(&date.replace('-', "")),
                },
            )
        })
        .collect();
    let mut result = summarize(&pool, &quotes);
    if failed > 0 {
        result["note"] = json!(format!("行情分 {batches} 批取,{failed} 批没取到,样本不全"));
    }
    body["result"] = result;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str) -> Row {
        Row {
            code: code.into(),
            name: format!("N{code}"),
            broken_times: 1,
            ..Row::default()
        }
    }

    fn q(pct: f64, price: f64, up: Option<f64>, down: Option<f64>, is_day: bool) -> Quote {
        Quote {
            pct: Some(pct),
            price: Some(price),
            limit_up: up,
            limit_down: down,
            is_day,
        }
    }

    #[test]
    fn results_use_the_limit_price_first_then_the_change() {
        assert_eq!(
            result_of(&q(10.0, 11.0, Some(11.0), Some(9.0), true)),
            Some("晋级涨停")
        );
        assert_eq!(
            result_of(&q(-10.0, 9.0, Some(11.0), Some(9.0), true)),
            Some("跌停")
        );
        assert_eq!(
            result_of(&q(3.0, 10.3, Some(11.0), Some(9.0), true)),
            Some("收红")
        );
        assert_eq!(
            result_of(&q(-3.0, 9.7, Some(11.0), Some(9.0), true)),
            Some("小跌")
        );
        assert_eq!(
            result_of(&q(0.0, 10.0, None, None, true)),
            Some("小跌"),
            "平盘算小跌(不是收红)"
        );
        assert_eq!(
            result_of(&q(-6.0, 9.4, Some(11.0), Some(9.0), true)),
            Some("跌超5%")
        );
        assert_eq!(
            result_of(&Quote {
                pct: None,
                price: None,
                limit_up: None,
                limit_down: None,
                is_day: true
            }),
            None
        );
    }

    #[test]
    fn summary_only_counts_quotes_dated_on_the_review_day() {
        let pool: Vec<Row> = ["1", "2", "3", "4", "5"].iter().map(|c| row(c)).collect();
        let mut quotes: HashMap<String, Quote> = HashMap::new();
        quotes.insert("1".into(), q(10.0, 11.0, Some(11.0), Some(9.0), true));
        quotes.insert("2".into(), q(4.0, 10.4, Some(11.0), Some(9.0), true));
        quotes.insert("3".into(), q(-6.0, 9.4, Some(11.0), Some(9.0), true));
        quotes.insert("4".into(), q(8.0, 10.8, Some(11.0), Some(9.0), false)); // 停牌 / 旧价,不算
                                                                               // 5 没取到行情
        let v = summarize(&pool, &quotes);
        assert_eq!(
            (
                v["sample"].clone(),
                v["missing"].clone(),
                v["total"].clone(),
                v["coverage_rate"].clone()
            ),
            (json!(3), json!(2), json!(5), json!(0.6))
        );
        assert_eq!(
            (
                v["avg"].clone(),
                v["median"].clone(),
                v["positive_rate"].clone(),
                v["limit_up_again"].clone(),
                v["loss5"].clone()
            ),
            (json!(2.67), json!(4.0), json!(0.667), json!(1), json!(1))
        );
        assert_eq!(v["feedback_row"]["tier"], "昨日炸板");
        assert_eq!(
            (
                v["feedback_row"]["合计"].clone(),
                v["feedback_row"]["晋级涨停"].clone(),
                v["feedback_row"]["收红"].clone(),
                v["feedback_row"]["跌超5%"].clone(),
                v["feedback_row"]["晋级率"].clone()
            ),
            (json!(3), json!(1), json!(1), json!(1), json!(0.333))
        );
        assert_eq!(v["rows"][0]["code"], "1", "按今日涨幅从高到低");
        let none = summarize(&pool, &HashMap::new());
        assert_eq!(none["available"], false);
        assert!(none["reason"].as_str().unwrap().contains("一只都没取到"));
    }

    /// 对真实上游:cargo test --locked prev_broken::tests::live_prev_broken_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_prev_broken_probe() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-prevbroken-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let get = |path: &str| crate::MobileApiRequest {
            path: path.into(),
            method: "GET".into(),
            body: Value::Null,
        };
        let r = app.handle(get("/mobile/shortline/prev-broken")).await;
        println!(
            "status {} date {} prev {} result {}",
            r.status,
            r.body["date"],
            r.body["prev_date"],
            serde_json::to_string(&r.body["result"])
                .unwrap()
                .chars()
                .take(500)
                .collect::<String>()
        );
        if let Ok(out) = std::env::var("VIBE_PREV_BROKEN_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&r.body).unwrap()).unwrap();
        }
        assert_eq!(r.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
