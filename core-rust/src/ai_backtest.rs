//! 自然语言回测:用户一句话 → 模型只负责把话翻译成回测工具的参数 → 服务端校验 + 真实运行 → 报告由程序按真实结果生成。
//!
//! 约束(AGENTS.md 6.1):
//!  - 模型先拿到服务端给的工具目录(`tool_catalog`),只能用目录里的字段;信息不足只补问,条件齐备才运行。
//!  - 模型不能覆盖内部字段:参数白名单之外的键一律丢掉并如实列出(`ignored_params`);`__` 开头的键永远不会传进回测。
//!  - 股票由用户写的代码或名称解析:6 位数字当代码,名称去股票搜索里找;**不采信模型自己编的代码**;名称不唯一时补问,不替用户猜。
//!  - 工具拒绝 / 报错 / 缺参数时原样告诉用户,不伪装成完成。
//!  - 结论只来自这一轮真实的工具返回:报告(回测问题 / 回测假设 / 回测逻辑 / 回测结果)由程序生成,模型不写结论。
//!  - 跑完的报告自动存进「研究记录」(本机)。

use crate::{ai_chat::ChatError, ai_config, backtest, documents, search};
use reqwest::Client;
use serde_json::{json, Map, Value};
use std::{path::Path, time::Duration};

const CONTENT_MAX: usize = 1_000;
const HISTORY_MAX: usize = 6;

/// 模型可以填的参数(白名单)。键 → 说明;取值范围由回测工具自己校验并报错
const ALLOWED: [&str; 24] = [
    "strategy",
    "fast",
    "slow",
    "entry",
    "exit",
    "period",
    "buy",
    "sell",
    "bars",
    "initial_capital",
    "fees",
    "slippage_bps",
    "position_pct",
    "stop_loss_pct",
    "limit_check",
    "experiment",
    "optimize",
    "walk_forward",
    "benchmark",
    "rebalance_every",
    "market_rules", "start_date", "end_date", "historical_rules",
];

/// 给模型和前端看的工具目录:和 `backtest::run` 实际接受的字段与范围一致
pub fn tool_catalog() -> Value {
    json!({
        "tool": "backtest",
        "description": "日线前复权回测:信号用前一交易日收盘,下一交易日开盘成交;默认计佣金/过户费/卖出印花税,按 100 股整手,涨跌停按板块近似,零量及指数日历缺失日禁止成交，ST/新股须有明确历史制度区间",
        "stocks": "用户写的股票代码或名称,最多 5 只,放在 stocks 数组里原样照抄用户的写法;不要自己猜代码",
        "strategies": {
            "sma_cross": {"label": "均线交叉", "params": {"fast": "3–60,默认 10", "slow": "10–200,默认 30,必须大于 fast"}},
            "donchian": {"label": "通道突破", "params": {"entry": "5–120,默认 20", "exit": "3–60,默认 10"}},
            "rsi": {"label": "RSI 超卖反弹", "params": {"period": "5–30,默认 14", "buy": "5–45,默认 30", "sell": "55–95,默认 70"}},
        },
        "common_params": {
            "start_date":"可选起点 YYYY-MM-DD，取得区间含策略预热期，源站未覆盖时拒绝", "end_date":"可选终点 YYYY-MM-DD，不允许未来",
            "bars": "日线根数,默认 500,最多 4000,实际可用范围以数据源返回为准", "initial_capital": "本金(元),默认 100000", "fees": "\"default\"(默认费率)或 \"none\"",
            "slippage_bps": "滑点(万分之几),0–200,默认 0", "position_pct": "每次进场仓位 %,10–100,默认 100", "stop_loss_pct": "止损 %,1–50,不填 = 不止损",
            "limit_check": "是否模拟涨跌停,默认 true", "experiment": "固定小格参数实验,仅单只", "optimize": "前 70% 选参数 / 后 30% 检验,仅单只",
            "walk_forward":"滚动样本外:252日训练/63日检验,仅单只", "benchmark":"同期沪深300价格指数对照",
            "market_rules":"用户明确给定的历史制度区间数组，每项symbol/start/end/limit_pct；0不限幅，不得猜测ST或上市日期",
            "historical_rules":"auto(默认)取沪深逐日ST/停牌与上市日，失败披露；required缺失即拒绝；off仅按板块和明确区间近似。不得自行关闭或放松用户要求",
            "rebalance_every":"组合共享现金等权再平衡间隔(1–252个交易日),需要至少两只;不填为各自独立",
        },
    })
}

