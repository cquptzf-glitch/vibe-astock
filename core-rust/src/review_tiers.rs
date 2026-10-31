//! 涨停分档(移植自 orchestrator/src/finance/review_tiers.ts):复盘那天的涨停股按东财行业归类,和前两个交易日比,
//! 按只数分四档,让复盘先看集中的、再看零散的。
//!
//! 🔴 只数是事实,档位是按只数的固定规则分的:不判断能不能持续、不预测、不排名个股。
//!    「连续集中」= 今天同一行业 ≥ 3 只、前两个交易日每天也 ≥ 2 只;其余见 TIERS。
//! 每个行业里的个股按 连板数 → 首次封板时间 → 代码 排,排第一的就是这个行业今天的「最高板」(固定规则,不是推荐)。
//! 来源:东财涨停池(按东财所属行业归类)、同花顺涨停原因里的题材标签(只取今天涨停的那几只)。
//! 与桌面版的差异:同档位同只数的行业并列时,桌面版按拼音排,手机端没有拼音库、按 Unicode 顺序排(只影响并列项的先后)。
//! 没做的:板块内「头号股贡献占比」(按市值的龙头集中度)—— 要板块成分和市值全表,东财列表接口取不到。

use crate::{
    discovery::DiscoveryError,
    emotion::{self, PoolKind, Row},
    theme_tree,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

const UNCLASSIFIED: &str = "未分类";
const TIERS: [(&str, &str, &str); 4] = [
    (
        "mainline",
        "连续集中",
        "今天同一行业 ≥ 3 只涨停,前两个交易日每天也 ≥ 2 只",
    ),
    (
        "today",
        "今天集中",
        "今天同一行业 ≥ 3 只涨停,前两个交易日不是每天都 ≥ 2 只",
    ),
    ("scattered", "零散", "今天同一行业 2 只涨停"),
    ("single", "单只", "今天这个行业只有 1 只涨停"),
];
const SOURCE: &str = "东财涨停池(按东财行业归类)、同花顺涨停原因题材标签";

fn industry(row: &Row) -> String {
    let s = row.sector.trim();
    if s.is_empty() {
        UNCLASSIFIED.into()
    } else {
        s.into()
    }
}

pub(crate) fn tier_of(today: usize, prev: [Option<usize>; 2]) -> &'static str {
    if today >= 3 {
        if prev.iter().all(|p| p.is_some_and(|n| n >= 2)) {
            "mainline"
        } else {
            "today"
        }
    } else if today == 2 {
        "scattered"
    } else {
        "single"
    }
}

fn count_by(rows: Option<&[Row]>) -> Option<HashMap<String, usize>> {
    let rows = rows?;
    let mut m: HashMap<String, usize> = HashMap::new();
    for r in rows {
        *m.entry(industry(r)).or_default() += 1;
    }
    Some(m)
}

/// 按只数分档;排序:档位 → 今天只数多的在前 → 行业名
pub(crate) fn build_tiers(
    today: &[Row],
    prev: [Option<&[Row]>; 2],
    reasons: &HashMap<String, Vec<String>>,
) -> Vec<Value> {
    let (p1, p2) = (count_by(prev[0]), count_by(prev[1]));
    let mut by: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for r in today {
        by.entry(industry(r)).or_default().push(r);
    }
    let order = |id: &str| TIERS.iter().position(|(t, _, _)| *t == id).unwrap_or(9);
    let mut groups: Vec<(usize, usize, String, Value)> = by
        .into_iter()
        .map(|(ind, mut list)| {
            let counts = [p1.as_ref().map(|m| m.get(&ind).copied().unwrap_or(0)), p2.as_ref().map(|m| m.get(&ind).copied().unwrap_or(0))];
            let tier = tier_of(list.len(), counts);
            // 连板多的在前;一样多时首封早的在前(没有首封时间的排后);再按代码,保证可复算
            list.sort_by(|a, b| {
                let seal = |r: &Row| if r.first_seal.is_empty() { "999999".to_string() } else { r.first_seal.clone() };
                b.boards.cmp(&a.boards).then_with(|| seal(a).cmp(&seal(b))).then_with(|| a.code.cmp(&b.code))
            });
            let stocks: Vec<Value> = list
                .iter()
                .map(|r| {
                    let tags = reasons.get(&r.code).cloned().unwrap_or_default();
                    let seal = (r.first_seal.len() == 6).then(|| format!("{}:{}:{}", &r.first_seal[0..2], &r.first_seal[2..4], &r.first_seal[4..6]));
                    json!({"symbol": r.code, "name": r.name, "industry": ind, "boards": r.boards, "first_seal": seal, "broken_times": r.broken_times, "tags": tags})
                })
                .collect();
            let mut freq: Vec<(String, usize)> = Vec::new();
            for r in &list {
                for t in reasons.get(&r.code).into_iter().flatten() {
                    match freq.iter_mut().find(|(x, _)| x == t) {
                        Some(e) => e.1 += 1,
                        None => freq.push((t.clone(), 1)),
                    }
                }
            }
            freq.sort_by_key(|b| std::cmp::Reverse(b.1));
            let tags: Vec<Value> = freq.into_iter().take(3).map(|(tag, count)| json!({"tag": tag, "count": count})).collect();
            let note = if list.len() >= 3 && counts.iter().any(Option::is_none) { "前两个交易日的涨停池没取全,分不出是不是连续" } else { "" };
            (order(tier), list.len(), ind.clone(), json!({"industry": ind, "tier": tier, "today": list.len(), "prev": counts, "stocks": stocks, "tags": tags, "note": note}))
        })
        .collect();
    groups.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    groups.into_iter().map(|g| g.3).collect()
}

