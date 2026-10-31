//! 交易日志设置:费率、风险规矩、账户规模(移植自 orchestrator/src/finance/journal/{defs,fees}.ts)。
//!
//! 🔴 默认值是**能跑起来的初值,不是推荐值,也不是用户的真实费率 / 规矩**:各家券商佣金差得远,资金量、打法不同,合理阈值差得远。
//!    页面标「默认」,用户改了才算他自己的。
//! 🔴 设置坏了就明说,不退回默认值:退回默认等于悄悄换成一套更宽松(或更严)的口径,用户却以为是自己定的。
//!    费率四项要么全填要么全不填;缺项不会用初值补。
//! 设置是用户自己的口径,进备份。

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fees {
    pub commission_rate: f64,
    pub commission_min: f64,
    pub stamp_tax_rate: f64,
    pub transfer_fee_rate: f64,
    /// 成本法:true = 先进先出,false = 移动加权平均(默认)。放在这里是因为结算函数本来就拿着 Fees
    pub fifo: bool,
}

pub(crate) const DEFAULT_FEES: Fees = Fees {
    commission_rate: 0.00025,
    commission_min: 5.0,
    stamp_tax_rate: 0.0005,
    transfer_fee_rate: 0.00001,
    fifo: false,
};
const FEE_KEYS: [(&str, &str); 4] = [
    ("commission_rate", "佣金费率(双向)"),
    ("commission_min", "单笔佣金最低(元)"),
    ("stamp_tax_rate", "印花税(仅卖出)"),
    ("transfer_fee_rate", "过户费(双向)"),
];
/// 成本法的两个取值:移动加权平均(默认)和先进先出
pub(crate) const COST_METHODS: [&str; 2] = ["average", "fifo"];
/// 费率上限:万 2.5 应填 0.00025,填成 2.5 这种一看就不是费率
const MAX_RATE: f64 = 0.01;

impl Fees {
    /// 一笔成交按费率估的费用(元)。填了实际费用的不走这里
    pub(crate) fn estimate(&self, side: &str, price: f64, shares: f64) -> f64 {
        let amount = price * shares;
        let mut fee = (amount * self.commission_rate).max(self.commission_min);
        fee += amount * self.transfer_fee_rate;
        if side == "sell" {
            fee += amount * self.stamp_tax_rate;
        }
        fee
    }

    fn to_value(self) -> Value {
        json!({"commission_rate": self.commission_rate, "commission_min": self.commission_min, "stamp_tax_rate": self.stamp_tax_rate, "transfer_fee_rate": self.transfer_fee_rate})
    }
}

pub(crate) const RULE_KEYS: [(&str, &str, f64); 6] = [
    ("max_loss_per_trade_pct", "单笔最大亏损", 5.0),
    ("max_loss_per_day_pct", "单日最大亏损", 8.0),
    ("max_positions", "最多同时持仓", 3.0),
    ("max_trades_per_day", "单日最多开仓", 5.0),
    ("pause_after_losses", "连亏几笔后停手", 3.0),
    ("max_unplanned_ratio", "计划外交易占比上限", 0.2),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rules {
    pub max_loss_per_trade_pct: f64,
    pub max_loss_per_day_pct: f64,
    pub max_positions: f64,
    pub max_trades_per_day: f64,
    pub pause_after_losses: f64,
    pub max_unplanned_ratio: f64,
}

impl Rules {
    fn get(&self, key: &str) -> f64 {
        match key {
            "max_loss_per_trade_pct" => self.max_loss_per_trade_pct,
            "max_loss_per_day_pct" => self.max_loss_per_day_pct,
            "max_positions" => self.max_positions,
            "max_trades_per_day" => self.max_trades_per_day,
            "pause_after_losses" => self.pause_after_losses,
            _ => self.max_unplanned_ratio,
        }
    }

    fn from_saved(saved: &Map<String, Value>) -> Self {
        let pick = |key: &str| {
            saved.get(key).and_then(Value::as_f64).unwrap_or_else(|| {
                RULE_KEYS
                    .iter()
                    .find(|(k, _, _)| *k == key)
                    .map(|(_, _, d)| *d)
                    .unwrap_or(0.0)
            })
        };
        Self {
            max_loss_per_trade_pct: pick("max_loss_per_trade_pct"),
            max_loss_per_day_pct: pick("max_loss_per_day_pct"),
            max_positions: pick("max_positions"),
            max_trades_per_day: pick("max_trades_per_day"),
            pause_after_losses: pick("pause_after_losses"),
            max_unplanned_ratio: pick("max_unplanned_ratio"),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        Value::Object(
            RULE_KEYS
                .iter()
                .map(|(k, _, _)| (k.to_string(), json!(self.get(k))))
                .collect(),
        )
    }
}

/// 百分比 / 占比类可以带小数,其余是笔数、只数,要整数
fn is_fraction_rule(key: &str) -> bool {
    key.ends_with("_pct") || key.ends_with("_ratio")
}

fn rule_label(key: &str) -> &'static str {
    RULE_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, l, _)| *l)
        .unwrap_or("")
}

