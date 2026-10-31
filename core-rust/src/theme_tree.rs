//! 题材事件树(移植自 .agents/skills/data-access/scripts/sources/shortline_themes.py,口径来自 Vibe AStock v2.0.0 duanxian/theme_tree.py)。
//!
//! 按涨停原因串(真实事件题材,同花顺涨停揭秘)复盘,不按行业分类:「间接投资长鑫科技」「兵装重组」在行业分类里找不到。
//! 每个题材按复盘顺序摆事实:昨日该题材涨停股今天什么反馈 → 几点首封 → 扩散到几只 → 连板多高 → 炸了几个。
//! 拿不到题材串时整棵树标不可用,**绝不退回行业分类冒充题材**(行业结构在「行业题材」页)。
//!
//! 与桌面版的差异:涨停原因只走同花顺涨停揭秘(桌面版配了问财密钥时先问问财;手机端不存问财密钥)。
//! 同花顺涨停揭秘不含北交所;逐行核对首次封板时间属于这一天(请求参数不算日期证据);翻页拿全,缺页 / 重复 / 数量对不上都整份不用,
//! 不交半份名单。已收盘日子的原因串存本机(公开数据,不进备份),源站编辑归因非公司确认。

use crate::{
    discovery::DiscoveryError,
    emotion::{self, PoolKind, Row},
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    time::Duration,
};

const THS_URL: &str = "https://data.10jqka.com.cn/dataapi/limit_up/limit_up_pool";
const THS_FIELDS: &str =
    "199112,10,9001,330323,330324,330325,9002,330329,133971,133970,1968584,3475914,9003,9004";
const THS_PAGE: usize = 200;
const THS_PAGES: usize = 20;
const MAX_TAGS: usize = 4;
const MAX_LEN: usize = 40;
const TOP_DEFAULT: usize = 10;
const GENERIC: [&str; 13] = [
    "国企改革",
    "央企改革",
    "破净",
    "低价股",
    "高送转",
    "摘帽",
    "壳资源",
    "小盘股",
    "微盘股",
    "次新股",
    "超跌反弹",
    "股权激励",
    "员工持股",
];
const GENERIC_SUFFIX: [&str; 3] = ["国资", "国企", "板块"];

fn is_generic(tag: &str) -> bool {
    GENERIC.contains(&tag) || GENERIC_SUFFIX.iter().any(|s| tag.ends_with(s))
}

/// 题材串清洗:统一分隔符,最多 4 个标签;超长时整个丢掉后面的标签,不截成半个词
pub(crate) fn clean_reason(text: &str) -> String {
    let s = text.replace(['，', ','], "+");
    let mut out: Vec<String> = Vec::new();
    for t in s.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        if !out.is_empty() && format!("{}+{}", out.join("+"), t).chars().count() > MAX_LEN {
            break;
        }
        out.push(t.chars().take(MAX_LEN).collect());
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out.join("+")
}

/// 题材串 → 事件标签。过滤纯属性词(国企改革 / 破净 / xx国资…)
pub(crate) fn tags_of(reason: &str) -> Vec<String> {
    reason
        .split('+')
        .map(str::trim)
        .filter(|t| !t.is_empty() && !is_generic(t))
        .map(String::from)
        .collect()
}

// ---------------------------------------------------------------- 同花顺涨停原因

