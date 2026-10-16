use crate::symbol::{normalize_stock_symbol, SymbolKey};
use anyhow::anyhow;
use chrono::{Datelike, Utc};
use reqwest::Client;
use serde_json::{json, Map, Value};

pub enum ResearchError {
    BadRequest(String),
    Upstream(anyhow::Error),
}

impl From<anyhow::Error> for ResearchError {
    fn from(value: anyhow::Error) -> Self {
        Self::Upstream(value)
    }
}

pub async fn profile(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let symbol = symbol_from_path(path)?;
    let response = client
        .get("https://push2.eastmoney.com/api/qt/stock/get")
        .query(&[
            ("fltt", "2"),
            ("invt", "2"),
            ("secid", symbol.eastmoney.as_str()),
            ("fields", "f57,f58,f84,f85,f116,f117,f127,f189"),
        ])
        .send()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?
        .error_for_status()
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let root: Value = response
        .json()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| ResearchError::Upstream(anyhow!("东财公司资料无数据")))?;

    let name = value_text(data.get("f58")).unwrap_or_default();
    if name.is_empty() {
        return Err(ResearchError::Upstream(anyhow!("东财公司资料缺少名称")));
    }

    Ok(json!({
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "name": name,
        "industry": value_text(data.get("f127")),
        "ipo_date": data.get("f189").and_then(ipo_date),
        "total_shares": value_number(data.get("f84")),
        "float_shares": value_number(data.get("f85")),
        "total_market_cap": value_number(data.get("f116")),
        "float_market_cap": value_number(data.get("f117")),
        "source": "eastmoney",
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

pub async fn announcements(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let symbol = symbol_from_path(path)?;
    let limit = query_param(path, "limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(5, 50);
    let limit_string = limit.to_string();

    let response = client
        .get("https://np-anotice-stock.eastmoney.com/api/security/ann")
        .query(&[
            ("sr", "-1"),
            ("page_size", limit_string.as_str()),
            ("page_index", "1"),
            ("ann_type", "A"),
            ("client_source", "web"),
            ("stock_list", symbol.digits.as_str()),
            ("f_node", "0"),
            ("s_node", "0"),
        ])
        .send()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?
        .error_for_status()
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let root: Value = response
        .json()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let list = root
        .pointer("/data/list")
        .and_then(Value::as_array)
        .ok_or_else(|| ResearchError::Upstream(anyhow!("公告列表为空")))?;

    let mut items = Vec::new();
    for item in list.iter().take(limit) {
        let title = item.get("title").and_then(Value::as_str).unwrap_or("").trim();
        let art_code = item
            .get("art_code")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let date = item
            .get("notice_date")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();
        if title.is_empty() || art_code.is_empty() || date.len() != 10 {
            continue;
        }
        items.push(json!({
            "title": title,
            "date": date,
            "art_code": art_code,
            "pdf_url": format!("https://pdf.dfcfw.com/pdf/H2_{art_code}_1.pdf"),
        }));
    }
    let count = items.len();

    Ok(json!({
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "items": items,
        "count": count,
        "source": "eastmoney",
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

pub async fn financials(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let symbol = symbol_from_path(path)?;
    let periods = query_param(path, "periods")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8)
        .clamp(4, 20);
    let periods_string = periods.to_string();

    // Same zero-key HTTP fallback as the desktop script, without embedding
    // Python/akshare into Android.
    let response = client
        .get("https://quotes.sina.cn/cn/api/openapi.php/CompanyFinanceService.getFinanceReport2022")
        .query(&[
            ("paperCode", symbol.tencent.as_str()),
            ("source", "lrb"),
            ("type", "0"),
            ("page", "1"),
            ("num", periods_string.as_str()),
        ])
        .send()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?
        .error_for_status()
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let root: Value = response
        .json()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let report_list = root
        .pointer("/result/data/report_list")
        .and_then(Value::as_object)
        .ok_or_else(|| ResearchError::Upstream(anyhow!("新浪财务报表无数据")))?;

    let mut keys = report_list.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|a, b| b.cmp(a));
    keys.truncate(periods);
    let mut rows = Vec::new();
    for key in keys {
        let Some(items) = report_list
            .get(&key)
            .and_then(|value| value.get("data"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let mut revenue = None;
        let mut net_profit_parent = None;
        for item in items {
            let title = item.get("item_title").and_then(Value::as_str).unwrap_or("");
            let value = value_number(item.get("item_value"));
            match title {
                "营业总收入" => revenue = value,
                "归属于母公司所有者的净利润" => net_profit_parent = value,
                _ => {}
            }
        }
        if revenue.is_none() && net_profit_parent.is_none() {
            continue;
        }
        rows.push(json!({
            "period": format_period(&key),
            "revenue_cum": revenue,
            "net_profit_parent_cum": net_profit_parent,
            "net_profit_deducted_cum": Value::Null,
        }));
    }
    if rows.is_empty() {
        return Err(ResearchError::Upstream(anyhow!("新浪财务报表未解析出营收或归母净利润")));
    }
    let count = rows.len();

    Ok(json!({
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "rows": rows,
        "count": count,
        "status": "partial",
        "missing": ["net_profit_deducted_cum"],
        "note": "移动端使用新浪利润表直连，无 Python/akshare 依赖；该源不提供扣非净利润。",
        "source": "sina_report",
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

pub async fn estimates(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let symbol = symbol_from_path(path)?;
    let limit = query_param(path, "limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(5, 40);
    let limit_string = limit.to_string();
    let year = Utc::now().year();
    let begin = format!("{}-01-01", year - 1);
    let end = format!("{}-12-31", year + 1);

    let response = client
        .get("https://reportapi.eastmoney.com/report/list")
        .query(&[
            ("industryCode", "*"),
            ("pageSize", limit_string.as_str()),
            ("industry", "*"),
            ("rating", "*"),
            ("ratingChange", "*"),
            ("beginTime", begin.as_str()),
            ("endTime", end.as_str()),
            ("pageNo", "1"),
            ("fields", ""),
            ("qType", "0"),
            ("orgCode", ""),
            ("code", symbol.digits.as_str()),
            ("rcode", ""),
        ])
        .send()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?
        .error_for_status()
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let root: Value = response
        .json()
        .await
        .map_err(|error| ResearchError::Upstream(error.into()))?;
    let data = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ResearchError::Upstream(anyhow!("东财研报预测列表为空")))?;

    let mut reports = Vec::new();
    for item in data.iter().take(limit) {
        let published = item
            .get("publishDate")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();
        let base_year = published
            .get(0..4)
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(year);
        let mut forecasts = Map::new();
        if let Some(value) = value_number(item.get("predictThisYearEps")) {
            forecasts.insert(format!("FY{base_year}"), json!(value));
        }
        if let Some(value) = value_number(item.get("predictNextYearEps")) {
            forecasts.insert(format!("FY{}", base_year + 1), json!(value));
        }
        if let Some(value) = value_number(item.get("predictNextTwoYearEps")) {
            forecasts.insert(format!("FY{}", base_year + 2), json!(value));
        }
        if forecasts.is_empty() {
            continue;
        }
        reports.push(json!({
            "info_code": item.get("infoCode").cloned().unwrap_or(Value::Null),
            "published_at": published,
            "organization": item.get("orgSName").cloned().unwrap_or(Value::Null),
            "title": item.get("title").cloned().unwrap_or(Value::Null),
            "rating": item.get("emRatingName").cloned().unwrap_or(Value::Null),
            "eps": Value::Object(forecasts),
        }));
    }
    if reports.is_empty() {
        return Err(ResearchError::Upstream(anyhow!("东财研报中没有可用 EPS 预测")));
    }
    let count = reports.len();

    Ok(json!({
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "reports": reports,
        "count": count,
        "status": "partial",
        "note": "这是逐篇机构研报预测，不等同于一致预期；移动端暂不把它聚合成共识值。",
        "source": "eastmoney_reportapi",
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

fn symbol_from_path(path: &str) -> std::result::Result<SymbolKey, ResearchError> {
    let raw = query_param(path, "symbol").unwrap_or_default();
    if raw.trim().is_empty() {
        return Err(ResearchError::BadRequest("symbol 不能为空".to_string()));
    }
    normalize_stock_symbol(&raw).map_err(|error| ResearchError::BadRequest(error.to_string()))
}

fn query_param(path: &str, key: &str) -> Option<String> {
    path.split('?').nth(1).and_then(|query| {
        query.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(found), Some(value)) if found == key => Some(value.to_string()),
                _ => None,
            }
        })
    })
}

fn value_text(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) => {
            let value = value.trim();
            (!value.is_empty() && value != "-").then(|| value.to_string())
        }
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn value_number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(value)) => value.as_f64(),
        Some(Value::String(value)) => value.replace(',', "").trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn ipo_date(value: &Value) -> Option<String> {
    let raw = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => return None,
    };
    let digits = raw.trim().trim_end_matches(".0");
    if digits.len() == 8 && digits.chars().all(|ch| ch.is_ascii_digit()) {
        Some(format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8]))
    } else {
        None
    }
}

fn format_period(value: &str) -> String {
    let digits = value.trim();
    if digits.len() == 8 && digits.chars().all(|ch| ch.is_ascii_digit()) {
        format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8])
    } else {
        digits.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symbol_query() {
        let symbol = symbol_from_path("/mobile/research/profile?symbol=600519.SH")
            .expect("symbol");
        assert_eq!(symbol.digits, "600519");
        assert_eq!(symbol.market.as_str(), "SH");
    }

    #[test]
    fn formats_yyyymmdd() {
        assert_eq!(format_period("20260630"), "2026-06-30");
        assert_eq!(ipo_date(&json!(19990827)).as_deref(), Some("1999-08-27"));
    }
}
