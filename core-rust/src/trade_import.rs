//! 从表格(CSV / 从表格软件复制来的内容)导入成交与分红(移动版;列名与口径对照 desktop 的 tradeCsv.ts)。
//!
//! 🔴 全部在本机解析,不发给任何 AI,也不联网。每一行都做和手动「记一笔」同一套检查;不对的行写原因、不导入,不猜不补。
//! 🔴 1–5 位纯数字代码一律拒:表格软件会把 000001 存成 1、00700 存成 700,分不清原来是 A 股还是港股。
//! 🔴 负数不取绝对值:有的券商导出卖出数量是负的,换算方向要用户自己确认。
//! 🔴 手机端的持仓是「交易记录(一轮)」:同一只票连续买入到卖光算一轮,卖光后再买是新的一轮。导入按日期重放切轮;
//!    同一天里先算买入再算卖出(券商导出常把同一天的顺序倒着写)。卖出超过此前买入(含手机里已记的?不含,只看这份表)的行
//!    写「卖出超过持有」、不导入;跳过一笔卖出会让之后的持有偏大,所以问题清单要摆在预览里。
//! 🔴 与已记的成交完全一样(同代码、同日、同方向、同价同量)的行标「已记过」、不重复导入;分红按同代码同日同金额判重。
//! 期初持仓按零费用的初始成本记录；送转拆股按明确的新股数/原股数比例结转，不产生现金流。
//! 🔴 费用列留空的成交按「设置」里的费率估(和手动记一笔一致),不当 0。

use crate::{journal, portfolio, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};

pub const MAX_ROWS: usize = 1000;
pub const MAX_BYTES: usize = 512 * 1024;
const EPS: f64 = 1e-6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Date,
    Kind,
    Symbol,
    Name,
    Shares,
    Price,
    Fee,
    Amount,
    Ratio,
    Note,
}

const FIELD_NAMES: [(Field, &str); 10] = [
    (Field::Date, "日期"), (Field::Kind, "类型"), (Field::Symbol, "代码"), (Field::Name, "名称"), (Field::Shares, "数量"),
    (Field::Price, "价格"), (Field::Fee, "费用"), (Field::Amount, "金额"), (Field::Ratio, "比例"), (Field::Note, "备注"),
];
const HEADER_ALIASES: [(Field, &[&str]); 10] = [
    (Field::Date, &["日期", "成交日期", "交易日期", "发生日期", "date"]),
    (Field::Kind, &["类型", "操作", "买卖", "买卖标志", "业务名称", "type"]),
    (Field::Symbol, &["代码", "证券代码", "股票代码", "symbol", "code"]),
    (Field::Name, &["名称", "证券名称", "股票名称", "name"]),
    (Field::Shares, &["数量", "成交数量", "股数", "shares"]),
    (Field::Price, &["价格", "成交价", "成交价格", "成交均价", "每股成本", "price"]),
    (Field::Fee, &["费用", "手续费", "佣金", "交易费用", "fee"]),
    (Field::Amount, &["金额", "到账金额", "分红金额", "发生金额", "amount"]),
    (Field::Ratio, &["比例", "送转比例", "ratio"]),
    (Field::Note, &["备注", "note"]),
];
const REQUIRED: [Field; 3] = [Field::Date, Field::Kind, Field::Symbol];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Buy,
    Sell,
    Dividend,
    Opening,
    Split,
}

const KIND_ALIASES: [(Kind, &[&str]); 5] = [
    (Kind::Buy, &["买入", "买", "证券买入", "buy"]),
    (Kind::Sell, &["卖出", "卖", "证券卖出", "sell"]),
    (Kind::Dividend, &["现金分红", "分红", "红利", "红利入账", "股息", "股息入账", "dividend"]),
    (Kind::Opening, &["期初持仓", "期初", "opening"]),
    (Kind::Split, &["送转", "拆股", "送转拆股", "送转/拆股", "合股", "split"]),
];

fn norm(text: &str) -> String {
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect();
    // 去掉结尾括号里的单位:「费用(元)」→「费用」
    let trimmed = match squeezed.rfind(['(', '（']) {
        Some(at) if squeezed.ends_with(')') || squeezed.ends_with('）') => squeezed[..at].to_string(),
        _ => squeezed,
    };
    trimmed.to_lowercase()
}