/// 一条规矩的值哪里不对;没问题返回 None
pub(crate) fn rule_problem(key: &str, value: &Value) -> Option<String> {
    let label = rule_label(key);
    let Some(v) = value.as_f64().filter(|v| v.is_finite()) else {
        return Some(format!("{label} 要是数字"));
    };
    if v < 0.0 || (v == 0.0 && key != "max_unplanned_ratio") {
        return Some(format!("{label} 要是正数"));
    }
    if key.ends_with("_ratio") && v > 1.0 {
        return Some("计划外交易占比要在 0 到 1 之间".into());
    }
    if !is_fraction_rule(key) && v.fract() != 0.0 {
        return Some(format!("{label} 要是整数"));
    }
    None
}

/// 一组费率哪里不对;没问题返回 None。缺项不用初值补
pub(crate) fn fee_problem(raw: &Map<String, Value>) -> Option<String> {
    for (key, label) in FEE_KEYS {
        match raw.get(key) {
            None | Some(Value::Null) => {
                return Some(format!("费率要四项都填(缺「{label}」),缺项不会自动用初值"))
            }
            Some(v) => {
                let Some(n) = v.as_f64().filter(|n| n.is_finite()) else {
                    return Some(format!("{label} 要是数字"));
                };
                if n < 0.0 {
                    return Some(format!("{label} 不能是负数"));
                }
                if key.ends_with("_rate") && n > MAX_RATE {
                    return Some(format!("{label} 看起来不像费率({n}),万 2.5 应填 0.00025"));
                }
            }
        }
    }
    None
}

#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub fees: Fees,
    pub fees_default: bool,
    pub rules: Rules,
    pub rules_default: bool,
    /// 用户填的账户规模;None = 没填,所有「占账户的比例」都不算
    pub equity_base: Option<f64>,
    pub updated_at: Option<String>,
}

impl Settings {
    pub(crate) fn cost_method(&self) -> &'static str {
        if self.fees.fifo {
            "fifo"
        } else {
            "average"
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fees: DEFAULT_FEES,
            fees_default: true,
            rules: Rules::from_saved(&Map::new()),
            rules_default: true,
            equity_base: None,
            updated_at: None,
        }
    }
}

/// 备份里的一行(和表一一对应)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Row {
    pub fees: Option<Map<String, Value>>,
    pub rules: Option<Map<String, Value>>,
    pub equity_base: Option<f64>,
    /// 成本法 'average' / 'fifo';旧备份和没设过的是 None = 移动加权平均
    #[serde(default)]
    pub cost_method: Option<String>,
    pub updated_at: String,
}

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile journal settings sqlite")
}