/// 解析一页同花顺涨停揭秘。逐行核对首次封板时间属于 `day`,代码无效 / 重复 / 缺封板时间整页拒绝
pub(crate) fn parse_ths_page(
    root: &Value,
    day: NaiveDate,
    seen: &mut HashSet<String>,
    reasons: &mut HashMap<String, String>,
) -> Result<(usize, usize)> {
    if root["status_code"].as_i64() != Some(0) {
        return Err(anyhow!("同花顺涨停揭秘没有成功返回"));
    }
    let data = &root["data"];
    let total = data["page"]["total"]
        .as_u64()
        .or_else(|| data["page"]["total"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0) as usize;
    let rows = data["info"].as_array().cloned().unwrap_or_default();
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid offset");
    for row in &rows {
        let code = row["code"].as_str().unwrap_or("").to_string();
        if code.len() != 6
            || !code.bytes().all(|b| b.is_ascii_digit())
            || !seen.insert(code.clone())
        {
            return Err(anyhow!("同花顺涨停揭秘有重复或无效代码,分页不完整"));
        }
        let stamp = row["first_limit_up_time"].as_f64().or_else(|| {
            row["first_limit_up_time"]
                .as_str()
                .and_then(|s| s.parse().ok())
        });
        let when = stamp
            .and_then(|s| DateTime::from_timestamp(s as i64, 0))
            .map(|t| t.with_timezone(&offset).date_naive());
        match when {
            None => return Err(anyhow!("{code} 缺少有效封板时间,核对不了是哪一天")),
            Some(w) if w != day => return Err(anyhow!("同花顺返回的是别的交易日,未使用")),
            _ => {}
        }
        if let Some(reason) = row["reason_type"]
            .as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty() && *r != "-" && *r != "暂无")
        {
            reasons.insert(code, clean_reason(reason));
        }
    }
    Ok((rows.len(), total))
}

async fn ths_reasons(client: &Client, d8: &str) -> Result<(HashMap<String, String>, String)> {
    let day = NaiveDate::parse_from_str(d8, "%Y%m%d")?;
    let (mut seen, mut reasons): (HashSet<String>, HashMap<String, String>) =
        (HashSet::new(), HashMap::new());
    let mut total: Option<usize> = None;
    for page in 1..=THS_PAGES {
        let root: Value = client
            .get(THS_URL)
            .query(&[
                ("page", page.to_string()),
                ("limit", THS_PAGE.to_string()),
                ("field", THS_FIELDS.into()),
                ("filter", "HS,GEM2STAR".into()),
                ("order_field", "330324".into()),
                ("order_type", "0".into()),
                ("date", d8.into()),
            ])
            .header("User-Agent", "Mozilla/5.0")
            .timeout(Duration::from_secs(12))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let (n, count) = parse_ths_page(&root, day, &mut seen, &mut reasons)?;
        if total.is_some_and(|t| t != count) {
            return Err(anyhow!("翻页期间名单变了,请重试"));
        }
        total = Some(count);
        if n == 0 && seen.len() < count {
            return Err(anyhow!("同花顺涨停揭秘分页缺失"));
        }
        if seen.len() >= count {
            if seen.len() != count {
                return Err(anyhow!("同花顺涨停揭秘分页数量对不上"));
            }
            if seen.is_empty() {
                return Err(anyhow!("同花顺 {} 没返回涨停记录,核对不了是哪一天", day));
            }
            let note = format!("来源:同花顺涨停揭秘 {day};原因覆盖 {}/{} 只;封板时间逐行核对;沪深主板 / 创业板 / 科创板,不含北交所;源站编辑归因非公司确认。", reasons.len(), seen.len());
            return Ok((reasons, note));
        }
    }
    Err(anyhow!("同花顺涨停揭秘翻页超过上限,不交半份名单"))
}