/// 一段表格文本 → 一行行格子(RFC 4180:引号里可以有分隔符、换行,"" 是一个引号)。每行带它开始的行号
pub(crate) fn split_table(text: &str, delimiter: char) -> Vec<(usize, Vec<String>)> {
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    let (mut cells, mut cell): (Vec<String>, String) = (Vec::new(), String::new());
    let (mut quoted, mut line, mut start) = (false, 1usize, 1usize);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quoted {
            if c == '"' && chars.get(i + 1) == Some(&'"') {
                cell.push('"');
                i += 1;
            } else if c == '"' {
                quoted = false;
            } else {
                if c == '\n' {
                    line += 1;
                }
                cell.push(c);
            }
        } else if c == '"' && cell.trim().is_empty() {
            quoted = true;
            cell.clear();
        } else if c == delimiter {
            cells.push(std::mem::take(&mut cell));
        } else if c == '\n' {
            cells.push(std::mem::take(&mut cell));
            out.push((start, std::mem::take(&mut cells)));
            line += 1;
            start = line;
        } else {
            cell.push(c);
        }
        i += 1;
    }
    if !cell.is_empty() || !cells.is_empty() {
        cells.push(cell);
        out.push((start, cells));
    }
    out
}

fn delimiter_of(text: &str) -> char {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let tabs = first.matches('\t').count();
    let commas = first.matches(',').count();
    if tabs > 0 && tabs >= commas { '\t' } else { ',' }
}

/// 日期:YYYY-MM-DD、YYYY/M/D、YYYY.M.D、YYYY年M月D日、YYYYMMDD;后面带时间的只取日期
pub(crate) fn normalize_date(raw: &str) -> String {
    let head = raw.trim().split(|c: char| c == 'T' || c.is_whitespace()).next().unwrap_or("");
    let digits_only = head.len() == 8 && head.bytes().all(|b| b.is_ascii_digit());
    if digits_only {
        return format!("{}-{}-{}", &head[0..4], &head[4..6], &head[6..8]);
    }
    let parts: Vec<&str> = head.trim_end_matches('日').split(['-', '/', '.', '年', '月']).collect();
    if parts.len() == 3 && parts[0].len() == 4 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) && parts[1].len() <= 2 && parts[2].len() <= 2 {
        return format!("{}-{:0>2}-{:0>2}", parts[0], parts[1], parts[2]);
    }
    raw.trim().to_string()
}

fn number(raw: &str) -> std::result::Result<Option<f64>, ()> {
    let text: String = raw.chars().filter(|c| !matches!(c, ',' | '，' | ' ' | '\u{3000}') && !c.is_whitespace()).collect();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or(())
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub line: usize,
    pub date: String,
    kind: Kind,
    pub kind_text: String,
    pub symbol: String,
    pub name: String,
    pub shares: Option<f64>,
    pub price: Option<f64>,
    pub fee: Option<f64>,
    pub amount: Option<f64>,
    pub problem: Option<String>,
    pub duplicate: bool,
}