pub(crate) fn read_row(conn: &Connection) -> Result<Option<Row>> {
    type Raw = (
        Option<String>,
        Option<String>,
        Option<f64>,
        Option<String>,
        String,
    );
    let raw: Option<Raw> = conn
        .query_row("SELECT fees, rules, equity_base, cost_method, updated_at FROM mobile_journal_settings WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .optional()?;
    let Some((fees, rules, equity_base, cost_method, updated_at)) = raw else {
        return Ok(None);
    };
    let parse = |text: Option<String>| {
        text.and_then(|t| serde_json::from_str::<Map<String, Value>>(&t).ok())
    };
    Ok(Some(Row {
        fees: parse(fees),
        rules: parse(rules),
        equity_base,
        cost_method,
        updated_at,
    }))
}

pub(crate) fn write_row(conn: &Connection, row: &Row) -> Result<()> {
    conn.execute(
        "INSERT INTO mobile_journal_settings(id, fees, rules, equity_base, cost_method, updated_at) VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET fees = excluded.fees, rules = excluded.rules, equity_base = excluded.equity_base, cost_method = excluded.cost_method, updated_at = excluded.updated_at",
        params![
            row.fees.as_ref().map(|m| Value::Object(m.clone()).to_string()),
            row.rules.as_ref().map(|m| Value::Object(m.clone()).to_string()),
            row.equity_base,
            row.cost_method,
            row.updated_at
        ],
    )?;
    Ok(())
}

/// 备份 / 保存共用的检查:返回第一个问题
pub(crate) fn row_problem(row: &Row) -> Option<String> {
    if let Some(fees) = &row.fees {
        if let Some(p) = fee_problem(fees) {
            return Some(p);
        }
    }
    if let Some(rules) = &row.rules {
        for (key, value) in rules {
            if !RULE_KEYS.iter().any(|(k, _, _)| k == key) {
                return Some(format!("不认识的规矩 {key:?}"));
            }
            if let Some(p) = rule_problem(key, value) {
                return Some(p);
            }
        }
    }
    if row.equity_base.is_some_and(|v| !v.is_finite() || v <= 0.0) {
        return Some("账户规模要是大于 0 的数字(元)".into());
    }
    if let Some(method) = &row.cost_method {
        if !COST_METHODS.contains(&method.as_str()) {
            return Some(format!("不认识的成本法 {method:?}(只有 average / fifo)"));
        }
    }
    None
}

pub(crate) fn settings_of(row: Option<&Row>) -> Settings {
    let Some(row) = row else {
        return Settings::default();
    };
    let fifo = row.cost_method.as_deref() == Some("fifo");
    let fees = row
        .fees
        .as_ref()
        .filter(|m| fee_problem(m).is_none())
        .map(|m| Fees {
            commission_rate: m["commission_rate"].as_f64().unwrap_or(0.0),
            commission_min: m["commission_min"].as_f64().unwrap_or(0.0),
            stamp_tax_rate: m["stamp_tax_rate"].as_f64().unwrap_or(0.0),
            transfer_fee_rate: m["transfer_fee_rate"].as_f64().unwrap_or(0.0),
            fifo: false,
        });
    let saved_rules: Map<String, Value> = row.rules.clone().unwrap_or_default();
    Settings {
        fees_default: fees.is_none(),
        fees: Fees {
            fifo,
            ..fees.unwrap_or(DEFAULT_FEES)
        },
        rules_default: saved_rules.is_empty(),
        rules: Rules::from_saved(&saved_rules),
        equity_base: row.equity_base.filter(|v| *v > 0.0),
        updated_at: Some(row.updated_at.clone()),
    }
}

pub(crate) fn load(db: &Path) -> Result<Settings> {
    Ok(settings_of(read_row(&open(db)?)?.as_ref()))
}

pub fn view(db: &Path) -> Result<Value> {
    let s = load(db)?;
    Ok(settings_value(&s))
}

pub(crate) fn settings_value(s: &Settings) -> Value {
    json!({
        "fees": s.fees.to_value(), "fees_default": s.fees_default,
        "rules": s.rules.to_value(), "rules_default": s.rules_default,
        "equity_base": s.equity_base, "updated_at": s.updated_at,
        "cost_method": s.cost_method(),
        "cost_methods": [{"value": "average", "label": "移动加权平均"}, {"value": "fifo", "label": "先进先出"}],
        "labels": {
            "fees": FEE_KEYS.iter().map(|(k, l)| (k.to_string(), json!(l))).collect::<Map<_, _>>(),
            "rules": RULE_KEYS.iter().map(|(k, l, _)| (k.to_string(), json!(l))).collect::<Map<_, _>>(),
        },
        "defaults": { "fees": DEFAULT_FEES.to_value(), "rules": Settings::default().rules.to_value() },
        "fraction_rules": RULE_KEYS.iter().filter(|(k, _, _)| is_fraction_rule(k)).map(|(k, _, _)| *k).collect::<Vec<_>>(),
        "note": "默认值只是能跑起来的初值,不是推荐值,也不是你的真实费率或规矩;费率四项要么全填要么全不填。",
    })
}

/// 改设置:没传的保持原样;`fees: null` = 改回默认(四项一起);规矩里某项 null = 改回默认;`equity_base: null` = 清掉
pub fn save(db: &Path, body: &Value) -> Result<Value> {
    let conn = open(db)?;
    let current = read_row(&conn)?;
    let mut row = current.unwrap_or(Row {
        fees: None,
        rules: None,
        equity_base: None,
        cost_method: None,
        updated_at: String::new(),
    });
    if let Some(fees) = body.get("fees") {
        row.fees = match fees {
            Value::Null => None,
            Value::Object(m) => {
                if let Some(p) = fee_problem(m) {
                    return Err(anyhow!(p));
                }
                Some(
                    FEE_KEYS
                        .iter()
                        .map(|(k, _)| (k.to_string(), m[*k].clone()))
                        .collect(),
                )
            }
            _ => return Err(anyhow!("费率格式不对")),
        };
    }
    if let Some(rules) = body.get("rules") {
        let Value::Object(incoming) = rules else {
            return Err(anyhow!("规矩格式不对"));
        };
        let mut merged = row.rules.take().unwrap_or_default();
        for (key, value) in incoming {
            if !RULE_KEYS.iter().any(|(k, _, _)| k == key) {
                return Err(anyhow!("不认识的规矩 {key:?}"));
            }
            if value.is_null() {
                merged.remove(key);
                continue;
            }
            if let Some(p) = rule_problem(key, value) {
                return Err(anyhow!(p));
            }
            merged.insert(key.clone(), value.clone());
        }
        row.rules = (!merged.is_empty()).then_some(merged);
    }
    if let Some(base) = body.get("equity_base") {
        row.equity_base = match base {
            Value::Null => None,
            Value::Number(n) => {
                let v = n.as_f64().unwrap_or(0.0);
                if !v.is_finite() || v <= 0.0 {
                    return Err(anyhow!("账户规模要是大于 0 的数字(元)"));
                }
                Some(v)
            }
            _ => return Err(anyhow!("账户规模要是大于 0 的数字(元)")),
        };
    }
    if let Some(method) = body.get("cost_method") {
        row.cost_method = match method {
            Value::Null => None,
            Value::String(m) if m == "average" => None,
            Value::String(m) if COST_METHODS.contains(&m.as_str()) => Some(m.clone()),
            _ => {
                return Err(anyhow!(
                    "成本法只能是 average(移动加权平均)或 fifo(先进先出)"
                ))
            }
        };
    }
    row.updated_at = Utc::now().to_rfc3339();
    write_row(&conn, &row)?;
    view(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn cost_method_defaults_to_average_and_round_trips() {
        assert!(!Settings::default().fees.fifo && Settings::default().cost_method() == "average");
        let row = Row {
            fees: None,
            rules: None,
            equity_base: None,
            cost_method: Some("fifo".into()),
            updated_at: "t".into(),
        };
        assert!(
            settings_of(Some(&row)).fees.fifo,
            "没改过费率也能单独选成本法"
        );
        assert_eq!(
            settings_value(&settings_of(Some(&row)))["cost_method"],
            "fifo"
        );
        assert!(row_problem(&Row {
            cost_method: Some("lifo".into()),
            ..row.clone()
        })
        .unwrap()
        .contains("成本法"));
        assert_eq!(row_problem(&row), None);
    }

    #[test]
    fn fee_estimate_matches_desktop() {
        let f = DEFAULT_FEES;
        assert!(
            (f.estimate("buy", 10.0, 100.0) - 5.01).abs() < 1e-9,
            "最低佣金 5 元 + 过户费"
        );
        assert!(
            (f.estimate("sell", 10.0, 100_000.0) - (250.0 + 10.0 + 500.0)).abs() < 1e-6,
            "卖出另收印花税"
        );
    }

    #[test]
    fn fee_and_rule_problems_are_explicit() {
        assert!(fee_problem(&map(json!({"commission_rate": 0.0003})))
            .unwrap()
            .contains("四项都填"));
        assert!(fee_problem(&map(json!({"commission_rate": 2.5, "commission_min": 5, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001}))).unwrap().contains("不像费率"));
        assert!(fee_problem(&map(json!({"commission_rate": 0.0003, "commission_min": -1, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001}))).unwrap().contains("不能是负数"));
        assert_eq!(
            fee_problem(&map(
                json!({"commission_rate": 0.0003, "commission_min": 0, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001})
            )),
            None
        );
        assert!(rule_problem("max_positions", &json!(2.5))
            .unwrap()
            .contains("整数"));
        assert!(rule_problem("max_loss_per_trade_pct", &json!(0))
            .unwrap()
            .contains("正数"));
        assert_eq!(
            rule_problem("max_unplanned_ratio", &json!(0)),
            None,
            "占比可以是 0"
        );
        assert!(rule_problem("max_unplanned_ratio", &json!(1.5))
            .unwrap()
            .contains("0 到 1"));
        assert!(rule_problem("max_trades_per_day", &json!("3"))
            .unwrap()
            .contains("数字"));
    }

    #[test]
    fn save_merges_resets_and_rejects() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("vibe-jsettings-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let v = view(&db).unwrap();
        assert_eq!(
            (
                v["fees_default"].clone(),
                v["rules_default"].clone(),
                v["equity_base"].clone()
            ),
            (json!(true), json!(true), Value::Null)
        );
        let v = save(
            &db,
            &json!({"rules": {"max_positions": 5}, "equity_base": 200000}),
        )
        .unwrap();
        assert_eq!(
            (
                v["rules"]["max_positions"].clone(),
                v["rules"]["max_trades_per_day"].clone(),
                v["rules_default"].clone()
            ),
            (json!(5.0), json!(5.0), json!(false))
        );
        assert_eq!(v["equity_base"], 200000.0);
        let v = save(&db, &json!({"fees": {"commission_rate": 0.0001, "commission_min": 0, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001}})).unwrap();
        assert_eq!(
            (
                v["fees"]["commission_min"].clone(),
                v["fees_default"].clone(),
                v["rules"]["max_positions"].clone()
            ),
            (json!(0.0), json!(false), json!(5.0)),
            "没传的规矩保持原样"
        );
        assert!(save(&db, &json!({"fees": {"commission_rate": 0.0001}}))
            .unwrap_err()
            .to_string()
            .contains("四项都填"));
        assert!(save(&db, &json!({"rules": {"nope": 1}}))
            .unwrap_err()
            .to_string()
            .contains("不认识"));
        assert!(save(&db, &json!({"equity_base": -5})).is_err());
        assert_eq!(
            load(&db).unwrap().fees.commission_rate,
            0.0001,
            "被拒的保存不改已有设置"
        );
        let v = save(
            &db,
            &json!({"fees": null, "rules": {"max_positions": null}, "equity_base": null}),
        )
        .unwrap();
        assert_eq!(
            (
                v["fees_default"].clone(),
                v["rules_default"].clone(),
                v["equity_base"].clone()
            ),
            (json!(true), json!(true), Value::Null)
        );
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn broken_saved_fees_fall_back_visibly() {
        let row = Row {
            fees: Some(map(json!({"commission_rate": 0.0003}))),
            rules: None,
            equity_base: Some(-1.0),
            cost_method: None,
            updated_at: "t".into(),
        };
        assert!(row_problem(&row).is_some());
        let s = settings_of(Some(&row));
        assert!(s.fees_default, "损坏的费率标成默认,页面据此提示");
        assert_eq!(s.equity_base, None);
    }
}