fn system_prompt() -> String {
    format!(
        "你是回测助手,只做一件事:把用户的话翻译成回测工具的参数。只输出一个 JSON 对象,不要任何别的文字。\n\
         信息不足(没说股票、策略说不清、要求的东西工具做不到)时输出 {{\"action\":\"ask\",\"question\":\"一句要补问的话\"}},只补问,不要猜。\n\
         条件齐备时输出 {{\"action\":\"run\",\"stocks\":[\"用户写的代码或名称\"],\"params\":{{...}}}}。params 只能用工具目录里的字段和范围,没提到的字段不要填(用默认);不要发明字段。\n\
         不要给买卖建议,不要预测行情,不要写结论。\n工具目录:{}",
        tool_catalog()
    )
}

/// 从模型回复里取出 JSON 对象(容忍 ``` 代码块和前后多余文字)
pub(crate) fn extract_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

#[derive(Debug)]
pub(crate) enum Plan {
    Ask(String),
    Run {
        stocks: Vec<String>,
        params: Map<String, Value>,
    },
}

/// 校验模型给的计划:动作必须是 ask / run;run 的 stocks 必须是 1–5 个非空字符串;params 必须是对象
pub(crate) fn parse_plan(value: &Value) -> Result<Plan, String> {
    match value.get("action").and_then(Value::as_str) {
        Some("ask") => {
            let q = value
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if q.is_empty() {
                return Err("模型要补问,但没有给出问题".into());
            }
            Ok(Plan::Ask(q.chars().take(200).collect()))
        }
        Some("run") => {
            let stocks: Vec<String> = value
                .get("stocks")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            if stocks.is_empty() {
                return Err("模型没有给出股票".into());
            }
            if stocks.len() > 5 {
                return Err("一次最多回测 5 只股票".into());
            }
            let params = match value.get("params") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(m)) => m.clone(),
                Some(_) => return Err("params 必须是对象".into()),
            };
            Ok(Plan::Run { stocks, params })
        }
        _ => Err("模型的回复不是约定的 ask / run".into()),
    }
}

/// 白名单过滤:返回(要传给回测的参数, 被丢掉的键)。`__` 开头和不在白名单里的键都丢掉
pub(crate) fn filter_params(params: &Map<String, Value>) -> (Map<String, Value>, Vec<String>) {
    let mut kept = Map::new();
    let mut ignored = Vec::new();
    for (k, v) in params {
        if ALLOWED.contains(&k.as_str()) && !v.is_null() {
            // fees 只接受 "default" / "none":不让模型塞自定义费率对象
            if k == "fees" && !matches!(v.as_str(), Some("default") | Some("none")) {
                ignored.push(k.clone());
                continue;
            }
            kept.insert(k.clone(), v.clone());
        } else {
            ignored.push(k.clone());
        }
    }
    ignored.sort();
    (kept, ignored)
}

pub(crate) enum Pick {
    Code(String),
    Ambiguous(Vec<(String, String)>),
    None,
}

/// 按名称搜到的结果里挑一只:名称完全一致且唯一优先;只搜到一只也算;多只就返回候选,不替用户猜
pub(crate) fn pick_stock(token: &str, items: &[Value]) -> Pick {
    let squash = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_uppercase()
    };
    let rows: Vec<(String, String)> = items
        .iter()
        .filter_map(|i| {
            Some((
                i.get("symbol")?.as_str()?.to_string(),
                i.get("name")?.as_str()?.to_string(),
            ))
        })
        .collect();
    if rows.is_empty() {
        return Pick::None;
    }
    let exact: Vec<&(String, String)> = rows
        .iter()
        .filter(|(_, n)| squash(n) == squash(token))
        .collect();
    if exact.len() == 1 {
        return Pick::Code(exact[0].0.clone());
    }
    if exact.is_empty() && rows.len() == 1 {
        return Pick::Code(rows[0].0.clone());
    }
    Pick::Ambiguous(rows.into_iter().take(5).collect())
}