pub(crate) fn parse_table(input: &str, today: &str) -> std::result::Result<Vec<Row>, String> {
    let text = input.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    if text.trim().is_empty() {
        return Err("没有内容:请粘贴表格,或选一个 CSV 文件".into());
    }
    let delimiter = delimiter_of(&text);
    let mut table: Vec<(usize, Vec<String>)> = split_table(&text, delimiter).into_iter().filter(|(_, cells)| cells.iter().any(|c| !c.trim().is_empty())).collect();
    if table.is_empty() {
        return Err("没有内容:请粘贴表格,或选一个 CSV 文件".into());
    }
    let (_, head) = table.remove(0);
    let mut col: HashMap<usize, usize> = HashMap::new(); // field index → column
    let field_ix = |f: Field| FIELD_NAMES.iter().position(|(x, _)| *x == f).unwrap();
    for (i, h) in head.iter().enumerate() {
        let key = norm(h);
        let found = HEADER_ALIASES.iter().find(|(_, names)| names.iter().any(|n| norm(n) == key)).map(|(f, _)| *f);
        let Some(f) = found else { continue };
        if let Some(prev) = col.insert(field_ix(f), i) {
            return Err(format!("有两列都是「{}」:第 {} 列和第 {} 列,请删掉一列", FIELD_NAMES[field_ix(f)].1, prev + 1, i + 1));
        }
    }
    let missing: Vec<&str> = REQUIRED.iter().filter(|f| !col.contains_key(&field_ix(**f))).map(|f| FIELD_NAMES[field_ix(*f)].1).collect();
    if !missing.is_empty() {
        let aliases: Vec<String> = REQUIRED
            .iter()
            .filter(|f| !col.contains_key(&field_ix(**f)))
            .map(|f| HEADER_ALIASES.iter().find(|(x, _)| x == f).unwrap().1.join(" / "))
            .collect();
        return Err(format!("第一行要是列名,至少要有{}这几列(认得的写法:{})", missing.iter().map(|m| format!("「{m}」")).collect::<String>(), aliases.join(";")));
    }
    if table.len() > MAX_ROWS {
        return Err(format!("一次最多导入 {MAX_ROWS} 行,这份有 {} 行:请分成几份", table.len()));
    }
    let get = |cells: &[String], f: Field| col.get(&field_ix(f)).and_then(|i| cells.get(*i)).map(|c| c.trim().to_string()).unwrap_or_default();
    let mut rows = Vec::with_capacity(table.len());
    for (line, cells) in table {
        let kind_text = get(&cells, Field::Kind);
        let kind_key = norm(&kind_text);
        let kind = KIND_ALIASES.iter().find(|(_, names)| names.iter().any(|n| norm(n) == kind_key)).map(|(k, _)| *k);
        let mut row = Row {
            line, date: normalize_date(&get(&cells, Field::Date)), kind: kind.unwrap_or(Kind::Buy), kind_text: kind_text.clone(), symbol: get(&cells, Field::Symbol), name: get(&cells, Field::Name),
            shares: None, price: None, fee: None, amount: None, problem: None, duplicate: false,
        };
        row.problem = check_row(&mut row, kind, [get(&cells, Field::Shares), get(&cells, Field::Price), get(&cells, Field::Fee), get(&cells, Field::Amount)], &get(&cells, Field::Ratio), today);
        rows.push(row);
    }
    Ok(rows)
}

fn check_row(row: &mut Row, kind: Option<Kind>, raw: [String; 4], ratio: &str, today: &str) -> Option<String> {
    let Some(kind) = kind else {
        return Some(format!("不认识的类型「{}」(认得:买入 / 卖出 / 现金分红)", row.kind_text));
    };
    if row.symbol.len() < 6 && !row.symbol.is_empty() && row.symbol.bytes().all(|b| b.is_ascii_digit()) {
        return Some(format!("代码「{}」只有 {} 位:表格软件会把 000001 存成 1,分不清原来是哪只,请把代码列改成文本格式", row.symbol, row.symbol.len()));
    }
    match normalize_stock_symbol(&row.symbol) {
        Ok(key) => row.symbol = key.digits,
        Err(error) => return Some(format!("代码不正确:{error}")),
    }
    match NaiveDate::parse_from_str(&row.date, "%Y-%m-%d") {
        Ok(_) if row.date.as_str() > today => return Some("日期不能是未来".into()),
        Ok(_) => {}
        Err(_) => return Some(format!("日期无效:{}", row.date)),
    }
    let [shares, price, fee, amount] = raw;
    let parse = |label: &str, text: &str| -> std::result::Result<Option<f64>, String> { number(text).map_err(|_| format!("{label}不是数字:{text}")) };
    let (shares, price, fee, amount) = match (parse("数量", &shares), parse("价格", &price), parse("费用", &fee), parse("金额", &amount)) {
        (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
        (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (_, _, _, Err(e)) => return Some(e),
    };
    if kind == Kind::Split {
        let ratio = number(ratio).ok().flatten();
        return match ratio.filter(|r|(0.001..=1000.0).contains(r)) {
            Some(r) => { row.price=Some(r); row.shares=Some(1.0); row.fee=Some(0.0); None },
            None => Some("送转拆股必须填比例(新股数/原股数，例如每十股转十股填 2；合股填 0.5)，范围 0.001–1000".into())
        };
    }
    match kind {
        Kind::Buy | Kind::Sell | Kind::Opening => {
            let (Some(s), Some(p)) = (shares, price) else { return Some("买卖要有数量和价格".into()) };
            if s <= 0.0 {
                return Some(if s < 0.0 { "数量是负数:有的券商导出卖出数量写负数,请自己改成正数并确认方向".into() } else { "数量要大于 0".into() });
            }
            if p <= 0.0 {
                return Some("价格要大于 0".into());
            }
            if fee.is_some_and(|f| f < 0.0) {
                return Some("费用不能是负数".into());
            }
            row.shares = Some(s);
            row.price = Some(p);
            row.fee = if kind == Kind::Opening {Some(0.0)}else{fee};
            None
        }
        Kind::Dividend => match amount {
            Some(a) if a > 0.0 => {
                row.amount = Some(a);
                None
            }
            Some(_) => Some("分红金额要大于 0(到手现金,元)".into()),
            None => Some("现金分红要有金额(到手现金,元)".into()),
        },
        Kind::Split => None,
    }
}

type ExistingFills = HashMap<String, Vec<(String, String, f64, f64)>>;

fn load_existing(conn: &Connection) -> Result<(ExistingFills, Vec<(String, String, f64)>)> {
    let mut stmt = conn.prepare("SELECT e.symbol, f.side, f.date, f.price, f.shares FROM journal_fills f JOIN journal_entries e ON e.id = f.entry_id")?;
    let mut fills: ExistingFills = HashMap::new();
    for r in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, f64>(3)?, r.get::<_, f64>(4)?)))? {
        let (symbol, side, date, price, shares) = r?;
        fills.entry(symbol).or_default().push((side, date, price, shares));
    }
    let dividends = portfolio::read_dividends(conn)?.into_iter().map(|d| (d.symbol, d.date, d.amount)).collect();
    Ok((fills, dividends))
}