pub async fn fetch(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, prev) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    let settled = emotion::is_settled(&date, &calendar, now);
    let prev2 = prev.as_ref().and_then(|p| {
        calendar
            .iter()
            .rev()
            .find(|d| d.as_str() < p.as_str())
            .cloned()
    });
    let prev_days = [prev.clone(), prev2];
    let mut gaps: Vec<String> = Vec::new();
    if prev_days.iter().any(Option::is_none) {
        gaps.push("前两个交易日没找到(交易日历不够长),「连续集中」分不出来".into());
    }
    let date_compact = date.replace('-', "");
    let (today, p1, p2, reasons) = futures_util::join!(
        emotion::fetch_pool(client, PoolKind::LimitUp, &date_compact),
        async {
            match &prev_days[0] {
                Some(d) => {
                    Some(emotion::fetch_pool(client, PoolKind::LimitUp, &d.replace('-', "")).await)
                }
                None => None,
            }
        },
        async {
            match &prev_days[1] {
                Some(d) => {
                    Some(emotion::fetch_pool(client, PoolKind::LimitUp, &d.replace('-', "")).await)
                }
                None => None,
            }
        },
        theme_tree::limit_reasons(client, db, &date, settled),
    );
    let prev_pools: [Option<Vec<Row>>; 2] = [p1, p2].map(|p| p.and_then(Result::ok));
    for (i, day) in prev_days.iter().enumerate() {
        if let (Some(d), None) = (day, &prev_pools[i]) {
            gaps.push(format!("{d} 的涨停池没取到,「连续集中」分不出来"));
        }
    }
    let tag_map: HashMap<String, Vec<String>> = match &reasons {
        Ok((m, _)) => m
            .iter()
            .map(|(c, r)| {
                let mut tags: Vec<String> = Vec::new();
                for t in r.split('+').map(str::trim).filter(|t| !t.is_empty()) {
                    if !tags.iter().any(|x| x == t) {
                        tags.push(t.to_string());
                    }
                }
                (c.clone(), tags)
            })
            .collect(),
        Err(e) => {
            gaps.push(format!("同花顺题材标签没取到,只按东财行业分({e})"));
            HashMap::new()
        }
    };
    let (groups, total) = match today {
        Ok(rows) if !rows.is_empty() => {
            let g = build_tiers(
                &rows,
                [prev_pools[0].as_deref(), prev_pools[1].as_deref()],
                &tag_map,
            );
            (g, Some(rows.len()))
        }
        Ok(_) => {
            gaps.insert(
                0,
                format!("{date} 的涨停池是空的(盘前、休市日或日期不对时如此),没法分档"),
            );
            (Vec::new(), None)
        }
        Err(e) => {
            gaps.insert(
                0,
                format!(
                    "{date} 的涨停池没取到({}),没法分档",
                    emotion::error_message("涨停池", &e)
                ),
            );
            (Vec::new(), None)
        }
    };
    let tiers: Vec<Value> = TIERS
        .iter()
        .map(|(id, label, rule)| {
            let g: Vec<&Value> = groups.iter().filter(|x| x["tier"] == *id).collect();
            json!({"id": id, "label": label, "rule": rule, "industries": g.len(), "stocks": g.iter().map(|x| x["today"].as_u64().unwrap_or(0)).sum::<u64>()})
        })
        .collect();
    Ok(
        json!({"date": date, "settled": settled, "prev_dates": prev_days, "total": total, "groups": groups, "tiers": tiers, "gaps": gaps, "source": SOURCE}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, sector: &str, boards: i64, seal: &str) -> Row {
        Row {
            code: code.into(),
            name: format!("N{code}"),
            sector: sector.into(),
            boards,
            first_seal: seal.into(),
            ..Row::default()
        }
    }

    #[test]
    fn tier_rule_uses_today_and_both_previous_days() {
        assert_eq!(tier_of(3, [Some(2), Some(5)]), "mainline");
        assert_eq!(tier_of(3, [Some(2), Some(1)]), "today");
        assert_eq!(
            tier_of(3, [Some(2), None]),
            "today",
            "前一天没取到不能当连续"
        );
        assert_eq!(tier_of(4, [Some(0), Some(0)]), "today");
        assert_eq!(
            (tier_of(2, [Some(9), Some(9)]), tier_of(1, [None, None])),
            ("scattered", "single")
        );
    }

    #[test]
    fn groups_sort_by_tier_then_count_and_stocks_by_board_then_seal_time() {
        let today = vec![
            row("1", "汽车", 2, "093000"),
            row("2", "汽车", 3, "100000"),
            row("3", "汽车", 3, "094500"),
            row("4", "银行", 1, "093000"),
            row("5", "电力", 1, "093000"),
            row("6", "电力", 1, ""),
            row("7", "", 1, "101500"),
        ];
        let p1 = vec![
            row("a", "汽车", 1, ""),
            row("b", "汽车", 1, ""),
            row("c", "电力", 1, ""),
        ];
        let p2 = vec![
            row("d", "汽车", 1, ""),
            row("e", "汽车", 1, ""),
            row("f", "汽车", 1, ""),
        ];
        let reasons: HashMap<String, Vec<String>> = [
            (
                "2".to_string(),
                vec!["兵装重组".to_string(), "无人机".to_string()],
            ),
            ("3".into(), vec!["兵装重组".into()]),
        ]
        .into_iter()
        .collect();
        let g = build_tiers(&today, [Some(&p1), Some(&p2)], &reasons);
        assert_eq!(
            g.iter()
                .map(|x| (
                    x["industry"].as_str().unwrap(),
                    x["tier"].as_str().unwrap(),
                    x["today"].as_u64().unwrap()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("汽车", "mainline", 3),
                ("电力", "scattered", 2),
                ("未分类", "single", 1),
                ("银行", "single", 1)
            ],
            "并列时按行业名的 Unicode 顺序(桌面版是拼音顺序,只影响并列项的先后)"
        );
        let car: Vec<&str> = g[0]["stocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["symbol"].as_str().unwrap())
            .collect();
        assert_eq!(car, vec!["3", "2", "1"], "连板多的在前,同板数首封早的在前");
        assert_eq!(g[0]["prev"], json!([2, 3]));
        assert_eq!(g[0]["tags"][0], json!({"tag": "兵装重组", "count": 2}));
        assert_eq!(
            g[1]["stocks"][1]["first_seal"],
            Value::Null,
            "没有首封时间排后、不编时间"
        );
        assert_eq!(g[0]["stocks"][0]["first_seal"], "09:45:00");
    }

    #[test]
    fn missing_previous_pools_are_called_out_instead_of_guessed() {
        let today: Vec<Row> = (1..=3)
            .map(|i| row(&i.to_string(), "汽车", 1, "093000"))
            .collect();
        let g = build_tiers(&today, [None, None], &HashMap::new());
        assert_eq!(
            (g[0]["tier"].clone(), g[0]["prev"].clone()),
            (json!("today"), json!([null, null]))
        );
        assert!(g[0]["note"].as_str().unwrap().contains("没取全"));
        let ok = build_tiers(&today, [Some(&[]), Some(&[])], &HashMap::new());
        assert_eq!(
            ok[0]["prev"],
            json!([0, 0]),
            "取到了但行业没涨停 = 0,不是缺"
        );
        assert_eq!(ok[0]["note"], "");
    }

    /// 对真实上游:cargo test --locked review_tiers::tests::live_tiers_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_tiers_probe() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-tiers-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let r = app
            .handle(crate::MobileApiRequest {
                path: "/mobile/review/tiers".into(),
                method: "GET".into(),
                body: Value::Null,
            })
            .await;
        println!(
            "status {} date {} prev {} total {} gaps {}",
            r.status, r.body["date"], r.body["prev_dates"], r.body["total"], r.body["gaps"]
        );
        println!("tiers {}", r.body["tiers"]);
        if let Ok(out) = std::env::var("VIBE_TIERS_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&r.body).unwrap()).unwrap();
        }
        assert_eq!(r.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