enum Resolved {
    Codes(Vec<String>),
    Ask(String),
}

async fn resolve_stocks(client: &Client, tokens: &[String]) -> Result<Resolved, String> {
    let mut codes: Vec<String> = Vec::new();
    for t in tokens {
        if t.len() == 6 && t.chars().all(|c| c.is_ascii_digit()) {
            if !codes.contains(t) {
                codes.push(t.clone());
            }
            continue;
        }
        let found = search::symbols(client, &json!({"query": t, "limit": 8}))
            .await
            .map_err(|e| format!("股票搜索没回应,没法按名称确认「{t}」:{e:?}"))?;
        let items = found["items"].as_array().cloned().unwrap_or_default();
        match pick_stock(t, &items) {
            Pick::Code(c) => {
                if !codes.contains(&c) {
                    codes.push(c);
                }
            }
            Pick::None => {
                return Ok(Resolved::Ask(format!(
                    "没搜到「{t}」(只支持沪深北 A 股),请换成 6 位代码或写全名称"
                )))
            }
            Pick::Ambiguous(c) => {
                return Ok(Resolved::Ask(format!(
                    "「{t}」有多只:{},请告诉我是哪一只(或直接写代码)",
                    c.iter()
                        .map(|(s, n)| format!("{n} {s}"))
                        .collect::<Vec<_>>()
                        .join("、")
                )));
            }
        }
    }
    Ok(Resolved::Codes(codes))
}

fn fmt_pct(v: &Value) -> String {
    v.as_f64()
        .map(|x| format!("{x:+.2}%"))
        .unwrap_or_else(|| "—".to_string())
}

fn fmt_num(v: &Value, digits: usize) -> String {
    v.as_f64()
        .map(|x| format!("{x:.digits$}"))
        .unwrap_or_else(|| "—".to_string())
}