fn mark_duplicates(rows: &mut [Row], existing: &ExistingFills, dividends: &[(String, String, f64)]) {
    for row in rows.iter_mut().filter(|r| r.problem.is_none()) {
        row.duplicate = match row.kind {
            Kind::Buy | Kind::Sell | Kind::Opening => {
                let side = match row.kind { Kind::Buy=>"buy",Kind::Opening=>"opening",_=>"sell" };
                existing.get(&row.symbol).is_some_and(|list| list.iter().any(|(s, d, p, n)| s == side && *d == row.date && (p - row.price.unwrap_or(0.0)).abs() < EPS && (n - row.shares.unwrap_or(0.0)).abs() < EPS))
            }
            Kind::Dividend => dividends.iter().any(|(s, d, a)| *s == row.symbol && *d == row.date && (a - row.amount.unwrap_or(0.0)).abs() < 0.005),
            Kind::Split => existing.get(&row.symbol).is_some_and(|list|list.iter().any(|(s,d,p,_)|s=="split" && *d==row.date && (p-row.price.unwrap_or(0.0)).abs()<EPS)),
        };
    }
}

#[derive(Debug)]
pub(crate) struct Episode {
    pub symbol: String,
    pub name: String,
    /// (side, date, price, shares, fee)
    pub fills: Vec<(String, String, f64, f64, Option<f64>)>,
    pub open_shares: f64,
}