fn cache_load(db: &Path, date: &str) -> Option<(HashMap<String, String>, String)> {
    let conn = Connection::open(db).ok()?;
    let (payload, note): (String, String) = conn
        .query_row(
            "SELECT payload, note FROM mobile_ths_reasons WHERE date = ?1",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    let map: HashMap<String, String> = serde_json::from_str(&payload).ok()?;
    (!map.is_empty()).then_some((map, note))
}

fn cache_store(db: &Path, date: &str, map: &HashMap<String, String>, note: &str) -> Result<()> {
    Connection::open(db)?.execute(
        "INSERT INTO mobile_ths_reasons(date, payload, note, fetched_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, note = excluded.note, fetched_at = excluded.fetched_at",
        params![date, serde_json::to_string(map)?, note, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// 某个交易日的 代码 → 涨停原因串。取不到返回错误原因。已收盘(settled)的日子读 / 写本机缓存
pub(crate) async fn limit_reasons(
    client: &Client,
    db: &Path,
    date: &str,
    settled: bool,
) -> Result<(HashMap<String, String>, String), String> {
    if settled {
        if let Some(hit) = cache_load(db, date) {
            return Ok(hit);
        }
    }
    match ths_reasons(client, &date.replace('-', "")).await {
        Ok((map, note)) => {
            if settled {
                let _ = cache_store(db, date, &map, &note);
            }
            Ok((map, note))
        }
        Err(e) => Err(format!(
            "同花顺涨停揭秘:{}",
            emotion::error_message("涨停原因", &e)
                .trim_start_matches("涨停原因:")
                .chars()
                .take(120)
                .collect::<String>()
        )),
    }
}

// ---------------------------------------------------------------- 题材事件树(纯计算)

fn stat_boards(row: &Row) -> i64 {
    row.stat.map(|(_, count)| count).unwrap_or(0)
}

#[derive(Default)]
struct Group {
    limit_up: i64,
    first_boards: i64,
    consec_boards: i64,
    highest: i64,
    broken: i64,
    limit_down: i64,
    seal_times: Vec<String>,
    members: Vec<(i64, Value)>,
    prev_members: i64,
    prev_still_up: i64,
}

/// 题材状态标签。只由已发生的读数推出,不含对明天的判断
fn state_of(
    g: &Group,
    broken_rate: Option<f64>,
    continuation: Option<f64>,
    has_prev: bool,
    prev_complete: bool,
) -> &'static str {
    let br = broken_rate.unwrap_or(0.0);
    if !has_prev {
        "无昨日题材数据"
    } else if g.limit_up == 0 && g.prev_members > 0 {
        "昨日题材未再涨停"
    } else if g.prev_members == 0 {
        if prev_complete {
            "今日新出现"
        } else {
            "昨日覆盖样本未见"
        }
    } else if continuation.is_some_and(|c| c <= 0.15) && g.limit_up <= 2 {
        "接力断档"
    } else if br >= 0.5 {
        "分歧加大"
    } else if g.limit_up >= 4 && g.first_boards >= 2 {
        "扩散中"
    } else if g.highest >= 3 && g.limit_up <= 2 {
        "高标独活"
    } else if continuation.is_some_and(|c| c >= 0.5) {
        "延续"
    } else {
        "维持"
    }
}

pub(crate) struct TreeInput<'a> {
    pub zt: &'a [Row],
    pub zb: &'a [Row],
    pub dt: &'a [Row],
    pub reasons: &'a HashMap<String, String>,
    pub prev_zt: Option<&'a [Row]>,
    pub prev_reasons: &'a HashMap<String, String>,
    pub top: usize,
    pub note: &'a str,
}

fn hhmm(t: &str) -> Value {
    if t.len() == 6 {
        json!(format!("{}:{}", &t[0..2], &t[2..4]))
    } else {
        Value::Null
    }
}

pub(crate) fn build_tree(input: &TreeInput) -> Value {
    let zt_by_code: HashMap<&str, &Row> = input.zt.iter().map(|r| (r.code.as_str(), r)).collect();
    let zb_codes: HashSet<&str> = input.zb.iter().map(|r| r.code.as_str()).collect();
    let dt_codes: HashSet<&str> = input.dt.iter().map(|r| r.code.as_str()).collect();
    let prev_boards: HashMap<&str, i64> = input
        .prev_zt
        .map(|rows| {
            rows.iter()
                .map(|r| (r.code.as_str(), r.boards.max(1)))
                .collect()
        })
        .unwrap_or_default();
    let has_prev = !input.prev_reasons.is_empty() && !prev_boards.is_empty();
    let prev_covered = prev_boards
        .keys()
        .filter(|c| {
            !tags_of(
                input
                    .prev_reasons
                    .get(**c)
                    .map(String::as_str)
                    .unwrap_or(""),
            )
            .is_empty()
        })
        .count();
    let prev_complete = !prev_boards.is_empty() && prev_covered == prev_boards.len();

    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut matched = 0usize;
    let mut codes: Vec<&String> = input.reasons.keys().collect();
    codes.sort();
    for code in codes {
        let Some(row) = zt_by_code.get(code.as_str()) else {
            continue;
        };
        let tags = tags_of(&input.reasons[code]);
        if tags.is_empty() {
            continue;
        }
        matched += 1;
        let b = row.boards.max(1);
        for tag in tags {
            let g = groups.entry(tag).or_default();
            g.limit_up += 1;
            g.highest = g.highest.max(b);
            if b <= 1 {
                g.first_boards += 1
            } else {
                g.consec_boards += 1
            }
            if !row.first_seal.is_empty() {
                g.seal_times.push(row.first_seal.clone());
            }
            // 反包票(3天2板)东财连板数只给 1,排序按有效高度,别沉到榜尾
            g.members.push((b.max(stat_boards(row)), json!({
                "code": code, "name": row.name, "boards": b, "label": emotion::board_label(b, row.stat),
                "first_seal": hhmm(&row.first_seal), "broken_times": row.broken_times,
            })));
        }
    }
    // 昨日题材今天一只没涨停也保留,免得整条失去反馈的题材从表里消失
    for (code, reason) in input.prev_reasons {
        if prev_boards.contains_key(code.as_str()) {
            for tag in tags_of(reason) {
                groups.entry(tag).or_default();
            }
        }
    }
    // 炸板 / 跌停的票归到它们的题材(今天没涨停就没有今日串,用昨日串兜底)
    for (codes, is_broken) in [(&zb_codes, true), (&dt_codes, false)] {
        for code in codes.iter() {
            let reason = input
                .reasons
                .get(*code)
                .or_else(|| input.prev_reasons.get(*code))
                .map(String::as_str)
                .unwrap_or("");
            for tag in tags_of(reason) {
                if let Some(g) = groups.get_mut(&tag) {
                    if is_broken {
                        g.broken += 1
                    } else {
                        g.limit_down += 1
                    }
                }
            }
        }
    }
    // 延续:昨天这个题材里的涨停股,今天还在不在
    for (code, reason) in input.prev_reasons {
        if !prev_boards.contains_key(code.as_str()) {
            continue;
        }
        for tag in tags_of(reason) {
            if let Some(g) = groups.get_mut(&tag) {
                g.prev_members += 1;
                if zt_by_code.contains_key(code.as_str()) {
                    g.prev_still_up += 1;
                }
            }
        }
    }
    if matched == 0 && groups.values().all(|g| g.prev_members == 0) {
        return json!({"available": false, "reason": "涨停原因和当天涨停池没匹配出事件题材"});
    }

    let mut themes: Vec<(String, Value, i64, i64, bool)> = Vec::new();
    for (tag, mut g) in groups {
        g.seal_times.sort();
        let attempts = g.limit_up + g.broken;
        let broken_rate = (attempts > 0).then(|| emotion::r3(g.broken as f64 / attempts as f64));
        let continuation = (g.prev_members > 0)
            .then(|| emotion::r3(g.prev_still_up as f64 / g.prev_members as f64));
        let state = state_of(&g, broken_rate, continuation, has_prev, prev_complete);
        g.members.sort_by_key(|b| std::cmp::Reverse(b.0));
        let members: Vec<Value> = g.members.iter().take(8).map(|(_, v)| v.clone()).collect();
        themes.push((tag.clone(), json!({
            "tag": tag, "limit_up": g.limit_up, "first_boards": g.first_boards, "consec_boards": g.consec_boards, "highest": g.highest,
            "broken": g.broken, "limit_down": g.limit_down, "first_seal": g.seal_times.first().map(|t| hhmm(t)), "last_seal": g.seal_times.last().map(|t| hhmm(t)),
            "broken_rate": broken_rate, "prev_members": g.prev_members, "prev_still_up": g.prev_still_up, "continuation_rate": continuation,
            "state": state, "members": members,
        }), g.limit_up, g.highest, g.limit_up == 0 && g.prev_members > 0));
    }
    themes.sort_by(|a, b| b.2.cmp(&a.2).then(b.3.cmp(&a.3)).then(a.0.cmp(&b.0)));
    let top = input.top;
    let mut shown: Vec<Value> = themes.iter().take(top).map(|t| t.1.clone()).collect();
    shown.extend(
        themes
            .iter()
            .skip(top)
            .filter(|t| t.4)
            .take(6)
            .map(|t| t.1.clone()),
    );
    let total_zt = input.zt.len();
    let mut src = format!("昨日有效题材覆盖 {prev_covered}/{} 只,只对已覆盖的样本算延续。列出当天前 {top} 个题材,另补最多 6 个昨日有、今天没再涨停的题材。{}", prev_boards.len(), input.note);
    if !has_prev {
        src.push_str(" 昨日涨停池或昨日题材串没取到,不能判断题材是不是新出现。");
    }
    json!({
        "available": true, "source_note": src, "tag_count": themes.len(), "prev_covered": prev_covered, "prev_total": prev_boards.len(), "has_prev": has_prev,
        "themes": shown,
        // 头部集中度:第一题材占全市场涨停的比例
        "concentration": themes.first().filter(|_| total_zt > 0).map(|t| emotion::r3(t.2 as f64 / total_zt as f64)),
        // 覆盖率 = 对上涨停池的只数 / 涨停家数(直接用原因条数会出现「42/40」这种看着像 bug 的数)
        "covered": matched, "total_limit_up": total_zt, "coverage_rate": (total_zt > 0).then(|| emotion::r3(matched as f64 / total_zt as f64)),
    })
}

// ---------------------------------------------------------------- 入口

pub async fn fetch(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let top = crate::discovery::integer(path, "top", TOP_DEFAULT, 3, 30)?;
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, prev) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    let settled = emotion::is_settled(&date, &calendar, now);
    let compact = date.replace('-', "");
    let prev_compact = prev.as_ref().map(|p| p.replace('-', ""));
    let (zt, zb, dt, reasons, prev_zt) = futures_util::join!(
        emotion::fetch_pool(client, PoolKind::LimitUp, &compact),
        emotion::fetch_pool(client, PoolKind::Broken, &compact),
        emotion::fetch_pool(client, PoolKind::LimitDown, &compact),
        limit_reasons(client, db, &date, settled),
        async {
            match &prev_compact {
                Some(p) => Some(emotion::fetch_pool(client, PoolKind::LimitUp, p).await),
                None => None,
            }
        },
    );
    let mut body = json!({"date": date, "prev_date": prev, "settled": settled, "top": top});
    let zt = match zt {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => {
            body["tree"] = json!({"available": false, "reason": format!("{date} 涨停池是空的(非交易日或数据还没出)")});
            body["reasons"] = json!({});
            return Ok(body);
        }
        Err(e) => {
            body["tree"] =
                json!({"available": false, "reason": emotion::error_message("涨停池", &e)});
            body["reasons"] = json!({});
            return Ok(body);
        }
    };
    let (reasons, note) = match reasons {
        Ok(v) => v,
        Err(reason) => {
            body["tree"] = json!({"available": false, "reason": format!("涨停原因串没取到({reason})。不用行业分类顶替:行业不是题材")});
            body["reasons"] = json!({});
            return Ok(body);
        }
    };
    let (prev_reasons, prev_note) = match (&prev, &prev_zt) {
        (Some(p), Some(_)) => match limit_reasons(client, db, p, true).await {
            Ok((m, _)) => (m, String::new()),
            Err(e) => (HashMap::new(), format!(" 昨日题材串没取到({e})。")),
        },
        _ => (HashMap::new(), String::new()),
    };
    let prev_rows = prev_zt.and_then(Result::ok);
    let zb_rows = zb.unwrap_or_default();
    let dt_rows = dt.unwrap_or_default();
    let note = format!("{note}{prev_note}");
    let tree = build_tree(&TreeInput {
        zt: &zt,
        zb: &zb_rows,
        dt: &dt_rows,
        reasons: &reasons,
        prev_zt: prev_rows.as_deref(),
        prev_reasons: &prev_reasons,
        top,
        note: &note,
    });
    // 首板名单等页面按代码取原因:只给当天涨停池里的票
    let zt_codes: HashSet<&str> = zt.iter().map(|r| r.code.as_str()).collect();
    body["reasons"] = json!(reasons
        .iter()
        .filter(|(c, _)| zt_codes.contains(c.as_str()))
        .collect::<BTreeMap<_, _>>());
    body["reason_note"] = json!(note);
    body["tree"] = tree;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, name: &str, boards: i64, seal: &str, stat: Option<(i64, i64)>) -> Row {
        Row {
            code: code.into(),
            name: name.into(),
            boards,
            first_seal: seal.into(),
            stat,
            ..Row::default()
        }
    }

    fn reasons(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(c, r)| (c.to_string(), clean_reason(r)))
            .collect()
    }

    #[test]
    fn reasons_are_cleaned_by_whole_tags_and_generic_attributes_are_dropped() {
        assert_eq!(
            clean_reason("数据中心，干式变压器,电网设备 + 控制权变更预期+第五个标签"),
            "数据中心+干式变压器+电网设备+控制权变更预期"
        );
        let long = clean_reason("间接投资长鑫科技+百货零售+合肥国资+一个非常非常非常非常非常非常非常非常非常非常非常非常长的标签名");
        assert_eq!(
            long, "间接投资长鑫科技+百货零售+合肥国资",
            "超长时整个丢掉后面的标签,不截成半个词"
        );
        assert_eq!(
            tags_of("国企改革+兵装重组+合肥国资+破净+板块轮动板块"),
            vec!["兵装重组".to_string()]
        );
        assert!(tags_of("").is_empty());
    }

    fn ths_root(rows: Vec<Value>, total: usize) -> Value {
        json!({"status_code": 0, "data": {"info": rows, "page": {"total": total}}})
    }

    fn ths_row(code: &str, ts: i64, reason: Value) -> Value {
        json!({"code": code, "first_limit_up_time": ts, "reason_type": reason})
    }

    #[test]
    fn ths_pages_are_verified_row_by_row() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        // 2026-10-16 09:31:00 +08:00 = 01:31:00 UTC
        let ts = 1_792_114_260 - (1_792_114_260 % 86_400) + 31 * 60 + 60 * 60;
        let (mut seen, mut got) = (HashSet::new(), HashMap::new());
        let page = ths_root(
            vec![
                ths_row("600001", ts, json!("数据中心+电网设备")),
                ths_row("000002", ts, json!("-")),
                ths_row("300003", ts, Value::Null),
            ],
            3,
        );
        assert_eq!(
            parse_ths_page(&page, day, &mut seen, &mut got).unwrap(),
            (3, 3)
        );
        assert_eq!(got.len(), 1, "「-」「暂无」和空原因不算");
        // 别的交易日 / 缺时间 / 重复代码 / 非 0 状态都整页拒绝
        let (mut s2, mut g2) = (HashSet::new(), HashMap::new());
        assert!(parse_ths_page(
            &ths_root(vec![ths_row("600001", ts - 86_400 * 2, json!("x"))], 1),
            day,
            &mut s2,
            &mut g2
        )
        .unwrap_err()
        .to_string()
        .contains("别的交易日"));
        assert!(parse_ths_page(
            &ths_root(vec![json!({"code": "600009"})], 1),
            day,
            &mut HashSet::new(),
            &mut HashMap::new()
        )
        .unwrap_err()
        .to_string()
        .contains("封板时间"));
        let dup = ths_root(
            vec![
                ths_row("600001", ts, json!("a")),
                ths_row("600001", ts, json!("b")),
            ],
            2,
        );
        assert!(
            parse_ths_page(&dup, day, &mut HashSet::new(), &mut HashMap::new())
                .unwrap_err()
                .to_string()
                .contains("重复")
        );
        assert!(parse_ths_page(
            &json!({"status_code": 1}),
            day,
            &mut HashSet::new(),
            &mut HashMap::new()
        )
        .is_err());
    }

    #[test]
    fn theme_states_follow_the_desktop_rules() {
        let g = |limit_up, first, highest, prev, still: i64| Group {
            limit_up,
            first_boards: first,
            highest,
            prev_members: prev,
            prev_still_up: still,
            ..Group::default()
        };
        let cont = |g: &Group| {
            (g.prev_members > 0).then(|| g.prev_still_up as f64 / g.prev_members as f64)
        };
        let st = |g: &Group, br: Option<f64>, has: bool, complete: bool| {
            state_of(g, br, cont(g), has, complete)
        };
        assert_eq!(st(&g(3, 3, 1, 0, 0), None, true, true), "今日新出现");
        assert_eq!(st(&g(3, 3, 1, 0, 0), None, true, false), "昨日覆盖样本未见");
        assert_eq!(st(&g(0, 0, 0, 4, 0), None, true, true), "昨日题材未再涨停");
        assert_eq!(st(&g(2, 2, 1, 10, 1), None, true, true), "接力断档");
        assert_eq!(st(&g(6, 4, 2, 4, 4), Some(0.5), true, true), "分歧加大");
        assert_eq!(st(&g(6, 4, 2, 4, 3), Some(0.1), true, true), "扩散中");
        assert_eq!(st(&g(2, 0, 4, 2, 1), Some(0.0), true, true), "高标独活");
        assert_eq!(st(&g(3, 1, 2, 4, 2), Some(0.0), true, true), "延续");
        assert_eq!(st(&g(3, 1, 2, 10, 3), Some(0.0), true, true), "维持");
        assert_eq!(st(&g(3, 1, 2, 10, 3), None, false, true), "无昨日题材数据");
    }

    #[test]
    fn tree_groups_by_event_theme_and_measures_yesterdays_continuation() {
        let zt = vec![
            row("600001", "甲", 3, "093000", None),
            row("600002", "乙", 1, "094500", None),
            row("600003", "丙", 1, "100000", Some((3, 2))),
            row("600004", "丁", 1, "101500", None),
        ];
        let zb = vec![row("600009", "戊", 0, "", None)];
        let prev = vec![
            row("600001", "甲", 2, "", None),
            row("600005", "己", 1, "", None),
            row("600006", "庚", 1, "", None),
        ];
        let today_reasons = reasons(&[
            ("600001", "兵装重组+国企改革"),
            ("600002", "兵装重组"),
            ("600003", "数据中心"),
            ("600004", "国企改革"),
            ("600009", "兵装重组"),
            ("000999", "不在涨停池里"),
        ]);
        let prev_reasons = reasons(&[
            ("600001", "兵装重组"),
            ("600005", "兵装重组"),
            ("600006", "固态电池"),
        ]);
        let tree = build_tree(&TreeInput {
            zt: &zt,
            zb: &zb,
            dt: &[],
            reasons: &today_reasons,
            prev_zt: Some(&prev),
            prev_reasons: &prev_reasons,
            top: 10,
            note: "来源:测试",
        });
        assert_eq!(tree["available"], true);
        assert_eq!(
            (
                tree["covered"].clone(),
                tree["total_limit_up"].clone(),
                tree["coverage_rate"].clone()
            ),
            (json!(3), json!(4), json!(0.75))
        );
        let themes = tree["themes"].as_array().unwrap();
        let by = |tag: &str| themes.iter().find(|t| t["tag"] == tag).unwrap();
        let army = by("兵装重组");
        assert_eq!(
            (
                army["limit_up"].clone(),
                army["first_boards"].clone(),
                army["consec_boards"].clone(),
                army["highest"].clone(),
                army["broken"].clone()
            ),
            (json!(2), json!(1), json!(1), json!(3), json!(1))
        );
        assert_eq!(
            (
                army["prev_members"].clone(),
                army["prev_still_up"].clone(),
                army["continuation_rate"].clone()
            ),
            (json!(2), json!(1), json!(0.5))
        );
        assert_eq!(
            (
                army["first_seal"].clone(),
                army["last_seal"].clone(),
                army["broken_rate"].clone()
            ),
            (json!("09:30"), json!("09:45"), json!(0.333))
        );
        assert_eq!(
            army["state"], "高标独活",
            "最高 3 板且只有 2 只涨停:判断顺序里「高标独活」在「延续」之前(同 Python)"
        );
        assert_eq!(by("数据中心")["state"], "今日新出现");
        assert_eq!(
            by("数据中心")["members"][0]["label"],
            "3天2板",
            "反包票(东财连板数 1)显示「N天M板」"
        );
        assert_eq!(
            by("固态电池")["state"],
            "昨日题材未再涨停",
            "昨日有、今天没再涨停的题材保留"
        );
        assert!(
            themes.iter().all(|t| t["tag"] != "国企改革"),
            "纯属性词不当题材"
        );
        assert_eq!(tree["concentration"], 0.5);
        assert_eq!(tree["prev_covered"], 3);
    }

    #[test]
    fn tree_is_unavailable_when_nothing_matches_instead_of_falling_back() {
        let zt = vec![row("600001", "甲", 1, "093000", None)];
        let none = reasons(&[("000999", "兵装重组")]);
        let tree = build_tree(&TreeInput {
            zt: &zt,
            zb: &[],
            dt: &[],
            reasons: &none,
            prev_zt: None,
            prev_reasons: &HashMap::new(),
            top: 10,
            note: "",
        });
        assert_eq!(tree["available"], false);
        // 没有昨日数据:只能标「无昨日题材数据」,不判断新出现
        let some = reasons(&[("600001", "兵装重组")]);
        let tree = build_tree(&TreeInput {
            zt: &zt,
            zb: &[],
            dt: &[],
            reasons: &some,
            prev_zt: None,
            prev_reasons: &HashMap::new(),
            top: 10,
            note: "",
        });
        assert_eq!(tree["themes"][0]["state"], "无昨日题材数据");
        assert!(tree["source_note"]
            .as_str()
            .unwrap()
            .contains("不能判断题材是不是新出现"));
    }

    #[test]
    fn reasons_cache_round_trips_only_non_empty_maps() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-ths-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let map = reasons(&[("600001", "兵装重组")]);
        cache_store(&db, "2026-10-16", &map, "来源:测试").unwrap();
        assert_eq!(
            cache_load(&db, "2026-10-16"),
            Some((map, "来源:测试".to_string()))
        );
        cache_store(&db, "2026-10-15", &HashMap::new(), "空").unwrap();
        assert_eq!(cache_load(&db, "2026-10-15"), None, "空名单不当缓存命中");
        assert_eq!(cache_load(&db, "2026-10-14"), None);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 对真实上游:cargo test --locked theme_tree::tests::live_theme_tree_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_theme_tree_probe() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-theme-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let started = std::time::Instant::now();
        let r = app
            .handle(crate::MobileApiRequest {
                path: "/mobile/shortline/theme-tree".into(),
                method: "GET".into(),
                body: Value::Null,
            })
            .await;
        let t = &r.body["tree"];
        println!(
            "status {} in {:?}; date {} available {} {}",
            r.status,
            started.elapsed(),
            r.body["date"],
            t["available"],
            t["reason"]
        );
        println!(
            "covered {}/{} tags {} reasons {}",
            t["covered"],
            t["total_limit_up"],
            t["tag_count"],
            r.body["reasons"].as_object().map_or(0, |o| o.len())
        );
        for th in t["themes"].as_array().into_iter().flatten().take(6) {
            println!(
                "{} [{}] 涨停{} 最高{} 昨日{}→{}",
                th["tag"],
                th["state"],
                th["limit_up"],
                th["highest"],
                th["prev_members"],
                th["prev_still_up"]
            );
        }
        if let Ok(out) = std::env::var("VIBE_THEME_TREE_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&r.body).unwrap()).unwrap();
        }
        assert_eq!(r.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