/// 报告:全部来自这一轮真实的回测返回和用户原话,四个固定小节;不写任何判断或建议
pub(crate) fn report_markdown(
    user_text: &str,
    used: &Map<String, Value>,
    ignored: &[String],
    result: &Value,
) -> String {
    let m = &result["metrics"];
    let st = &result["strategy"];
    let symbols: Vec<String> = result["symbols"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let mut out = String::new();
    out.push_str("# 回测报告(历史假设核对)\n\n## 回测问题\n");
    out.push_str(&format!("{}\n\n", user_text.trim()));
    out.push_str("## 回测假设\n");
    out.push_str(&format!("- 标的:{}\n", symbols.join("、")));
    out.push_str(&format!(
        "- 区间:{} → {}({} 根日线,前复权)\n",
        result["period"]["start"].as_str().unwrap_or("—"),
        result["period"]["end"].as_str().unwrap_or("—"),
        result["period"]["bars"]
    ));
    out.push_str(&format!(
        "- 本金:{} 元;每次进场仓位 {}%\n",
        fmt_num(&m["initial_capital"], 0),
        result["costs"]["position_pct"].as_f64().unwrap_or(100.0)
    ));
    out.push_str(&format!("- 成本:{}\n", st["fees"].as_str().unwrap_or("—")));
    out.push_str(&format!(
        "- 涨跌停:{}\n",
        if st["limit_check"].as_bool().unwrap_or(true) {
            "按板块幅度近似模拟"
        } else {
            "未模拟"
        }
    ));
    if let Some(sl) = st["stop_loss_pct"].as_f64() {
        out.push_str(&format!("- 止损:成本价下方 {sl}%\n"));
    }
    if !used.is_empty() {
        out.push_str(&format!(
            "- 实际传给回测工具的参数:{}\n",
            Value::Object(used.clone())
        ));
    }
    if !ignored.is_empty() {
        out.push_str(&format!(
            "- 模型给出但被工具白名单丢掉的参数:{}\n",
            ignored.join("、")
        ));
    }
    out.push_str("\n## 回测逻辑\n");
    out.push_str(&format!(
        "- 策略:{}({})\n",
        st["name"].as_str().unwrap_or("—"),
        st["label"].as_str().unwrap_or("—")
    ));
    out.push_str(&format!(
        "- 执行:{};按 100 股整手、{}\n",
        st["execution"].as_str().unwrap_or("—"),
        st["weighting"].as_str().unwrap_or("单只全仓")
    ));
    out.push_str("\n## 回测结果\n");
    out.push_str(&format!(
        "- 策略收益 {},买入持有 {},超额 {}\n",
        fmt_pct(&m["total_return_pct"]),
        fmt_pct(&m["buy_hold_return_pct"]),
        fmt_pct(&m["excess_return_pct"])
    ));
    out.push_str(&format!(
        "- 最大回撤 {},年化 {},夏普(日度){}\n",
        fmt_pct(&m["max_drawdown_pct"]),
        fmt_pct(&m["annualized_return_pct"]),
        fmt_num(&m["sharpe"], 2)
    ));
    out.push_str(&format!(
        "- 已平仓 {} 笔,胜率 {};手续费 {} 元,滑点成本 {} 元\n",
        m["closed_trades"],
        fmt_pct(&m["win_rate_pct"]).trim_start_matches('+'),
        fmt_num(&m["total_fees"], 2),
        fmt_num(&m["slippage_cost"], 2)
    ));
    if m["blocked_buys"].as_u64().unwrap_or(0) + m["blocked_sells"].as_u64().unwrap_or(0) > 0 {
        out.push_str(&format!(
            "- 涨跌停没成交:买 {} 次、卖 {} 次\n",
            m["blocked_buys"], m["blocked_sells"]
        ));
    }
    if let Some(dd) = result["drawdowns"].as_array().and_then(|a| a.first()) {
        out.push_str(&format!(
            "- 最深一段回撤 {}({} → {},{})\n",
            fmt_pct(&dd["depth_pct"]),
            dd["peak_date"].as_str().unwrap_or("—"),
            dd["trough_date"].as_str().unwrap_or("—"),
            dd["recover_date"]
                .as_str()
                .map(|d| format!("{d} 恢复"))
                .unwrap_or_else(|| "到最后还没恢复".to_string())
        ));
    }
    if let Some(o) = result.get("optimize").filter(|o| o["available"] == true) {
        let b = &o["best_in_sample"];
        out.push_str(&format!("- 样本内/外对照:样本内最好 {}(样本内 {} → 样本外 {},样本外排第 {}/{});全部参数样本外收益中位数 {}\n", b["label"].as_str().unwrap_or("—"), fmt_pct(&b["is_return_pct"]), fmt_pct(&b["oos_return_pct"]), b["oos_rank"], b["of"], fmt_pct(&o["median_oos_return_pct"])));
    }
    if let Some(e) = result.get("experiment") {
        out.push_str(&format!(
            "- 参数实验:{} 组,{} 为正收益\n",
            e["rows"].as_array().map(|r| r.len()).unwrap_or(0),
            e["positive_share"]
                .as_f64()
                .map(|p| format!("{:.0}%", p * 100.0))
                .unwrap_or_else(|| "—".to_string())
        ));
    }
    for (key,title) in [("sharpe_interval","夏普区间"),("walk_forward","滚动样本外"),("benchmark","同期基准"),("cost_comparison","成本对照"),("suspension","停牌与日历"),("market_rules","历史制度边界"),("data_sources","实际行情来源")] {
        if let Some(value)=result.get(key) {out.push_str(&format!("\n### {title}\n\n```json\n{}\n```\n",serde_json::to_string_pretty(value).unwrap_or_default()));}
    }
    out.push_str(&format!(
        "\n> {}。以上只是历史假设核对,不构成买卖建议。\n",
        result["note"].as_str().unwrap_or("")
    ));
    out
}

fn bad(message: impl Into<String>) -> ChatError {
    ChatError::BadRequest(message.into())
}

/// 入口。返回 status:needs_info(要补问)/ ok(跑完)/ tool_error(工具拒绝或失败,原样告知)
pub async fn run(client: &Client, db_path: &Path, body: &Value) -> Result<Value, ChatError> {
    let content = body
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if content.is_empty() {
        return Err(bad("请先描述要回测什么"));
    }
    if content.chars().count() > CONTENT_MAX {
        return Err(bad(format!("描述最多 {CONTENT_MAX} 个字符")));
    }
    // 之前补问过的几轮(只收 user / assistant 文字)
    let mut messages = vec![json!({"role": "system", "content": system_prompt()})];
    if let Some(history) = body.get("history").and_then(Value::as_array) {
        for item in history.iter().rev().take(HISTORY_MAX).rev() {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("");
            let text = item
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if matches!(role, "user" | "assistant")
                && !text.is_empty()
                && text.chars().count() <= CONTENT_MAX
            {
                messages.push(json!({"role": role, "content": text}));
            }
        }
    }
    messages.push(json!({"role": "user", "content": &content}));
    let config = ai_config::load(db_path)
        .map_err(ChatError::Internal)?
        .ok_or_else(|| bad("还没有配置模型 API"))?;
    if config.base_url.is_empty() || config.model.is_empty() {
        return Err(bad("模型 API 配置不完整"));
    }
    let mut request = client
        .post(format!(
            "{}/chat/completions",
            config.base_url.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(60))
        .header("Accept", "application/json")
        .json(&json!({"model": &config.model, "messages": &messages}));
    let key = ai_config::injected_api_key(body);
    if !key.is_empty() {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|e| ChatError::Upstream(format!("连接模型 API 失败: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ChatError::Upstream(format!("读取模型响应失败: {e}")))?;
    if !status.is_success() {
        return Err(ChatError::Upstream(format!(
            "模型 API 返回 HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
        )));
    }
    let root: Value = serde_json::from_str(&text)
        .map_err(|e| ChatError::Upstream(format!("模型响应不是有效 JSON: {e}")))?;
    let answer = crate::ai_chat::answer_of(&root).ok_or_else(|| {
        ChatError::Upstream("模型响应缺少 choices[0].message.content".to_string())
    })?;
    let plan_value = match extract_json(&answer) {
        Some(v) => v,
        None => {
            return Ok(
                json!({"status": "tool_error", "message": "模型的回复里没有可用的 JSON,没有运行回测;请换个说法再试", "model_reply": answer.chars().take(300).collect::<String>()}),
            )
        }
    };
    let plan = match parse_plan(&plan_value) {
        Ok(p) => p,
        Err(message) => {
            return Ok(
                json!({"status": "tool_error", "message": format!("{message},没有运行回测;请换个说法再试")}),
            )
        }
    };
    let (stocks, params) = match plan {
        Plan::Ask(question) => return Ok(json!({"status": "needs_info", "question": question})),
        Plan::Run { stocks, params } => (stocks, params),
    };
    let mut user_text = content.clone();
    for h in body["history"].as_array().into_iter().flatten() {
        if h["role"] == "user" {
            user_text.push_str(h["content"].as_str().unwrap_or(""));
        }
    }
    let normalized: String = user_text.chars().filter(|c| !c.is_whitespace()).collect();
    if stocks.iter().any(|s| {
        !normalized.contains(&s.chars().filter(|c| !c.is_whitespace()).collect::<String>())
    }) {
        return Ok(
            json!({"status":"needs_info","question":"模型给出的标的不完全来自你的原话。请直接写股票代码或完整名称,没有运行回测。"}),
        );
    }
    let codes = match resolve_stocks(client, &stocks).await {
        Ok(Resolved::Codes(c)) => c,
        Ok(Resolved::Ask(question)) => {
            return Ok(json!({"status": "needs_info", "question": question}))
        }
        Err(message) => return Ok(json!({"status": "tool_error", "message": message})),
    };
    let (mut kept, ignored) = filter_params(&params);
    if codes.len() > 1 {
        kept.insert("symbols".into(), json!(codes));
    } else {
        kept.insert("symbol".into(), json!(codes[0]));
    }
    let request_body = Value::Object(kept.clone());
    let result = match backtest::run(client, &request_body).await {
        Ok(v) => v,
        // 工具拒绝或失败:原样告诉用户,不伪装成完成
        Err(error) => {
            return Ok(
                json!({"status": "tool_error", "message": format!("回测工具没有运行成功:{error}"), "params_used": kept, "ignored_params": ignored}),
            )
        }
    };
    Ok(finish(
        db_path,
        &content,
        &codes,
        &kept,
        &ignored,
        result,
        &config.model,
    ))
}

/// 回测跑完之后:程序生成报告、存进研究记录、组装返回(不碰网络,测试直接喂真实结构的结果)
pub(crate) fn finish(
    db_path: &Path,
    content: &str,
    codes: &[String],
    kept: &Map<String, Value>,
    ignored: &[String],
    result: Value,
    model: &str,
) -> Value {
    let report = report_markdown(content, kept, ignored, &result);
    let title = format!(
        "回测:{} {}",
        codes.join("、"),
        result["strategy"]["label"].as_str().unwrap_or("")
    );
    let saved = documents::save(
        db_path,
        &json!({"kind": "note", "category": "回测", "title": title.chars().take(100).collect::<String>(), "body": report, "source": "AI 回测"}),
    );
    json!({
        "status": "ok", "report_markdown": report, "result": result, "params_used": kept, "ignored_params": ignored,
        "resolved_symbols": codes, "saved_document_id": saved.as_ref().ok().and_then(|d| d["id"].as_i64()),
        "save_error": saved.err().map(|e| e.to_string()), "model": model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_lists_every_strategy_and_the_whitelist_matches_the_tool() {
        let c = tool_catalog();
        for s in ["sma_cross", "donchian", "rsi"] {
            assert!(c["strategies"].get(s).is_some(), "{s}");
        }
        for k in [
            "bars",
            "initial_capital",
            "fees",
            "slippage_bps",
            "position_pct",
            "stop_loss_pct",
            "limit_check",
            "experiment",
            "optimize",
        ] {
            assert!(c["common_params"].get(k).is_some(), "{k}");
            assert!(ALLOWED.contains(&k), "{k}");
        }
        assert!(system_prompt().contains("只补问") && system_prompt().contains("不要给买卖建议"));
    }

    #[test]
    fn json_is_pulled_out_of_fences_and_chatter_and_bad_plans_are_rejected() {
        let v =
            extract_json("好的:\n```json\n{\"action\":\"ask\",\"question\":\"哪只股票?\"}\n```")
                .unwrap();
        assert!(matches!(parse_plan(&v), Ok(Plan::Ask(q)) if q == "哪只股票?"));
        assert!(extract_json("没有 json").is_none());
        assert!(parse_plan(&json!({"action": "ask"})).is_err());
        assert!(parse_plan(&json!({"action": "run", "stocks": []})).is_err());
        assert!(
            parse_plan(&json!({"action": "run", "stocks": ["1", "2", "3", "4", "5", "6"]}))
                .unwrap_err()
                .contains("最多回测 5 只")
        );
        assert!(
            parse_plan(&json!({"action": "run", "stocks": ["600519"], "params": "x"}))
                .unwrap_err()
                .contains("对象")
        );
        assert!(parse_plan(&json!({"action": "sell"})).is_err());
        assert!(
            matches!(parse_plan(&json!({"action": "run", "stocks": [" 贵州茅台 "]})), Ok(Plan::Run { stocks, params }) if stocks == ["贵州茅台"] && params.is_empty())
        );
    }

    #[test]
    fn model_cannot_smuggle_internal_fields_or_custom_fees() {
        let params = json!({"strategy": "rsi", "buy": 25, "__secure_api_key": "sk-x", "action": "delete", "symbol": "000001", "fees": {"commission_rate": 0}, "stop_loss_pct": null, "limit_check": false, "evil": 1});
        let (kept, ignored) = filter_params(params.as_object().unwrap());
        assert_eq!(
            Value::Object(kept.clone()),
            json!({"strategy": "rsi", "buy": 25, "limit_check": false})
        );
        assert_eq!(
            ignored,
            [
                "__secure_api_key",
                "action",
                "evil",
                "fees",
                "stop_loss_pct",
                "symbol"
            ]
        );
        let (ok, _) = filter_params(json!({"fees": "none"}).as_object().unwrap());
        assert_eq!(ok["fees"], "none");
    }

    #[test]
    fn stock_names_resolve_only_when_unambiguous() {
        let items = vec![
            json!({"symbol": "000001", "name": "平安银行"}),
            json!({"symbol": "601318", "name": "中国平安"}),
        ];
        assert!(matches!(pick_stock("平安银行", &items), Pick::Code(c) if c == "000001"));
        assert!(
            matches!(pick_stock("平安", &items), Pick::Ambiguous(c) if c.len() == 2),
            "名称不完全一致又有多只:列候选,不猜"
        );
        assert!(
            matches!(pick_stock("茅台", &[json!({"symbol": "600519", "name": "贵州茅台"})]), Pick::Code(c) if c == "600519")
        );
        assert!(matches!(pick_stock("x", &[]), Pick::None));
        assert!(
            matches!(pick_stock("ST 国华", &[json!({"symbol": "000004", "name": "ST国华"}), json!({"symbol": "000005", "name": "世纪星源"})]), Pick::Code(c) if c == "000004")
        );
    }

    /// 本地模拟模型:按顺序回复每个请求,返回(地址, 收到的请求体们)
    fn model_server(replies: Vec<&'static str>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut bodies = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = stream.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..at]).to_lowercase();
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .map(|v| v.trim().parse().unwrap())
                            .unwrap_or(0);
                        if buf.len() >= at + 4 + len {
                            break;
                        }
                    }
                }
                let at = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                bodies.push(String::from_utf8(buf[at..].to_vec()).unwrap());
                let msg = json!({"choices": [{"message": {"content": reply}}]}).to_string();
                let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", msg.len(), msg);
            }
            bodies
        });
        (format!("http://{addr}/v1"), handle)
    }

    #[tokio::test]
    async fn ask_and_garbage_replies_never_run_the_backtest_and_the_prompt_carries_the_catalog() {
        let (url, server) = model_server(vec![
            "{\"action\":\"ask\",\"question\":\"想回测哪只股票?\"}",
            "我觉得可以买入",
        ]);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-nlbt-{nonce}"));
        let core = crate::AppCore::new(dir.clone()).unwrap();
        ai_config::save(
            core.db_path(),
            &json!({"base_url": url, "model": "m"}),
            false,
        )
        .unwrap();
        let client = Client::new();
        let history = json!([{"role": "user", "content": "之前的话"}, {"role": "system", "content": "不收 system"}, {"role": "assistant", "content": "之前的补问"}]);
        let first = run(
            &client,
            core.db_path(),
            &json!({"content": "帮我回测一个策略", "history": history, "__secure_api_key": "sk-x"}),
        )
        .await
        .unwrap();
        assert_eq!(
            (first["status"].as_str(), first["question"].as_str()),
            (Some("needs_info"), Some("想回测哪只股票?"))
        );
        let second = run(&client, core.db_path(), &json!({"content": "随便"}))
            .await
            .unwrap();
        assert_eq!(
            second["status"], "tool_error",
            "模型没给 JSON:原样告知,不运行"
        );
        assert!(second["message"]
            .as_str()
            .unwrap()
            .contains("没有可用的 JSON"));
        let bodies = server.join().unwrap();
        let sent: Value = serde_json::from_str(&bodies[0]).unwrap();
        let msgs = sent["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert!(
            msgs[0]["content"].as_str().unwrap().contains("sma_cross")
                && msgs[0]["content"].as_str().unwrap().contains("只补问")
        );
        assert_eq!(
            msgs.len(),
            4,
            "system + 两条历史(system 角色的丢掉)+ 本次用户话"
        );
        assert_eq!(msgs[3]["content"], "帮我回测一个策略");
        assert!(!bodies[0].contains("sk-x"), "密钥不进请求体");
        assert_eq!(
            core.handle(crate::MobileApiRequest {
                method: "GET".into(),
                path: "/mobile/backtest/tools".into(),
                body: json!({})
            })
            .await
            .body["tool"],
            "backtest"
        );
        // 没配模型 / 空描述
        let empty = run(&client, core.db_path(), &json!({"content": "  "})).await;
        assert!(matches!(empty, Err(ChatError::BadRequest(m)) if m.contains("描述")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn finished_runs_are_saved_to_the_local_notes_with_the_real_report() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-nlbt-finish-{nonce}"));
        let core = crate::AppCore::new(dir.clone()).unwrap();
        let result = json!({
            "symbols": ["600519"], "period": {"start": "2025-01-02", "end": "2026-06-30", "bars": 360}, "costs": {"position_pct": 100.0},
            "strategy": {"name": "SMA 趋势交叉 Lite", "label": "MA10 / MA30", "fees": "未计手续费", "execution": "x", "limit_check": false, "weighting": "单只全仓"},
            "metrics": {"initial_capital": 100000.0, "total_return_pct": -2.0, "buy_hold_return_pct": 5.0, "excess_return_pct": -7.0, "max_drawdown_pct": -9.0, "closed_trades": 0, "win_rate_pct": null},
            "note": "这是移动端 Lite 回测",
        });
        let kept = json!({"strategy": "sma_cross", "symbol": "600519"})
            .as_object()
            .unwrap()
            .clone();
        let out = finish(
            core.db_path(),
            "回测茅台均线",
            &["600519".to_string()],
            &kept,
            &[],
            result,
            "m",
        );
        assert_eq!(out["status"], "ok");
        let id = out["saved_document_id"].as_i64().expect("存进了研究记录");
        let list = documents::list(core.db_path(), "/mobile/documents?kind=note").unwrap();
        let doc = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .unwrap();
        assert_eq!(
            (doc["category"].as_str(), doc["source"].as_str()),
            (Some("回测"), Some("AI 回测"))
        );
        assert!(
            doc["title"].as_str().unwrap().contains("600519")
                && doc["body"].as_str().unwrap().contains("策略收益 -2.00%")
        );
        assert_eq!(
            out["report_markdown"].as_str().unwrap().trim(),
            doc["body"].as_str().unwrap().trim()
        );
        assert!(out["save_error"].is_null());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn report_has_four_fixed_sections_and_only_real_numbers() {
        let result = json!({
            "symbols": ["600519"], "period": {"start": "2025-01-02", "end": "2026-06-30", "bars": 360},
            "costs": {"position_pct": 50.0},
            "strategy": {"name": "唐奇安通道突破 Lite", "label": "突破 20 日高 / 跌破 10 日低", "fees": "已计佣金、过户费与卖出印花税(费率见 costs);未计滑点", "execution": "信号使用前一交易日收盘,下一交易日开盘成交", "limit_check": true, "stop_loss_pct": 5.0, "weighting": "单只全仓"},
            "metrics": {"initial_capital": 100000.0, "total_return_pct": 8.0, "buy_hold_return_pct": 5.0, "excess_return_pct": 3.0, "max_drawdown_pct": -4.5, "annualized_return_pct": null, "sharpe": 1.234, "closed_trades": 2, "win_rate_pct": 50.0, "total_fees": 188.5, "slippage_cost": 0.0, "blocked_buys": 1, "blocked_sells": 0},
            "drawdowns": [{"depth_pct": -4.5, "peak_date": "2026-03-04", "trough_date": "2026-04-20", "recover_date": null}],
            "note": "这是移动端 Lite 回测",
        });
        let used = json!({"strategy": "donchian", "symbol": "600519"})
            .as_object()
            .unwrap()
            .clone();
        let md = report_markdown("用通道突破回测茅台", &used, &["evil".to_string()], &result);
        for part in [
            "## 回测问题",
            "用通道突破回测茅台",
            "## 回测假设",
            "## 回测逻辑",
            "## 回测结果",
            "策略收益 +8.00%",
            "年化 —",
            "夏普(日度)1.23",
            "涨跌停没成交:买 1 次、卖 0 次",
            "到最后还没恢复",
            "被工具白名单丢掉的参数:evil",
            "每次进场仓位 50%",
            "不构成买卖建议",
        ] {
            assert!(md.contains(part), "{part}\n{md}");
        }
        assert!(!md.contains("建议买入") && !md.contains("建议卖出"));
    }
}