/// 按代码、日期重放,把买卖切成一轮一轮;同日先买后卖。卖出超过此前买入的行写问题、不导入。
pub(crate) fn plan_episodes(rows: &mut [Row]) -> Vec<Episode> {
    let mut order: Vec<usize> = (0..rows.len()).filter(|i| rows[*i].problem.is_none() && !rows[*i].duplicate && matches!(rows[*i].kind, Kind::Buy | Kind::Sell | Kind::Opening | Kind::Split)).collect();
    order.sort_by(|a, b| {
        let (x, y) = (&rows[*a], &rows[*b]);
        x.symbol.cmp(&y.symbol).then(x.date.cmp(&y.date)).then((match x.kind {Kind::Split=>0,Kind::Opening=>1,Kind::Buy=>2,_=>3}).cmp(&(match y.kind {Kind::Split=>0,Kind::Opening=>1,Kind::Buy=>2,_=>3}))).then(x.line.cmp(&y.line))
    });
    let mut episodes: Vec<Episode> = Vec::new();
    let mut current: Option<Episode> = None;
    let mut position = 0.0_f64;
    for ix in order {
        if current.as_ref().is_some_and(|e| e.symbol != rows[ix].symbol) {
            episodes.extend(current.take());
            position = 0.0;
        }
        if rows[ix].kind == Kind::Split {
            if let Some(e) = current.as_mut().filter(|_|position>EPS) {
                let r=&rows[ix]; let ratio=r.price.unwrap();
                e.fills.push(("split".into(),r.date.clone(),ratio,1.0,Some(0.0)));
                position*=ratio; e.open_shares=position;
            } else { rows[ix].problem=Some("送转拆股此前没有持仓(只看本份表)".into()); }
            continue;
        }
        if rows[ix].kind == Kind::Opening && position>EPS {rows[ix].problem=Some("期初持仓之前已有持仓，不能重复建立期初".into());continue;}
        let side = if rows[ix].kind==Kind::Opening {"opening"}else{"buy"};
        let (symbol, name, date, price, shares, fee, is_buy) = {
            let r = &rows[ix];
            (r.symbol.clone(), r.name.clone(), r.date.clone(), r.price.unwrap_or(0.0), r.shares.unwrap_or(0.0), r.fee, matches!(r.kind,Kind::Buy|Kind::Opening))
        };
        if is_buy {
            let e = current.get_or_insert_with(|| Episode { symbol: symbol.clone(), name: name.clone(), fills: Vec::new(), open_shares: 0.0 });
            if e.name.is_empty() {
                e.name = name;
            }
            e.fills.push((side.into(), date, price, shares, fee));
            position += shares;
            e.open_shares = position;
        } else if position + EPS < shares {
            rows[ix].problem = Some(format!("卖出 {} 股超过此前买入的 {} 股(只看这份表里同一只票之前的买入)", fmt(shares), fmt(position.max(0.0))));
        } else if let Some(e) = current.as_mut() {
            e.fills.push(("sell".into(), date, price, shares, fee));
            position -= shares;
            e.open_shares = position.max(0.0);
            if position <= EPS {
                episodes.extend(current.take());
                position = 0.0;
            }
        }
    }
    episodes.extend(current.take());
    episodes
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < EPS { format!("{}", v.round() as i64) } else { format!("{v}") }
}

pub(crate) struct Plan {
    pub rows: Vec<Row>,
    pub episodes: Vec<Episode>,
    pub dividends: Vec<usize>,
}

pub(crate) fn plan(db: &Path, text: &str) -> Result<Plan, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("这份表格太大(超过 {} KB),请分成几份导入", MAX_BYTES / 1024));
    }
    let today = crate::emotion::now_china().0.format("%Y-%m-%d").to_string();
    let mut rows = parse_table(text, &today)?;
    let conn = Connection::open(db).context("open mobile sqlite").map_err(|e| e.to_string())?;
    let (existing, dividends) = load_existing(&conn).map_err(|e| e.to_string())?;
    mark_duplicates(&mut rows, &existing, &dividends);
    let episodes = plan_episodes(&mut rows);
    let mut seen: Vec<(String, String, f64)> = Vec::new();
    let mut dividend_rows = Vec::new();
    for (i, row) in rows.iter_mut().enumerate() {
        if row.kind == Kind::Dividend && row.problem.is_none() && !row.duplicate {
            let key = (row.symbol.clone(), row.date.clone(), row.amount.unwrap_or(0.0));
            if seen.iter().any(|(s, d, a)| *s == key.0 && *d == key.1 && (a - key.2).abs() < 0.005) {
                row.problem = Some("表里有同一天、同金额的重复分红,只导入第一行".into());
            } else {
                seen.push(key);
                dividend_rows.push(i);
            }
        }
    }
    Ok(Plan { rows, episodes, dividends: dividend_rows })
}

fn status_of(row: &Row) -> (&'static str, Option<&str>) {
    match (&row.problem, row.duplicate) {
        (Some(why), _) => ("problem", Some(why.as_str())),
        (None, true) => ("duplicate", Some("已记过(同代码、同日、同方向、同价同量),不重复导入")),
        _ => ("ok", None),
    }
}

