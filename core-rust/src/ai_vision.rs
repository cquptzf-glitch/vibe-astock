//! 截图识别自选 / 持仓:把用户**主动选的**截图交给当前配置的多模态模型,识别出「股票代码、名称、(持仓)数量、成本」,只生成核对草稿。
//!
//! 🔴 只有用户在模型配置里标了「支持图片识别」才会发图(`probe` 检测除外);图片不落盘、不入库,识别完就丢。
//! 🔴 模型只是读图:代码和名称再用腾讯股票搜索各核对一遍(代码对应的名称和截图里的名称对不上就标出来);
//!    读不到代码的,只给名称搜索出的候选,**不替用户选**。数量、成本读不到就是空,不补 0。
//! 🔴 草稿不会自动保存:页面让用户逐条对照截图核对、勾选、确认后才写入自选 / 交易日志。
//! 🔴 只给识别结果,不评价股票、不给任何买卖建议。

use crate::{ai_config, search, symbol::normalize_stock_symbol};
use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

pub const MAX_IMAGES: usize = 4;
const MAX_IMAGE_B64_CHARS: usize = 6_000_000; // ≈ 4.5 MB 原图
const MAX_TOTAL_B64_CHARS: usize = 14_000_000;
const MAX_DRAFTS: usize = 60;
const MIMES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

const SYSTEM_PROMPT: &str = "你是证券 App 截图的转写员。只把截图里**看得见**的内容转写成 JSON,不要补充、猜测或评价。\
规则:1) 每只股票一条,字段 code(6 位数字代码,看不清写 null,不要凭名称猜代码)、name(截图里的名称原样)、\
shares(持有数量,股,纯数字;看不到写 null)、cost(成本价或持仓成本,元,纯数字;看不到写 null)、note(看不清或有歧义的地方,没有就写空字符串)。\
2) 数字必须逐位照抄,千分位逗号去掉;不确定的位写进 note,不要自己修正。3) 港股、美股、基金、可转债照样转写,code 原样写。\
4) 只输出一个 JSON 对象:{\"stocks\":[…],\"warnings\":[…]},不要代码块标记,不要任何别的文字。";

