use super::*;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Clone)]
pub(super) struct Row {
    pub day: String,
    pub values: BTreeMap<String, f64>,
}
pub(super) fn day(s: &Value) -> R<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re=RE.get_or_init(||regex::Regex::new(r"^(\d{4}-\d{2}-\d{2})(?:[ T](?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d)?(?:\.\d+)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)?)?$").unwrap());
    let raw = s.as_str().ok_or("日期必须是字符串")?.trim();
    let captures = re.captures(raw).ok_or("日期或时间后缀非法")?;
    let d = &captures[1];
    super::series::date(d)?;
    Ok(d.to_owned())
}
pub(super) fn rows(a: &Value, need: &[&str], optional: &[&str]) -> R<Vec<Row>> {
    let list = a
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("klines 必须是非空 K 线行列表")?;
    if list.len() > 10000 {
        return Err("K 线超过一万行预算".into());
    }
    let mut rows = Vec::new();
    for r in list {
        if !r.is_object() {
            return Err("K 线元素不是对象".into());
        }
        let date = day(&r["date"])?;
        let mut values = BTreeMap::new();
        for &key in need
            .iter()
            .chain(optional.iter().filter(|&&k| !r[k].is_null() && r[k] != ""))
        {
            let value = n(r, key)?;
            if ["open", "high", "low", "close"].contains(&key) && value <= 0. {
                return Err(format!("{date} {key} 须为正数"));
            }
            values.insert(key.to_owned(), value);
        }
        if let (Some(&h), Some(&l)) = (values.get("high"), values.get("low")) {
            if h < l {
                return Err("high < low".into());
            }
            for key in ["open", "close"] {
                if values.get(key).is_some_and(|&x| x < l || x > h) {
                    return Err(format!("{key} 不在 [low, high] 内"));
                }
            }
        }
        rows.push(Row { day: date, values });
    }
    rows.sort_by(|a, b| a.day.cmp(&b.day));
    if rows.windows(2).any(|w| w[0].day == w[1].day) {
        return Err("K 线日期重复".into());
    }
    Ok(rows)
}
fn periods(a: &Value, k: &str, default: &[usize]) -> R<Vec<usize>> {
    if a.get(k).is_none() {
        return Ok(default.to_vec());
    }
    let list = a[k]
        .as_array()
        .filter(|s| !s.is_empty() && s.len() <= 64)
        .ok_or("周期须为非空整数列表，至多六十四项")?;
    list.iter()
        .map(|p| {
            p.as_u64()
                .filter(|p| (1..=500).contains(p))
                .map(|n| n as usize)
                .ok_or("周期须在一至五百".into())
        })
        .collect()
}
fn ema(values: &[f64], period: usize) -> Vec<f64> {
    let mut e = vec![values[0]];
    let k = 2. / (period + 1) as f64;
    for &x in &values[1..] {
        e.push(x * k + e.last().unwrap() * (1. - k));
    }
    e
}
fn technical(a: &Value) -> R<Value> {
    let rows = rows(&a["klines"], &["open", "high", "low", "close"], &[])?;
    let ma = periods(a, "ma", &[5, 10, 20, 60])?;
    let ep = periods(a, "ema", &[12, 26])?;
    let macd = periods(a, "macd", &[12, 26, 9])?;
    let rsi = periods(a, "rsi", &[6, 12, 24])?;
    let kdj = periods(a, "kdj", &[9, 3, 3])?;
    if macd.len() != 3 || kdj.len() != 3 {
        return Err("macd / kdj 必须各给三个参数".into());
    }
    let boll = a.get("boll").cloned().unwrap_or_else(|| json!([20, 2.]));
    let boll = boll
        .as_array()
        .filter(|s| s.len() == 2)
        .ok_or("boll 必须为 [周期,标准差倍数]")?;
    let bn = boll[0]
        .as_u64()
        .filter(|n| (1..=500).contains(n))
        .ok_or("BOLL 周期须在一至五百")? as usize;
    let bk = num(&boll[1], "boll.num_std")?;
    if bk <= 0. || bk > 10. {
        return Err("boll.num_std 须在 (0,10]".into());
    }
    let mp = integer(a, "min_points", 30, 5, usize::MAX)?;
    let n = rows.len();
    if n < mp {
        return Ok(nm(
            "元",
            format!("K 线仅 {n} 根,少于 {mp},指标不可靠"),
            json!({"points":n}),
        ));
    }
    let closes: Vec<_> = rows.iter().map(|r| r.values["close"]).collect();
    let last = rows.last().unwrap();
    let mut det = json!({"date":last.day,"points":n,"window":[rows[0].day,last.day],"params":{"ma":ma,"ema":ep,"macd":macd,"rsi":rsi,"kdj":kdj,"boll":[bn,bk]},"ma":{},"ema":{},"rsi":{}});
    for p in ma {
        det["ma"][format!("ma{p}")] = if n >= p {
            v(closes[n - p..].iter().sum::<f64>() / p as f64)
        } else {
            Value::Null
        };
    }
    for p in ep {
        det["ema"][format!("ema{p}")] = v(*ema(&closes, p).last().unwrap());
    }
    let dif: Vec<_> = ema(&closes, macd[0])
        .into_iter()
        .zip(ema(&closes, macd[1]))
        .map(|(a, b)| a - b)
        .collect();
    let dea = ema(&dif, macd[2]);
    det["macd"] = json!({"dif":v(*dif.last().unwrap()),"dea":v(*dea.last().unwrap()),"hist":v((dif.last().unwrap()-dea.last().unwrap())*2.)});
    let mut changes = vec![0.];
    changes.extend(closes.windows(2).map(|w| w[1] - w[0]));
    for p in rsi {
        let value = if n - 1 < p {
            Value::Null
        } else {
            let gains = changes[n - p..].iter().map(|&c| c.max(0.)).sum::<f64>() / p as f64;
            let losses = changes[n - p..].iter().map(|&c| (-c).max(0.)).sum::<f64>() / p as f64;
            v(if losses == 0. {
                100.
            } else {
                100. - 100. / (1. + gains / losses)
            })
        };
        det["rsi"][format!("rsi{p}")] = value;
    }
    let (kn, m1, m2) = (kdj[0], kdj[1] as f64, kdj[2] as f64);
    let (mut kv, mut dv) = (50., 50.);
    let mut kd = Value::Null;
    for i in kn - 1..n {
        let w = &rows[i + 1 - kn..=i];
        let hi = w
            .iter()
            .map(|r| r.values["high"])
            .fold(f64::NEG_INFINITY, f64::max);
        let lo = w
            .iter()
            .map(|r| r.values["low"])
            .fold(f64::INFINITY, f64::min);
        let rsv = if hi != lo {
            (rows[i].values["close"] - lo) / (hi - lo) * 100.
        } else {
            50.
        };
        kv = rsv / m1 + kv * (1. - 1. / m1);
        dv = kv / m2 + dv * (1. - 1. / m2);
        kd = json!({"k":v(kv),"d":v(dv),"j":v(3.*kv-2.*dv)});
    }
    det["kdj"] = kd;
    det["boll"] = if n >= bn {
        let w = &closes[n - bn..];
        let mid = w.iter().sum::<f64>() / bn as f64;
        let std = (w.iter().map(|x| (x - mid).powi(2)).sum::<f64>() / bn as f64).sqrt();
        let (up, lo) = (mid + bk * std, mid - bk * std);
        json!({"upper":v(up),"middle":v(mid),"lower":v(lo),"bandwidth_pct":if mid!=0.{v((up-lo)/mid*100.)}else{Value::Null}})
    } else {
        Value::Null
    };
    Ok(ok(*closes.last().unwrap(), "元", det))
}
fn nearest(grid: &[f64], price: f64) -> usize {
    let mut best = 0;
    for i in 1..grid.len() {
        if (grid[i] - price).abs() < (grid[best] - price).abs() {
            best = i;
        }
    }
    best
}
fn triangle(grid: &[f64], lo: f64, hi: f64, avg: f64) -> Vec<f64> {
    let mut weights = vec![0.; grid.len()];
    if hi < lo {
        return weights;
    }
    if hi - lo < 1e-9 {
        weights[nearest(grid, lo)] = 1.;
        return weights;
    }
    let avg = avg.max(lo).min(hi);
    for (i, &g) in grid.iter().enumerate() {
        weights[i] = if lo <= g && g <= avg {
            if avg - lo > 1e-9 {
                (g - lo) / (avg - lo)
            } else {
                1.
            }
        } else if avg < g && g <= hi {
            if hi - avg > 1e-9 {
                (hi - g) / (hi - avg)
            } else {
                1.
            }
        } else {
            0.
        };
    }
    let total = weights.iter().sum::<f64>();
    if total > 0. {
        for w in &mut weights {
            *w /= total;
        }
    } else {
        weights[nearest(grid, avg)] = 1.;
    }
    weights
}
fn chips(a: &Value) -> R<Value> {
    let rows = rows(&a["klines"], &["high", "low", "close", "turn"], &["open"])?;
    let size = integer(a, "grid_size", 300, 50, 2000)?;
    let decay = if a.get("decay").is_none() {
        1.
    } else {
        n(a, "decay")?
    };
    if decay <= 0. || decay > 5. {
        return Err("decay 须在 (0,5]".into());
    }
    let mp = integer(a, "min_points", 20, 2, usize::MAX)?;
    if rows
        .iter()
        .any(|r| !(0. ..=100.).contains(&r.values["turn"]))
    {
        return Err("turn 须在 0..100 (百分数)".into());
    }
    if rows.len() < mp {
        return Ok(nm(
            "小数",
            format!("K 线仅 {} 根,少于 {mp}", rows.len()),
            json!({"points":rows.len()}),
        ));
    }
    let lo = rows
        .iter()
        .map(|r| r.values["low"])
        .fold(f64::INFINITY, f64::min);
    let hi = rows
        .iter()
        .map(|r| r.values["high"])
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = if hi != lo {
        (hi - lo) * 0.02
    } else {
        (lo * 0.02).max(0.01)
    };
    let (g0, g1) = (lo - pad, hi + pad);
    let grid: Vec<_> = (0..size)
        .map(|i| g0 + (g1 - g0) * i as f64 / (size - 1) as f64)
        .collect();
    let mut chips: Option<Vec<f64>> = None;
    for r in &rows {
        let t = (r.values["turn"] / 100. * decay).clamp(0., 1.);
        let w = triangle(
            &grid,
            r.values["low"],
            r.values["high"],
            (r.values["high"] + r.values["low"] + r.values["close"]) / 3.,
        );
        if w.iter().sum::<f64>() <= 0. {
            continue;
        }
        chips = Some(match chips {
            None => w,
            Some(chips) => chips
                .into_iter()
                .zip(w)
                .map(|(c, x)| c * (1. - t) + x * t)
                .collect(),
        });
    }
    let mut chips = chips.ok_or("无法构建筹码分布")?;
    let total = chips.iter().sum::<f64>();
    if total <= 0. {
        return Err("筹码总量为零".into());
    }
    for c in &mut chips {
        *c /= total;
    }
    let mut acc = 0.;
    let cum: Vec<_> = chips
        .iter()
        .map(|c| {
            acc += c;
            acc
        })
        .collect();
    let at = |q: f64| {
        for (i, &cv) in cum.iter().enumerate() {
            if cv >= q {
                if i == 0 || cum[i] == cum[i - 1] {
                    return grid[i];
                }
                let fraction = (q - cum[i - 1]) / (cum[i] - cum[i - 1]);
                return grid[i - 1] + (grid[i] - grid[i - 1]) * fraction;
            }
        }
        *grid.last().unwrap()
    };
    let (p05, p15, p85, p95) = (at(0.05), at(0.15), at(0.85), at(0.95));
    let mut peak = 0;
    for i in 1..size {
        if chips[i] > chips[peak] {
            peak = i;
        }
    }
    let price = rows.last().unwrap().values["close"];
    let step = (g1 - g0) / (size - 1) as f64;
    let profit = grid
        .iter()
        .zip(&chips)
        .filter(|(g, _)| **g <= price + step / 2.)
        .map(|(_, c)| c)
        .sum();
    Ok(ok(
        profit,
        "小数",
        json!({"price":price,"avg_cost":v(grid.iter().zip(&chips).map(|(g,c)|g*c).sum()),"cost_90":floats(&[p05,p95]),"cost_70":floats(&[p15,p85]),"concentration_90":if p95+p05!=0.{v((p95-p05)/(p95+p05))}else{Value::Null},"concentration_70":if p85+p15!=0.{v((p85-p15)/(p85+p15))}else{Value::Null},"peak_price":v(grid[peak]),"days":rows.len(),"window":[rows[0].day,rows.last().unwrap().day],"cum_turnover_pct":v(rows.iter().map(|r|r.values["turn"]).sum()),"grid_size":size,"decay":decay}),
    ))
}
pub(super) fn run(function: &str, args: &Value) -> R<Value> {
    if function == "technical_indicators" {
        technical(args)
    } else {
        chips(args)
    }
}
