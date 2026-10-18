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
        if code.starts_with("30") && b.date.as_str() < "2020-08-24" {
            b.limit_override = Some(0.10);
        }
        if let Some(r) = rules
            .iter()
            .find(|r| r.symbol == code && r.start <= b.date && r.end >= b.date)
        {
            b.limit_override = Some(r.limit);
        }
    }
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
        assert_eq!(bars[0].limit(Some(0.2)), Some(0.1));
        assert_eq!(bars[1].limit(Some(0.2)), None);
        let complete = complete_calendar(&bars, &[("2020-08-24".into(), 1.0)]);
        assert_eq!(complete.len(), 3);
        assert!(!complete[1].tradable);
    }
}
