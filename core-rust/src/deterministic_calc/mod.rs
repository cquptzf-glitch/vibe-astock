//! Mobile implementation of calc/SPEC.md 0.5.0. Pure, bounded JSON calculations.
mod earnings;
mod formulas;
pub(crate) mod graph;
mod indicators;
mod series;
use serde_json::{json, Value};
pub const VERSION: &str = "0.5.0";
type R<T> = Result<T, String>;
fn num(v: &Value, name: &str) -> R<f64> {
    let n = v
        .as_f64()
        .or_else(|| v.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite());
    n.ok_or_else(|| format!("{name} 必须是有限数，不能缺失或为布尔值"))
}
fn n(a: &Value, k: &str) -> R<f64> {
    num(&a[k], k)
}
fn opt(a: &Value, k: &str) -> R<Option<f64>> {
    if a[k].is_null() {
        Ok(None)
    } else {
        n(a, k).map(Some)
    }
}
fn text<'a>(a: &'a Value, k: &str, default: &'a str) -> R<&'a str> {
    match a.get(k) {
        None => Ok(default),
        Some(v) => v.as_str().ok_or_else(|| format!("{k} 必须是字符串")),
    }
}
fn integer(a: &Value, k: &str, default: usize, lo: usize, hi: usize) -> R<usize> {
    let v = match a.get(k) {
        None => default,
        Some(v) => v
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| format!("{k} 必须是整数"))?,
    };
    if !(lo..=hi).contains(&v) {
        Err(format!("{k} 必须在 {lo}..{hi}"))
    } else {
        Ok(v)
    }
}
fn boolean(a: &Value, k: &str, default: bool) -> R<bool> {
    match a.get(k) {
        None => Ok(default),
        Some(v) => v.as_bool().ok_or_else(|| format!("{k} 必须为布尔值")),
    }
}
// Every computed float passes this adapter before JSON construction; serde_json's
// default NaN -> null conversion must never turn an overflow into a valid output.
fn v(n: f64) -> Value {
    serde_json::Number::from_f64(n)
        .map(Value::Number)
        .unwrap_or_else(|| json!({"$nonfinite":true}))
}
fn floats(a: &[f64]) -> Value {
    Value::Array(a.iter().copied().map(v).collect())
}
fn nonfinite(j: &Value) -> bool {
    match j {
        Value::Object(m) => {
            m.get("$nonfinite") == Some(&Value::Bool(true)) || m.values().any(nonfinite)
        }
        Value::Array(a) => a.iter().any(nonfinite),
        _ => false,
    }
}
fn out(
    status: &str,
    value: Option<f64>,
    unit: &str,
    reason: impl Into<String>,
    details: Value,
) -> Value {
    if value.is_some_and(|x| !x.is_finite()) || nonfinite(&details) {
        return json!({"status":"error","value":null,"unit":unit,"reason":"结果或中间量出现非有限数(溢出 / NaN)，输入量级或内容不合理","details":{}});
    }
    json!({"status":status,"value":value,"unit":unit,"reason":reason.into(),"details":details})
}
fn ok(value: f64, unit: &str, details: Value) -> Value {
    out("ok", Some(value), unit, "", details)
}
fn nm(unit: &str, reason: impl Into<String>, details: Value) -> Value {
    out("not_meaningful", None, unit, reason, details)
}
fn error(reason: impl Into<String>) -> Value {
    json!({"status":"error","value":null,"unit":"","reason":reason.into(),"details":{}})
}
pub(crate) fn money_unit(unit: &str) -> R<(&'static str, f64, &'static str)> {
    match unit {
        "元" => Ok(("CNY", 1., "元")),
        "万元" => Ok(("CNY", 1e4, "元")),
        "亿元" => Ok(("CNY", 1e8, "元")),
        "美元" => Ok(("USD", 1., "美元")),
        "万美元" => Ok(("USD", 1e4, "美元")),
        "亿美元" => Ok(("USD", 1e8, "美元")),
        "港元" => Ok(("HKD", 1., "港元")),
        "万港元" => Ok(("HKD", 1e4, "港元")),
        "亿港元" => Ok(("HKD", 1e8, "港元")),
        _ => Err("金额单位不支持；必须明确人民币/美元/港元及元/万/亿量级".into()),
    }
}
fn finite(n: f64) -> R<f64> {
    if n.is_finite() {
        Ok(n)
    } else {
        Err("计算结果溢出".into())
    }
}
fn number_display(n: f64) -> String {
    let decimals = if n == 0. || n.abs() >= 1. {
        2
    } else {
        (3. - n.abs().log10().floor()).max(2.) as usize
    };
    let value = if n == 0. { 0. } else { n };
    format!("{value:.decimals$}")
}
fn display(r: &Value) -> Option<String> {
    if r["status"] != "ok" {
        return None;
    }
    let mut value = r["value"].as_f64()?;
    let unit = r["unit"].as_str().unwrap_or("");
    if unit == "小数" || unit == "%" {
        if unit == "小数" {
            value *= 100.;
        }
        return value
            .is_finite()
            .then(|| format!("{}%", number_display(value)));
    }
    if let Ok((_, scale, base)) = money_unit(unit) {
        value *= scale;
        if !value.is_finite() {
            return None;
        }
        let (value, prefix) = if value.abs() >= 1e8 {
            (value / 1e8, "亿")
        } else if value.abs() >= 1e4 {
            (value / 1e4, "万")
        } else {
            (value, "")
        };
        return Some(format!("{} {prefix}{base}", number_display(value)));
    }
    if unit == "期" && value.fract() == 0. {
        return Some(format!("{value:.0} 期"));
    }
    Some(format!(
        "{}{}{}",
        number_display(value),
        if unit.is_empty() { "" } else { " " },
        unit
    ))
}
fn attach(r: &mut Value, depth: usize) {
    if depth > 6 {
        return;
    }
    match r {
        Value::Array(a) => {
            for v in a {
                attach(v, depth + 1)
            }
        }
        Value::Object(m) => {
            for v in m.values_mut() {
                attach(v, depth + 1)
            }
        }
        _ => {}
    }
    if r.get("status").is_some() && r.get("value").is_some() && r.get("unit").is_some() {
        r["display"] = json!(display(r));
    }
}
/// Unknown names/arguments are rejected; this registry never executes code or files.
pub fn execute(function: &str, args: &Value) -> Value {
    let result = (|| -> R<Value> {
        let descriptor =
            descriptor(function).ok_or_else(|| "计算函数不在受控目录内".to_string())?;
        let params = args
            .as_object()
            .ok_or_else(|| "args 必须是对象".to_string())?;
        if params
            .keys()
            .any(|k| !descriptor.params.contains(&k.as_str()))
        {
            return Err("含目录外参数；不能指定文件、网络或 action".into());
        }
        if serde_json::to_vec(args).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("计算输入超过 256 KiB".into());
        }
        match descriptor.group {
            "formulas" => formulas::run(function, args),
            "series" => series::run(function, args),
            "indicators" => indicators::run(function, args),
            "earnings" => earnings::run(function, args),
            _ => unreachable!(),
        }
    })();
    let mut result = result.unwrap_or_else(error);
    attach(&mut result, 0);
    result
}
pub(crate) struct Descriptor {
    pub name: &'static str,
    pub group: &'static str,
    pub params: &'static [&'static str],
    pub config: &'static [&'static str],
    pub description: &'static str,
}
macro_rules! d {($name:literal,$group:literal,[$($p:literal),*],[$($c:literal),*],$desc:literal)=>{Descriptor{name:$name,group:$group,params:&[$($p),*],config:&[$($c),*],description:$desc}};}
const FUNCTIONS: &[Descriptor] = &[
    d!(
        "pe_deducted_annualized",
        "formulas",
        [
            "total_market_cap",
            "cap_unit",
            "latest_quarter_deducted_profit",
            "profit_unit"
        ],
        ["cap_unit", "profit_unit"],
        "总市值 ÷ 最新单季扣非净利润 × 四的分母；金额必须带单位"
    ),
    d!(
        "pe_ttm_from_parts",
        "formulas",
        [
            "total_market_cap",
            "cap_unit",
            "ttm_profit",
            "profit_unit",
            "fx_rate",
            "fx_unit"
        ],
        ["cap_unit", "profit_unit", "fx_unit"],
        "总市值 ÷ TTM 净利润，跨币种必须有汇率证据"
    ),
    d!(
        "forward_pe",
        "formulas",
        [
            "price",
            "eps_forecast",
            "price_unit",
            "eps_unit",
            "fx_rate",
            "fx_unit"
        ],
        ["price_unit", "eps_unit", "fx_unit"],
        "现价 ÷ 一致预期 EPS，币种不同时换算 EPS"
    ),
    d!(
        "fx_convert",
        "formulas",
        ["value", "unit", "fx_rate", "fx_unit"],
        ["unit", "fx_unit"],
        "金额按 报价币种/基准币种 汇率换算"
    ),
    d!(
        "forward_cagr",
        "formulas",
        ["eps_t", "eps_t_plus_n", "years"],
        ["years"],
        "正 EPS 起止点 CAGR；years 为一至十年整数"
    ),
    d!(
        "growth_rate",
        "formulas",
        ["current", "base", "label"],
        ["label"],
        "同单位增速，非正基期无意义，输出小数"
    ),
    d!(
        "ratio",
        "formulas",
        ["numerator", "denominator", "label", "unit_in"],
        ["label", "unit_in"],
        "同单位两数比率；非正分母无意义"
    ),
    d!(
        "peg",
        "formulas",
        ["pe", "cagr"],
        [],
        "PE ÷ CAGR 百分数；输入增速为小数"
    ),
    d!(
        "pe_digestion_years",
        "formulas",
        ["pe", "cagr", "anchor"],
        ["anchor"],
        "固定估值锚的消化年数，低于锚为零年"
    ),
    d!(
        "pe_digestion_scenarios",
        "formulas",
        ["pe", "cagr"],
        [],
        "四种固定估值锚的消化年数"
    ),
    d!(
        "percentile_rank",
        "formulas",
        ["history", "current", "exclude_nonpositive"],
        ["exclude_nonpositive"],
        "有效历史至少二十项，默认剔除非正值，输出百分数"
    ),
    d!(
        "consensus_dispersion",
        "formulas",
        ["low", "mean", "high"],
        [],
        "一致预期分歧度 max/min 及 range/mean"
    ),
    d!(
        "forward_vs_ttm_judgement",
        "formulas",
        ["forward_cagr_value", "ttm_yoy_value", "tolerance_pp"],
        ["tolerance_pp"],
        "前瞻与 TTM 同比差，阈值为百分点"
    ),
    d!(
        "quarterize",
        "series",
        ["cumulative", "unit", "money"],
        ["unit", "money"],
        "累计 YTD 拆单季，缺季不补零；输出 details.series"
    ),
    d!(
        "latest_quarter",
        "series",
        ["single_quarters", "unit", "money"],
        ["unit", "money"],
        "最新一季缺失时不得拿更早季度顶替"
    ),
    d!(
        "ttm_sum",
        "series",
        ["single_quarters", "end_period", "unit", "money"],
        ["end_period", "unit", "money"],
        "连续四季之和，缺季无意义"
    ),
    d!(
        "ttm_yoy",
        "series",
        ["single_quarters", "end_period", "unit", "money"],
        ["end_period", "unit", "money"],
        "连续八季 TTM 同比，非正基期无意义"
    ),
    d!(
        "qoq",
        "series",
        ["single_quarters", "end_period", "unit", "money"],
        ["end_period", "unit", "money"],
        "单季环比仅描述动量，不作增速分母"
    ),
    d!(
        "ttm_from_ytd",
        "series",
        ["cumulative", "end_period", "unit", "money"],
        ["end_period", "unit", "money"],
        "任意财年按 起始日..截止日 的 YTD 桥接 TTM"
    ),
    d!(
        "ttm_yoy_from_ytd",
        "series",
        ["cumulative", "end_period", "unit", "money"],
        ["end_period", "unit", "money"],
        "任意财年累计值桥接 TTM 同比"
    ),
    d!(
        "technical_indicators",
        "indicators",
        [
            "klines",
            "ma",
            "ema",
            "macd",
            "rsi",
            "kdj",
            "boll",
            "min_points"
        ],
        ["ma", "ema", "macd", "rsi", "kdj", "boll", "min_points"],
        "MA/EMA/MACD/简单平均 RSI/KDJ/BOLL，仅返回已发生数值"
    ),
    d!(
        "chip_distribution",
        "indicators",
        ["klines", "grid_size", "decay", "min_points"],
        ["grid_size", "decay", "min_points"],
        "前复权、非停牌日换手衰减三角筹码分布"
    ),
    d!(
        "earnings_reaction",
        "earnings",
        ["klines", "events", "lookback"],
        ["lookback"],
        "公告反应日涨跌及此前事件均值；同反应日合并"
    ),
    d!(
        "earnings_surprise",
        "earnings",
        ["actual", "estimate", "low", "high", "unit"],
        ["unit"],
        "实际与预期/预告区间比较，不同单位需先换算"
    ),
];
pub(crate) fn descriptor(name: &str) -> Option<&'static Descriptor> {
    FUNCTIONS.iter().find(|d| d.name == name)
}
pub fn catalog() -> Value {
    json!({"calc_version":VERSION,"functions":FUNCTIONS.iter().map(|d|{
    let required=match d.name{"pe_deducted_annualized"|"pe_ttm_from_parts"|"fx_convert"=>4,"pe_digestion_years"|"consensus_dispersion"=>3,"quarterize"|"latest_quarter"|"technical_indicators"|"chip_distribution"|"earnings_surprise"=>1,_=>2};
    json!({"name":d.name,"group":d.group,"parameters":d.params,"required_parameters":&d.params[..required],"literal_parameters":d.config,"description":d.description})
}).collect::<Vec<_>>(),"output_contract":"status=ok/not_meaningful/error，value、unit、reason、details、display；报告照抄 display。金融金额序列传 money=true；没有任意文件或网络权限。"})
}

#[cfg(test)]
mod tests;