fn summary(plan: &Plan) -> Value {
    let count = |s: &str| plan.rows.iter().filter(|r| status_of(r).0 == s).count();
    json!({
        "rows": plan.rows.len(), "importable": count("ok"), "duplicates": count("duplicate"), "problems": count("problem"),
        "episodes": plan.episodes.len(), "closed_episodes": plan.episodes.iter().filter(|e| e.open_shares <= EPS).count(),
        "open_episodes": plan.episodes.iter().filter(|e| e.open_shares > EPS).count(), "dividends": plan.dividends.len(),
    })
}

pub fn preview(db: &Path, body: &Value) -> Result<Value> {
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let plan = plan(db, text).map_err(|e| anyhow!(e))?;
    Ok(json!({
        "summary": summary(&plan),
        "rows": plan.rows.iter().map(|r| {
            let (status, why) = status_of(r);
            json!({"line": r.line, "date": r.date, "type": r.kind_text, "symbol": r.symbol, "name": r.name, "shares": r.shares, "price": r.price, "fee": r.fee, "amount": r.amount, "status": status, "why": why})
        }).collect::<Vec<_>>(),
        "episodes": plan.episodes.iter().map(|e| json!({"symbol": e.symbol, "name": e.name, "fills": e.fills.len(), "open_shares": e.open_shares, "closed": e.open_shares <= EPS})).collect::<Vec<_>>(),
        "note": "按日期重放切成「一轮一轮」的交易:同一只票连续买入到卖光算一轮;同一天先算买入再算卖出;只看这份表,不接在手机里已记的交易后面。费用留空的按设置里的费率估。导入前会再检查一遍;导入后每一轮都能在交易日志里单独删除。",
    }))
}

