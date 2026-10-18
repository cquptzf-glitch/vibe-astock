//! Point-in-time rule overrides. Current ST names are never projected into the past.
use super::*;
use chrono::NaiveDate;
#[derive(Clone)]
pub(super) struct Rule {
    symbol: String,
    start: String,
    end: String,
    limit: f64,
}
pub(super) fn range(body: &Value) -> Result<(Option<String>, Option<String>)> {
    let date = |key: &str| -> Result<Option<String>> {
        match body.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => {
                let text = v
                    .as_str()
                    .ok_or_else(|| anyhow!("{key} 必须是 YYYY-MM-DD"))?;
                let d = NaiveDate::parse_from_str(text, "%Y-%m-%d")?;
                if d.to_string() != text || d > crate::emotion::now_china().0 {
                    return Err(anyhow!("{key} 日期无效或在未来"));
                }
                Ok(Some(text.into()))
            }
        }
    };
    let start = date("start_date")?;
    let end = date("end_date")?;
    if start.as_ref().zip(end.as_ref()).is_some_and(|(a, b)| a > b) {
        return Err(anyhow!("开始日期不能晚于结束日期"));
    }
    Ok((start, end))
}
pub(super) fn parse(body: &Value, symbols: &[crate::SymbolKey]) -> Result<Vec<Rule>> {
    let Some(raw) = body.get("market_rules") else {
        return Ok(vec![]);
    };
    let rows = raw
        .as_array()
        .filter(|r| r.len() <= 100)
        .ok_or_else(|| anyhow!("market_rules 必须是最多 100 段的数组"))?;
    let mut out: Vec<Rule> = Vec::new();
    for r in rows {
        let code = crate::normalize_stock_symbol(r["symbol"].as_str().unwrap_or(""))?.digits;
        if !symbols.iter().any(|s| s.digits == code) {
            return Err(anyhow!("规则代码不在本次回测中"));
        }
        let start = r["start"].as_str().unwrap_or("");
        let end = r["end"].as_str().unwrap_or("");
        for d in [start, end] {
            if NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .is_none_or(|x| x.to_string() != d)
            {
                return Err(anyhow!("制度区间日期无效"));
            }
        }
        if start > end || end > crate::emotion::now_china().0.to_string().as_str() {
            return Err(anyhow!("制度区间倒置或在未来"));
        }
        let limit = r["limit_pct"]
            .as_f64()
            .filter(|v| v.is_finite() && (*v == 0.0 || (1.0..=100.0).contains(v)))
            .ok_or_else(|| anyhow!("limit_pct 必须为 0(不设涨跌幅限制)或 1–100"))?
            / 100.0;
        if out
            .iter()
            .any(|x| x.symbol == code && x.start.as_str() <= end && x.end.as_str() >= start)
        {
            return Err(anyhow!("同一股票的制度区间不能重叠"));
        }
        out.push(Rule {
            symbol: code,
            start: start.into(),
            end: end.into(),
            limit,
        });
    }
    Ok(out)
}
pub(super) fn apply(code: &str, data: &mut [Bar], rules: &[Rule]) {
    for b in data {
        // ChiNext reform applies historically, rather than today's rate for the full sample.
        if code.starts_with("30") && b.date.as_str() < "2020-08-24" && b.limit_override.is_none() {
            b.limit_override = Some((0.10, 0.10));
        }
        if let Some(r) = rules
            .iter()
            .find(|r| r.symbol == code && r.start <= b.date && r.end >= b.date)
        {
            b.limit_override = Some((r.limit, r.limit));
        }
    }
}
/// Historical states, followed by user-supplied intervals (the explicit override wins).
pub(super) fn automatic(
    code: &str,
    data: &mut [Bar],
    h: &crate::historical_metadata::History,
) -> Value {
    let mut covered = 0;
    let mut missing = Vec::new();
    let mut ipo_count = 0;
    for b in data.iter_mut() {
        if b.date.as_str() < h.ipo.to_string().as_str()
            || h.delisted.is_some_and(|d| b.date > d.to_string())
        {
            b.tradable = false;
            continue;
        }
        let Some(day) = h.days.get(&b.date) else {
            missing.push(b.date.clone());
            continue;
        };
        covered += 1;
        b.tradable &= day.tradable;
        let growth = code.starts_with("30");
        let star = code.starts_with("688") || code.starts_with("689");
        let limit = if star || (growth && b.date.as_str() >= "2020-08-24") {
            0.20
        } else if day.st && b.date.as_str() < "2026-07-06" {
            0.05
        } else {
            0.10
        };
        b.limit_override = Some((limit, limit));
        if let Some(index) = h.ipo_days.iter().position(|d| d == &b.date) {
            let modern = star
                || (growth && h.ipo.to_string().as_str() >= "2020-08-24")
                || (!growth && h.ipo.to_string().as_str() >= "2023-04-10");
            if modern {
                b.limit_override = Some((0.0, 0.0));
                ipo_count += 1;
            } else if index == 0 && h.ipo.to_string().as_str() >= "2014-01-01" {
                b.limit_override = Some((0.44, 0.36));
                ipo_count += 1;
            } else if index == 0 {
                missing.push(b.date.clone());
            }
        }
    }
    let mut p = crate::historical_metadata::provenance(h);
    p["status"] = json!(if missing.is_empty() { "ok" } else { "partial" });
    p["covered_bars"] = json!(covered);
    p["missing_dates"] = json!(missing);
    p["ipo_rule_bars"] = json!(ipo_count);
    p["scope"]=json!("按逐日 isST、tradestatus 与真实上市日应用沪深制度；主板风险警示幅度自2026-07-06由5%变10%；旧制新股首日+44%/-36%。不推断重新上市、退市整理等特殊制度；开盘成交可达性仍为日线近似。");
    p
}
pub(super) fn complete_calendar(data: &[Bar], calendar: &[(String, f64)]) -> Vec<Bar> {
    let (Some(first), Some(last)) = (data.first(), data.last()) else {
        return vec![];
    };
    let by: std::collections::BTreeMap<_, _> = data.iter().map(|b| (&b.date, b)).collect();
    let mut dates: std::collections::BTreeSet<_> = by.keys().copied().collect();
    dates.extend(
        calendar
            .iter()
            .map(|(d, _)| d)
            .filter(|d| *d >= &first.date && *d <= &last.date),
    );
    let mut previous = first.clone();
    dates
        .into_iter()
        .map(|d| match by.get(d) {
            Some(b) => {
                previous = (*b).clone();
                previous.clone()
            }
            None => Bar {
                date: d.clone(),
                open: previous.close,
                close: previous.close,
                tradable: false,
                limit_override: previous.limit_override,
            },
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample(date: &str) -> Bar {
        Bar {
            date: date.into(),
            open: 10.,
            close: 10.,
            tradable: true,
            limit_override: None,
        }
    }
    fn history(
        ipo: &str,
        dates: &[(&str, bool, bool)],
        ipo_days: &[&str],
    ) -> crate::historical_metadata::History {
        crate::historical_metadata::History {
            ipo: NaiveDate::parse_from_str(ipo, "%Y-%m-%d").unwrap(),
            delisted: None,
            days: dates
                .iter()
                .map(|(d, st, tradable)| {
                    (
                        d.to_string(),
                        crate::historical_metadata::Day {
                            st: *st,
                            tradable: *tradable,
                        },
                    )
                })
                .collect(),
            ipo_days: ipo_days.iter().map(|s| s.to_string()).collect(),
        }
    }
    #[test]
    fn st_changes_and_chinext_reform_use_historical_dates_and_explicit_override_wins() {
        let h = history(
            "2001-01-01",
            &[
                ("2026-07-03", true, true),
                ("2026-07-06", true, true),
                ("2026-07-07", false, false),
            ],
            &[],
        );
        let mut bars = vec![
            sample("2026-07-03"),
            sample("2026-07-06"),
            sample("2026-07-07"),
            sample("2026-07-08"),
        ];
        let a = automatic("600000", &mut bars, &h);
        assert_eq!(a["status"], "partial");
        assert_eq!(a["missing_dates"], json!(["2026-07-08"]));
        assert_eq!(bars[0].limit(Some(0.1), true), Some(0.05));
        assert_eq!(bars[1].limit(Some(0.1), true), Some(0.1));
        assert!(!bars[2].tradable);
        let h = history(
            "2001-01-01",
            &[("2020-08-21", true, true), ("2020-08-24", true, true)],
            &[],
        );
        let mut bars = vec![sample("2020-08-21"), sample("2020-08-24")];
        automatic("300001", &mut bars, &h);
        apply("300001", &mut bars, &[]);
        assert_eq!(bars[0].limit(Some(0.2), true), Some(0.05));
        assert_eq!(bars[1].limit(Some(0.2), true), Some(0.2));
        let rules=parse(&json!({"market_rules":[{"symbol":"300001","start":"2020-08-21","end":"2020-08-21","limit_pct":0}]}),&[crate::normalize_stock_symbol("300001").unwrap()]).unwrap();
        apply("300001", &mut bars, &rules);
        assert_eq!(bars[0].limit(Some(0.2), true), None);
        assert_eq!(bars[0].limit(None, true), None);
    }
    #[test]
    fn ipo_windows_are_anchored_at_listing_not_at_start_of_backtest() {
        let h = history(
            "2023-04-10",
            &[
                ("2023-04-10", false, true),
                ("2023-04-14", false, true),
                ("2023-04-17", false, true),
            ],
            &[
                "2023-04-10",
                "2023-04-11",
                "2023-04-12",
                "2023-04-13",
                "2023-04-14",
            ],
        );
        let mut bars = vec![sample("2023-04-14"), sample("2023-04-17")];
        automatic("601000", &mut bars, &h);
        assert_eq!(bars[0].limit(Some(0.1), true), None);
        assert_eq!(bars[1].limit(Some(0.1), true), Some(0.1));
        let h = history(
            "2018-05-03",
            &[("2018-05-03", false, true), ("2018-05-04", false, true)],
            &["2018-05-03", "2018-05-04"],
        );
        let mut bars = vec![sample("2018-05-03"), sample("2018-05-04")];
        automatic("603000", &mut bars, &h);
        assert_eq!(bars[0].limit(Some(0.1), true), Some(0.44));
        assert_eq!(bars[0].limit(Some(0.1), false), Some(0.36));
        assert_eq!(bars[1].limit(Some(0.1), false), Some(0.1));
    }
    #[test]
    fn range_rejects_future_reversed_and_noncanonical_dates() {
        for body in [
            json!({"start_date":"2099-01-01"}),
            json!({"start_date":"2026-02-02","end_date":"2026-02-01"}),
            json!({"start_date":"2026-2-2"}),
        ] {
            assert!(range(&body).is_err());
        }
        assert_eq!(
            range(&json!({"start_date":"2026-01-01","end_date":"2026-02-01"})).unwrap(),
            (Some("2026-01-01".into()), Some("2026-02-01".into()))
        );
    }
    #[test]
    fn historical_rules_and_explicit_unlimited_periods_are_respected() {
        let key = crate::normalize_stock_symbol("300308").unwrap();
        let rules=parse(&json!({"market_rules":[{"symbol":"300308","start":"2026-01-01","end":"2026-01-05","limit_pct":0}]}),&[key]).unwrap();
        let mut bars = vec![
            Bar {
                date: "2020-08-21".into(),
                open: 10.0,
                close: 10.0,
                tradable: true,
                limit_override: None,
            },
            Bar {
                date: "2026-01-02".into(),
                open: 10.0,
                close: 10.0,
                tradable: true,
                limit_override: None,
            },
        ];
        apply("300308", &mut bars, &rules);
        assert_eq!(bars[0].limit(Some(0.2), true), Some(0.1));
        assert_eq!(bars[1].limit(Some(0.2), true), None);
        let complete = complete_calendar(&bars, &[("2020-08-24".into(), 1.0)]);
        assert_eq!(complete.len(), 3);
        assert!(!complete[1].tradable);
    }
}