#[derive(Debug)]
pub enum VisionError {
    BadRequest(String),
    Upstream(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for VisionError {
    fn from(value: anyhow::Error) -> Self {
        Self::Internal(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Image {
    pub name: String,
    pub mime: String,
    pub data: String,
}

fn bad(message: impl Into<String>) -> VisionError {
    VisionError::BadRequest(message.into())
}

pub(crate) fn parse_images(body: &Value) -> Result<Vec<Image>, String> {
    let list = body.get("images").and_then(Value::as_array).ok_or("缺少 images")?;
    if list.is_empty() || list.len() > MAX_IMAGES {
        return Err(format!("一次选 1–{MAX_IMAGES} 张截图"));
    }
    let mut out = Vec::new();
    let mut total = 0usize;
    for (i, item) in list.iter().enumerate() {
        let mime = item.get("mime").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
        if !MIMES.contains(&mime.as_str()) {
            return Err(format!("第 {} 张不是支持的图片格式(png / jpeg / webp / gif)", i + 1));
        }
        let data = item.get("data").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if data.is_empty() || !data.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')) {
            return Err(format!("第 {} 张图片数据不是有效的 base64", i + 1));
        }
        if data.len() > MAX_IMAGE_B64_CHARS {
            return Err(format!("第 {} 张太大(单张最多约 4.5 MB),请截小一点", i + 1));
        }
        total += data.len();
        if total > MAX_TOTAL_B64_CHARS {
            return Err("这批截图合计太大,请少选几张".into());
        }
        let name: String = item.get("name").and_then(Value::as_str).unwrap_or("").chars().take(80).collect();
        out.push(Image { name: if name.trim().is_empty() { format!("截图 {}", i + 1) } else { name }, mime, data });
    }
    Ok(out)
}

pub(crate) fn build_messages(images: &[Image], target: &str) -> Value {
    let ask = match target {
        "position" => "这是持仓页面的截图。请转写每只股票的代码、名称、持有数量和成本价(没有就写 null)。",
        _ => "这是自选股列表或持仓的截图。请转写每只股票的代码和名称(数量、成本看得到就写,看不到写 null)。",
    };
    let mut parts = vec![json!({"type": "text", "text": ask})];
    for image in images {
        parts.push(json!({"type": "image_url", "image_url": {"url": format!("data:{};base64,{}", image.mime, image.data)}}));
    }
    json!([{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": parts}])
}

/// 模型回答里取出 JSON 对象:整段就是 JSON、被 ``` 包着、或前后有废话都认
pub(crate) fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let start = t.find('{')?;
    let end = t.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&t[start..=end]).ok()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Raw {
    pub code: Option<String>,
    pub name: String,
    pub shares: Option<f64>,
    pub cost: Option<f64>,
    pub note: String,
}

fn number_of(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.replace([',', '，', ' '], "").parse::<f64>().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

/// 从模型写的代码里取出 6 位数字(容忍 600519.SH、SH600519 这类写法);没有连续 6 位数字就是读不到
pub(crate) fn code_of(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut run = 0;
    for (i, b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            run += 1;
            if run == 6 && !bytes.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
                return Some(raw[i - 5..=i].to_string());
            }
        } else {
            run = 0;
        }
    }
    None
}

pub(crate) fn parse_answer(text: &str) -> Result<(Vec<Raw>, Vec<String>), String> {
    let root = extract_json(text).ok_or("模型的回答不是 JSON,没法转成草稿(换一张更清晰的截图,或换一个模型再试)")?;
    let stocks = root.get("stocks").and_then(Value::as_array).ok_or("模型的回答里没有 stocks 列表")?;
    let warnings = root.get("warnings").and_then(Value::as_array).map(|w| w.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let rows = stocks
        .iter()
        .filter_map(|s| {
            let name = s.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let code = s.get("code").and_then(|c| match c {
                Value::String(t) => code_of(t),
                Value::Number(n) => code_of(&format!("{n:0>6}")),
                _ => None,
            });
            if code.is_none() && name.is_empty() {
                return None;
            }
            Some(Raw { code, name, shares: number_of(s.get("shares")), cost: number_of(s.get("cost")), note: s.get("note").and_then(Value::as_str).unwrap_or("").trim().to_string() })
        })
        .collect();
    Ok((rows, warnings))
}

fn plain(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace() && !matches!(c, '*' | '-')).collect::<String>().to_uppercase()
}

/// 截图名称和代码对应名称是不是一回事:去掉空白、星号后,一方包含另一方就算(ST、退市整理等前缀不影响)
pub(crate) fn names_agree(shot: &str, official: &str) -> bool {
    let (a, b) = (plain(shot), plain(official));
    a.is_empty() || b.is_empty() || a.contains(&b) || b.contains(&a) || a.trim_start_matches("ST") == b.trim_start_matches("ST")
}

/// 一条草稿(核对前):代码合法性 + 数字合理性;搜索核对在 recognize 里补
pub(crate) fn draft_of(raw: &Raw, target: &str) -> Value {
    let mut uncertain: Vec<String> = Vec::new();
    let code = raw.code.as_deref().and_then(|c| normalize_stock_symbol(c).ok().map(|k| k.digits));
    if raw.code.is_some() && code.is_none() {
        uncertain.push(format!("代码 {} 不是有效的 A 股代码", raw.code.as_deref().unwrap_or("")));
    }
    if raw.code.is_none() {
        uncertain.push("没有读到代码,只有名称".into());
    }
    let shares = raw.shares.filter(|v| *v > 0.0);
    let cost = raw.cost.filter(|v| *v > 0.0);
    if target == "position" {
        if shares.is_none() {
            uncertain.push("没有读到持有数量".into());
        }
        if cost.is_none() {
            uncertain.push("没有读到成本价".into());
        }
    }
    if raw.shares.is_some() && shares.is_none() {
        uncertain.push("数量不是正数,已丢弃".into());
    }
    if raw.cost.is_some() && cost.is_none() {
        uncertain.push("成本不是正数,已丢弃".into());
    }
    if !raw.note.is_empty() {
        uncertain.push(format!("模型备注:{}", raw.note));
    }
    json!({"code": code, "name": raw.name, "shares": shares, "cost": cost, "uncertain": uncertain, "candidates": [], "official_name": Value::Null})
}

fn dedupe(drafts: Vec<Value>, warnings: &mut Vec<String>) -> Vec<Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for d in drafts {
        match d["code"].as_str() {
            Some(code) if seen.iter().any(|s| s == code) => warnings.push(format!("{code} 在截图里出现了不止一次,只留第一条")),
            Some(code) => {
                seen.push(code.to_string());
                out.push(d);
            }
            None => out.push(d),
        }
    }
    out
}

/// 用腾讯股票搜索核对:有代码的核对名称,没代码的按名称给候选
async fn cross_check(client: &Client, mut drafts: Vec<Value>) -> (Vec<Value>, Option<String>) {
    let results: Vec<(usize, Result<Vec<Value>, String>)> = stream::iter(drafts.iter().enumerate().map(|(i, d)| (i, d["code"].as_str().map(String::from), d["name"].as_str().unwrap_or("").to_string())))
        .map(|(i, code, name)| {
            let client = client.clone();
            async move {
                let query = code.clone().unwrap_or(name);
                if query.is_empty() {
                    return (i, Ok(Vec::new()));
                }
                let r = search::symbols(&client, &json!({"query": query, "limit": 5})).await;
                (i, r.map(|v| v["items"].as_array().cloned().unwrap_or_default()).map_err(|e| format!("{e:?}")))
            }
        })
        .buffer_unordered(4)
        .collect()
        .await;
    let mut failed = 0usize;
    for (i, result) in results {
        match result {
            Err(_) => failed += 1,
            Ok(items) => apply_search(&mut drafts[i], &items),
        }
    }
    (drafts, (failed > 0).then(|| format!("{failed} 条的名称没核对成(股票搜索没回应),请自己对照截图确认代码")))
}

pub(crate) fn apply_search(draft: &mut Value, items: &[Value]) {
    let shot_name = draft["name"].as_str().unwrap_or("").to_string();
    let mut add: Vec<String> = Vec::new();
    match draft["code"].as_str().map(String::from) {
        Some(code) => match items.iter().find(|x| x["symbol"].as_str() == Some(code.as_str())) {
            Some(hit) => {
                let official = hit["name"].as_str().unwrap_or("").to_string();
                if !names_agree(&shot_name, &official) {
                    add.push(format!("截图里的名称「{shot_name}」和代码 {code} 对应的「{official}」对不上,请核对是不是读错了代码"));
                }
                draft["official_name"] = json!(official);
            }
            None => add.push(format!("股票搜索里查不到代码 {code}(可能读错了一位,或不是沪深北 A 股)")),
        },
        None => {
            let cands: Vec<Value> = items.iter().take(3).map(|x| json!({"symbol": x["symbol"], "name": x["name"]})).collect();
            if cands.is_empty() {
                add.push(format!("按名称「{shot_name}」没搜到股票"));
            }
            draft["candidates"] = json!(cands);
        }
    }
    if let Some(list) = draft["uncertain"].as_array_mut() {
        list.extend(add.into_iter().map(Value::String));
    }
}

pub async fn recognize(client: &Client, db: &Path, body: &Value) -> Result<Value, VisionError> {
    let target = body.get("target").and_then(Value::as_str).unwrap_or("watch");
    if !matches!(target, "watch" | "position") {
        return Err(bad("target 只能是 watch 或 position"));
    }
    let probe = body.get("probe").and_then(Value::as_bool).unwrap_or(false);
    let config = ai_config::load(db).map_err(VisionError::Internal)?.ok_or_else(|| bad("还没有配置模型 API(到「AI」页配置)"))?;
    if config.base_url.is_empty() || config.model.is_empty() {
        return Err(bad("模型 API 配置不完整"));
    }
    if !config.vision && !probe {
        return Err(bad("当前模型没有标记为「支持图片识别」。到「AI」页的模型配置里打开这个开关(先点「测试图片识别」确认模型真的能读图)"));
    }
    let images = parse_images(body).map_err(bad)?;
    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut request = client
        .post(&endpoint)
        .timeout(Duration::from_secs(120))
        .header("Accept", "application/json")
        .json(&json!({"model": &config.model, "messages": build_messages(&images, target), "temperature": 0}));
    let key = ai_config::injected_api_key(body);
    if !key.is_empty() {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|e| VisionError::Upstream(format!("连接模型 API 失败: {e}")))?;
    let status = response.status();
    let text = response.text().await.map_err(|e| VisionError::Upstream(format!("读取模型响应失败: {e}")))?;
    if !status.is_success() {
        let short: String = text.chars().take(500).collect();
        let hint = if matches!(status.as_u16(), 400 | 404 | 415 | 422) { "(如果提示不支持图片 / image_url,说明这个模型不是多模态模型,换一个再试)" } else { "" };
        return Err(VisionError::Upstream(format!("模型 API 返回 HTTP {}{hint}: {short}", status.as_u16())));
    }
    let root: Value = serde_json::from_str(&text).map_err(|e| VisionError::Upstream(format!("模型响应不是有效 JSON: {e}")))?;
    let answer = root["choices"][0]["message"]["content"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| VisionError::Upstream("模型响应缺少 choices[0].message.content".into()))?;
    let (raws, mut warnings) = parse_answer(answer).map_err(VisionError::Upstream)?;
    if raws.len() > MAX_DRAFTS {
        warnings.push(format!("截图里识别出 {} 条,只取前 {MAX_DRAFTS} 条", raws.len()));
    }
    let drafts: Vec<Value> = raws.iter().take(MAX_DRAFTS).map(|r| draft_of(r, target)).collect();
    let drafts = dedupe(drafts, &mut warnings);
    let (drafts, check_note) = if probe { (drafts, None) } else { cross_check(client, drafts).await };
    if let Some(note) = check_note {
        warnings.push(note);
    }
    Ok(json!({
        "status": "ok", "target": target, "model": config.model, "images": images.iter().map(|i| i.name.clone()).collect::<Vec<_>>(),
        "drafts": drafts, "warnings": warnings, "probe": probe,
        "note": "这是模型读图的结果,可能读错数字或名称。请逐条对照截图核对,勾选确认后才会写入;读不到的项是空的,不会当 0。",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn image(n: usize) -> Value {
        json!({"name": format!("s{n}.png"), "mime": "image/png", "data": "iVBORw0KGgo="})
    }

    #[test]
    fn images_are_validated_before_anything_is_sent() {
        assert!(parse_images(&json!({})).is_err());
        assert!(parse_images(&json!({"images": []})).is_err());
        assert!(parse_images(&json!({"images": (0..5).map(image).collect::<Vec<_>>()})).unwrap_err().contains("1–4"));
        let mut svg = image(0);
        svg["mime"] = json!("image/svg+xml");
        assert!(parse_images(&json!({"images": [svg]})).unwrap_err().contains("不是支持的图片格式"));
        let mut junk = image(0);
        junk["data"] = json!("not base64!!");
        assert!(parse_images(&json!({"images": [junk]})).unwrap_err().contains("base64"));
        let mut huge = image(0);
        huge["data"] = json!("A".repeat(MAX_IMAGE_B64_CHARS + 1));
        assert!(parse_images(&json!({"images": [huge]})).unwrap_err().contains("太大"));
        let ok = parse_images(&json!({"images": [image(1), json!({"mime": "IMAGE/JPEG", "data": "AAAA"})]})).unwrap();
        assert_eq!((ok.len(), ok[1].mime.as_str(), ok[1].name.as_str()), (2, "image/jpeg", "截图 2"));
    }

    #[test]
    fn request_uses_openai_multimodal_parts_and_a_transcription_only_prompt() {
        let images = parse_images(&json!({"images": [image(1)]})).unwrap();
        let m = build_messages(&images, "position");
        assert_eq!(m[0]["role"], "system");
        let prompt = m[0]["content"].as_str().unwrap();
        assert!(prompt.contains("不要补充、猜测或评价") && prompt.contains("看不清写 null"));
        assert_eq!(m[1]["content"][0]["type"], "text");
        assert!(m[1]["content"][0]["text"].as_str().unwrap().contains("持有数量和成本"));
        assert_eq!(m[1]["content"][1]["type"], "image_url");
        assert_eq!(m[1]["content"][1]["image_url"]["url"], "data:image/png;base64,iVBORw0KGgo=");
    }

    #[test]
    fn answers_parse_from_plain_fenced_or_chatty_json_and_numbers_come_out_clean() {
        let plain = r#"{"stocks":[{"code":"600519","name":"贵州茅台","shares":"1,200","cost":1450.5,"note":""},{"code":"SZ000001","name":"平安银行","shares":null,"cost":null,"note":"成本被遮住"},{"code":null,"name":"中际旭创","shares":100,"cost":"abc"},{"code":null,"name":""}],"warnings":["第二张图有点糊"]}"#;
        for text in [plain.to_string(), format!("```json\n{plain}\n```"), format!("好的,结果如下:\n{plain}\n以上。")] {
            let (rows, warnings) = parse_answer(&text).unwrap();
            assert_eq!(rows.len(), 3, "全空的那条丢掉");
            assert_eq!((rows[0].code.as_deref(), rows[0].shares, rows[0].cost), (Some("600519"), Some(1200.0), Some(1450.5)));
            assert_eq!(rows[1].code.as_deref(), Some("000001"), "SZ000001 取出 6 位数字");
            assert_eq!((rows[2].code.clone(), rows[2].shares, rows[2].cost), (None, Some(100.0), None), "数字写成 abc 当读不到");
            assert_eq!(warnings, vec!["第二张图有点糊"]);
        }
        assert!(parse_answer("抱歉我看不清").unwrap_err().contains("不是 JSON"));
        assert!(parse_answer("{\"x\":1}").unwrap_err().contains("没有 stocks"));
        assert_eq!(code_of("600519.SH"), Some("600519".into()));
        assert_eq!(code_of("1234567"), None, "连续 7 位不是 6 位代码");
        assert_eq!(code_of("12-3456"), None);
    }

    #[test]
    fn drafts_flag_what_is_missing_and_never_turn_missing_numbers_into_zero() {
        let raw = Raw { code: Some("600519".into()), name: "贵州茅台".into(), shares: Some(100.0), cost: None, note: "".into() };
        let watch = draft_of(&raw, "watch");
        assert_eq!(watch["code"], "600519");
        assert!(watch["uncertain"].as_array().unwrap().is_empty(), "自选不要求数量成本");
        let pos = draft_of(&raw, "position");
        assert!(pos["cost"].is_null());
        assert!(pos["uncertain"].as_array().unwrap().iter().any(|u| u.as_str().unwrap().contains("没有读到成本价")));
        let bad = draft_of(&Raw { code: Some("999999".into()), name: "x".into(), shares: Some(-5.0), cost: Some(0.0), note: "模糊".into() }, "position");
        assert!(bad["code"].is_null() || bad["code"] == "999999");
        let notes: Vec<String> = bad["uncertain"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        assert!(notes.iter().any(|n| n.contains("数量不是正数")) && notes.iter().any(|n| n.contains("成本不是正数")) && notes.iter().any(|n| n.contains("模型备注:模糊")), "{notes:?}");
        let nameless = draft_of(&Raw { code: None, name: "中际旭创".into(), shares: None, cost: None, note: "".into() }, "watch");
        assert!(nameless["uncertain"].as_array().unwrap().iter().any(|u| u.as_str().unwrap().contains("只有名称")));
    }

    #[test]
    fn search_cross_check_flags_name_mismatches_unknown_codes_and_offers_candidates_without_choosing() {
        let items = vec![json!({"symbol": "600519", "name": "贵州茅台"}), json!({"symbol": "000001", "name": "平安银行"})];
        let mut ok = draft_of(&Raw { code: Some("600519".into()), name: "贵州 茅台".into(), shares: None, cost: None, note: "".into() }, "watch");
        apply_search(&mut ok, &items);
        assert_eq!(ok["official_name"], "贵州茅台");
        assert!(ok["uncertain"].as_array().unwrap().is_empty(), "空白差异不算对不上");
        let mut mismatch = draft_of(&Raw { code: Some("600519".into()), name: "平安银行".into(), shares: None, cost: None, note: "".into() }, "watch");
        apply_search(&mut mismatch, &items);
        assert!(mismatch["uncertain"][0].as_str().unwrap().contains("对不上"));
        let mut unknown = draft_of(&Raw { code: Some("600518".into()), name: "康美药业".into(), shares: None, cost: None, note: "".into() }, "watch");
        apply_search(&mut unknown, &items);
        assert!(unknown["uncertain"][0].as_str().unwrap().contains("查不到代码 600518"));
        let mut by_name = draft_of(&Raw { code: None, name: "平安".into(), shares: None, cost: None, note: "".into() }, "watch");
        apply_search(&mut by_name, &items);
        assert!(by_name["code"].is_null(), "只给候选,不替用户选");
        assert_eq!(by_name["candidates"].as_array().unwrap().len(), 2);
        assert!(names_agree("*ST康美", "康美药业") || names_agree("ST康美", "康美"));
        assert!(!names_agree("贵州茅台", "平安银行"));
    }

    #[test]
    fn duplicate_codes_in_one_batch_keep_only_the_first() {
        let mut warnings = Vec::new();
        let drafts = vec![json!({"code": "600519"}), json!({"code": "000001"}), json!({"code": "600519"}), json!({"code": null})];
        let out = dedupe(drafts, &mut warnings);
        assert_eq!(out.len(), 3);
        assert!(warnings[0].contains("600519"));
    }

    /// 起一个最小的 OpenAI 兼容服务,验证真实请求的形状,并把一段模型回答原样回给 recognize
    fn mock_server(answer: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let header_end;
            loop {
                let n = stream.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = p + 4;
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
            let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length:")).map(|v| v.trim().parse().unwrap()).unwrap_or(0);
            while buf.len() < header_end + len {
                let n = stream.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            let request_body = String::from_utf8_lossy(&buf[header_end..header_end + len]).to_string();
            let auth = head.lines().find(|l| l.starts_with("authorization:")).unwrap_or("").to_string();
            let reply = serde_json::to_string(&json!({"choices": [{"message": {"content": answer}}]})).unwrap();
            let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", reply.len(), reply);
            format!("{auth}\n{request_body}")
        });
        (format!("http://{addr}/v1"), handle)
    }

    fn db(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-vision-{name}-{nonce}.sqlite3"));
        crate::storage::init(&path).unwrap();
        crate::journal::init(&path).unwrap();
        crate::daily_review::init(&path).unwrap();
        crate::ai_chat::init(&path).unwrap();
        crate::ai_config::init(&path).unwrap();
        crate::schema::migrate(&path).unwrap();
        path
    }

    #[tokio::test]
    async fn recognize_requires_the_vision_switch_and_sends_the_image_to_the_configured_model() {
        let path = db("run");
        let client = Client::new();
        let body = json!({"images": [image(1)], "target": "position", "__secure_api_key": "sk-test"});
        // 没配模型
        assert!(matches!(recognize(&client, &path, &body).await, Err(VisionError::BadRequest(m)) if m.contains("还没有配置模型")));
        let (url, server) = mock_server(r#"{"stocks":[{"code":"600519","name":"贵州茅台","shares":100,"cost":1450},{"code":null,"name":""}],"warnings":[]}"#);
        crate::ai_config::save(&path, &json!({"base_url": url, "model": "vision-model"}), true).unwrap();
        // 没标支持图片识别:拒绝,什么都不发
        assert!(matches!(recognize(&client, &path, &body).await, Err(VisionError::BadRequest(m)) if m.contains("没有标记为「支持图片识别」")));
        crate::ai_config::save(&path, &json!({"vision": true}), true).unwrap();
        // probe 跳过搜索核对,不依赖外网
        let mut probe_body = body.clone();
        probe_body["probe"] = json!(true);
        let out = recognize(&client, &path, &probe_body).await.unwrap();
        assert_eq!(out["drafts"][0]["code"], "600519");
        assert_eq!((out["drafts"][0]["shares"].clone(), out["drafts"][0]["cost"].clone()), (json!(100.0), json!(1450.0)));
        assert_eq!(out["drafts"].as_array().unwrap().len(), 1);
        assert_eq!(out["probe"], true);
        let seen = server.join().unwrap();
        assert!(seen.contains("authorization: bearer sk-test"), "{seen}");
        let sent: Value = serde_json::from_str(seen.split_once('\n').unwrap().1).unwrap();
        assert_eq!(sent["model"], "vision-model");
        assert_eq!(sent["messages"][1]["content"][1]["image_url"]["url"], "data:image/png;base64,iVBORw0KGgo=");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn upstream_errors_say_when_the_model_is_not_multimodal() {
        let path = db("err");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut chunk = [0u8; 65536];
            let _ = stream.read(&mut chunk);
            let msg = r#"{"error":{"message":"image_url is not supported for this model"}}"#;
            let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", msg.len(), msg);
        });
        crate::ai_config::save(&path, &json!({"base_url": format!("http://{addr}/v1"), "model": "text-only", "vision": true}), false).unwrap();
        let err = recognize(&Client::new(), &path, &json!({"images": [image(1)]})).await.unwrap_err();
        match err {
            VisionError::Upstream(m) => assert!(m.contains("HTTP 400") && m.contains("不是多模态模型") && m.contains("image_url"), "{m}"),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }
}