/// 导入:重新按同一份文本算一遍计划(不信任预览之后的任何状态),在一个事务里写入全部可导入的轮次与分红
pub fn import(db: &Path, body: &Value) -> Result<Value> {
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let plan = plan(db, text).map_err(|e| anyhow!(e))?;
    if plan.episodes.is_empty() && plan.dividends.is_empty() {
        return Err(anyhow!("没有可以导入的行(都是问题行或已记过的)"));
    }
    let mut conn = Connection::open(db).context("open mobile sqlite")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let tx = conn.transaction()?;
    let mut ids = Vec::new();
    for e in &plan.episodes {
        let fills: Vec<Value> = e.fills.iter().map(|(side, date, price, shares, fee)| json!({"type": side, "date": date, "price": price, "shares": shares, "fee": fee})).collect();
        let entry = json!({"symbol": e.symbol, "name": e.name, "note": "表格导入", "fills": fills});
        ids.push(journal::insert_record_tx(&tx, &entry).with_context(|| format!("导入 {} 失败", e.symbol))?);
    }
    let existing_dividends: i64 = tx.query_row("SELECT COUNT(*) FROM mobile_dividends", [], |r| r.get(0))?;
    if existing_dividends as usize + plan.dividends.len() > portfolio::DIVIDENDS_MAX {
        return Err(anyhow!("分红最多 {} 条,导入后会超出", portfolio::DIVIDENDS_MAX));
    }
    let now = Utc::now().to_rfc3339();
    for ix in &plan.dividends {
        let r = &plan.rows[*ix];
        tx.execute(
            "INSERT INTO mobile_dividends(symbol, date, amount, note, created_at, updated_at) VALUES (?1, ?2, ?3, '表格导入', ?4, ?4)",
            params![r.symbol, r.date, r.amount.unwrap_or(0.0), now],
        )?;
    }
    tx.commit()?;
    Ok(json!({
        "imported_entries": ids.len(), "entry_ids": ids, "imported_fills": plan.episodes.iter().map(|e| e.fills.len()).sum::<usize>(),
        "imported_dividends": plan.dividends.len(), "summary": summary(&plan),
        "skipped": plan.rows.iter().filter(|r| status_of(r).0 != "ok").map(|r| json!({"line": r.line, "why": status_of(r).1})).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-10-17";

    fn db(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-import-{name}-{nonce}.sqlite3"));
        crate::storage::init(&path).unwrap();
        crate::journal::init(&path).unwrap();
        crate::daily_review::init(&path).unwrap();
        crate::ai_chat::init(&path).unwrap();
        crate::ai_config::init(&path).unwrap();
        crate::schema::migrate(&path).unwrap();
        path
    }

    #[test]
    fn headers_dates_numbers_and_quoted_cells_parse_like_the_desktop_importer() {
        assert_eq!(normalize_date("2026/3/5"), "2026-03-05");
        assert_eq!(normalize_date("2026.03.05 09:31:00"), "2026-03-05");
        assert_eq!(normalize_date("20260305"), "2026-03-05");
        assert_eq!(normalize_date("2026年3月5日"), "2026-03-05");
        assert_eq!(normalize_date("明天"), "明天");
        assert_eq!(number("1,234.50"), Ok(Some(1234.5)));
        assert_eq!(number("  "), Ok(None));
        assert!(number("12a").is_err());
        let cells = split_table("a,\"b,c\",\"d\"\"e\"\n1,2,3", ',');
        assert_eq!(cells[0].1, vec!["a", "b,c", "d\"e"]);
        assert_eq!(cells[1], (2, vec!["1".into(), "2".into(), "3".into()]));
        let text = "成交日期\t买卖标志\t证券代码\t证券名称\t成交数量\t成交均价\t手续费\n2026/3/5\t证券买入\t600519\t贵州茅台\t100\t1,450.00\t5\n";
        let rows = parse_table(text, TODAY).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].problem.is_none());
        assert_eq!((rows[0].symbol.as_str(), rows[0].shares, rows[0].price, rows[0].fee, rows[0].date.as_str()), ("600519", Some(100.0), Some(1450.0), Some(5.0), "2026-03-05"));
    }

    #[test]
    fn header_errors_and_row_problems_are_explicit() {
        assert!(parse_table("  ", TODAY).unwrap_err().contains("没有内容"));
        assert!(parse_table("日期,类型\n2026-01-01,买入", TODAY).unwrap_err().contains("「代码」"));
        assert!(parse_table("日期,代码,代码,类型\n", TODAY).unwrap_err().contains("两列都是「代码」"));
        let big = format!("日期,类型,代码\n{}", "2026-01-01,买入,600519\n".repeat(MAX_ROWS + 1));
        assert!(parse_table(&big, TODAY).unwrap_err().contains("最多导入"));
        let csv = "日期,类型,代码,数量,价格,费用,金额\n\
                   2026-01-05,买入,1,100,10,,\n\
                   2026-01-05,买入,600519,-100,10,,\n\
                   2026-01-05,买入,600519,100,0,,\n\
                   2026-01-05,持有,600519,100,10,,\n\
                   2026-01-05,送转,600519,,,,\n\
                   2099-01-05,买入,600519,100,10,,\n\
                   2026-13-45,买入,600519,100,10,,\n\
                   2026-01-05,买入,600519,abc,10,,\n\
                   2026-01-05,现金分红,600519,,,,0\n\
                   2026-01-05,买入,600519,100,10,-1,\n\
                   2026-01-05,买入,12345x,100,10,,\n";
        let rows = parse_table(csv, TODAY).unwrap();
        let why: Vec<String> = rows.iter().map(|r| r.problem.clone().unwrap_or_default()).collect();
        assert!(why[0].contains("只有 1 位"), "{}", why[0]);
        assert!(why[1].contains("数量是负数"));
        assert!(why[2].contains("价格要大于 0"));
        assert!(why[3].contains("不认识的类型"));
        assert!(why[4].contains("必须填比例"));
        assert!(why[5].contains("未来"));
        assert!(why[6].contains("日期无效"));
        assert!(why[7].contains("数量不是数字"));
        assert!(why[8].contains("金额要大于 0"));
        assert!(why[9].contains("费用不能是负数"));
        assert!(why[10].contains("代码不正确"));
    }

    #[test]
    fn episodes_split_at_flat_same_day_buys_come_first_and_oversells_are_reported() {
        let csv = "日期,类型,代码,名称,数量,价格\n\
                   2026-02-02,卖出,600519,贵州茅台,100,1500\n\
                   2026-02-02,买入,600519,贵州茅台,100,1450\n\
                   2026-02-10,买入,600519,贵州茅台,50,1480\n\
                   2026-02-11,卖出,600519,贵州茅台,80,1490\n\
                   2026-02-12,卖出,600519,贵州茅台,500,1495\n\
                   2026-03-01,买入,000001,平安银行,1000,10\n";
        let mut rows = parse_table(csv, TODAY).unwrap();
        let episodes = plan_episodes(&mut rows);
        assert_eq!(episodes.len(), 3, "茅台 02-02 一轮(买卖同日、买先卖后)、02-10 起又一轮,平安银行一轮");
        let mt: Vec<&Episode> = episodes.iter().filter(|e| e.symbol == "600519").collect();
        assert_eq!(mt.len(), 2);
        assert_eq!(mt[0].fills.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["buy", "sell"]);
        assert!(mt[0].open_shares.abs() < 1e-9, "第一轮卖光");
        assert_eq!((mt[1].fills.len(), mt[1].open_shares), (1, 50.0), "第二轮只有那笔 50 股买入:后面两笔卖出(80、500)都超过了在手的 50");
        let problems: Vec<(usize, String)> = rows.iter().filter_map(|r| r.problem.clone().map(|p| (r.line, p))).collect();
        assert_eq!(problems.iter().map(|p| p.0).collect::<Vec<_>>(), [5, 6]);
        assert!(problems[0].1.contains("卖出 80 股超过此前买入的 50 股"), "{}", problems[0].1);
        assert!(problems[1].1.contains("卖出 500 股"));
    }

    #[test]
    fn preview_marks_duplicates_and_import_is_one_transaction_that_rerun_does_not_double() {
        let path = db("run");
        // 手机里已记过一笔
        crate::journal::act(&path, &json!({"action": "record", "symbol": "600519", "name": "贵州茅台", "fills": [{"type": "buy", "date": "2026-02-02", "price": 1450.0, "shares": 100.0}]})).unwrap();
        portfolio::act(&path, &json!({"action": "save", "symbol": "600519", "date": "2026-06-01", "amount": 300.5})).unwrap();
        let csv = "日期,类型,代码,名称,数量,价格,费用,金额
                   2026-02-02,买入,600519,贵州茅台,100,1450,,
                   2026-03-02,买入,000001,平安银行,1000,10,5,
                   2026-03-09,卖出,000001,平安银行,1000,11,7.5,
                   2026-04-01,买入,300308,中际旭创,200,50,,
                   2026-06-01,现金分红,600519,,,,,300.5
                   2026-07-01,现金分红,000001,,,,,88
                   2026-07-01,现金分红,000001,,,,,88
                   2026-05-01,卖出,300308,中际旭创,999,60,,
";
        let pv = preview(&path, &json!({"text": csv})).unwrap();
        assert_eq!(pv["summary"]["rows"], 8);
        assert_eq!((pv["summary"]["importable"].clone(), pv["summary"]["duplicates"].clone(), pv["summary"]["problems"].clone()), (json!(4), json!(2), json!(2)), "{}", pv["summary"]);
        assert_eq!(pv["rows"][0]["status"], "duplicate");
        assert_eq!(pv["rows"][4]["status"], "duplicate", "分红同代码同日同金额已记过");
        assert_eq!(pv["rows"][6]["status"], "problem", "表里重复的分红只导第一行");
        assert!(pv["rows"][7]["why"].as_str().unwrap().contains("超过此前买入"));

        let done = import(&path, &json!({"text": csv})).unwrap();
        assert_eq!((done["imported_entries"].clone(), done["imported_fills"].clone(), done["imported_dividends"].clone()), (json!(2), json!(3), json!(1)));
        let v = crate::journal::view(&path).unwrap();
        assert_eq!(v["rows"].as_array().map(|a| a.len()).unwrap_or(0), 3, "原有 1 + 导入 2 轮");
        let sum = portfolio::view(&path).unwrap();
        assert_eq!(sum["totals"]["dividends"], json!(388.5), "300.5 + 新导入的 88");
        // 再导入同一份:全部已记过 / 是问题行,没有可导入的
        let again = import(&path, &json!({"text": csv})).unwrap_err().to_string();
        assert!(again.contains("没有可以导入的行"), "{again}");
        assert_eq!(crate::journal::view(&path).unwrap()["rows"].as_array().unwrap().len(), 3);
        let _ = std::fs::remove_file(path);
    }
}
