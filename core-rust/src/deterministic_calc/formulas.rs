use super::*;
fn amount(a: &Value, key: &str, unit_key: &str) -> R<(f64, &'static str)> {
    let (ccy, scale, _) = money_unit(text(a, unit_key, "")?)?;
    Ok((finite(n(a, key)? * scale)?, ccy))
}
fn fx(a: &Value) -> R<(f64, &str, &str)> {
    let r = n(a, "fx_rate")?;
    if r <= 0. {
        return Err("fx_rate 必须为正数".into());
    }
    let s = text(a, "fx_unit", "")?;
    let (quote, base) = s.split_once('/').ok_or("fx_unit 应为 报价币种/基准币种")?;
    if quote == base
        || !["CNY", "USD", "HKD"].contains(&quote)
        || !["CNY", "USD", "HKD"].contains(&base)
    {
        return Err("fx_unit 币种无效".into());
    }
    Ok((r, quote, base))
}
fn no_fx(a: &Value) -> R<()> {
    if !a["fx_rate"].is_null() || !a["fx_unit"].is_null() {
        Err("同币种或未提供价格单位时，不应传汇率".into())
    } else {
        Ok(())
    }
}
fn convert(amount: f64, from: &str, to: &str, a: &Value) -> R<f64> {
    let (rate, quote, base) = fx(a)?;
    if from == base && to == quote {
        finite(amount * rate)
    } else if from == quote && to == base {
        finite(amount / rate)
    } else {
        Err("汇率币种与金额币种不对应".into())
    }
}
fn suffix(ccy: &str) -> &str {
    match ccy {
        "USD" => "usd",
        "HKD" => "hkd",
        _ => "yuan",
    }
}
fn ratio_arg(a: &Value, key: &str, max: f64) -> R<f64> {
    let x = n(a, key)?;
    if x.abs() > max {
        Err(format!("{key} 疑似把百分数当小数传入"))
    } else {
        Ok(x)
    }
}
fn price_ccy(unit: &str, share: bool) -> R<&str> {
    let base = if share {
        unit.strip_suffix("/股").ok_or("EPS 单位应以 /股 结尾")?
    } else {
        unit
    };
    match base {
        "元" => Ok("CNY"),
        "美元" => Ok("USD"),
        "港元" => Ok("HKD"),
        _ => Err("价格或 EPS 单位不支持".into()),
    }
}
fn pe(a: &Value, annualized: bool) -> R<Value> {
    let (cap, ccy) = amount(a, "total_market_cap", "cap_unit")?;
    let profit_key = if annualized {
        "latest_quarter_deducted_profit"
    } else {
        "ttm_profit"
    };
    let (mut profit, profit_ccy) = amount(a, profit_key, "profit_unit")?;
    let scale = if annualized { 4. } else { 1. };
    let label = if annualized {
        "最新单季扣非净利润"
    } else {
        "TTM 净利润"
    };
    let mut details = json!({});
    if ccy != profit_ccy {
        let (rate, _, _) = fx(a)?;
        details = json!({"fx_rate":rate,"fx_unit":a["fx_unit"],"denominator_original":v(profit*scale),"denominator_original_currency":profit_ccy});
        profit = convert(profit, profit_ccy, ccy, a)?;
    } else {
        no_fx(a)?;
    }
    let denominator = finite(profit * scale)?;
    let s = suffix(ccy);
    details[format!("mcap_{s}")] = v(cap);
    if ccy != "CNY" || ccy != profit_ccy {
        details["currency"] = json!(ccy);
    }
    if cap <= 0. {
        return Ok(nm("倍", "总市值 ≤ 0", details));
    }
    details[format!("denominator_{s}")] = v(denominator);
    if denominator <= 0. {
        return Ok(nm("倍", format!("{label} ≤ 0,PE 无意义"), details));
    }
    details["denominator_label"] = json!(label);
    if annualized {
        details[format!("quarter_profit_{s}")] = v(denominator / 4.);
        details[format!("annualized_profit_{s}")] = v(denominator);
    } else {
        details[format!("ttm_{s}")] = v(denominator);
    }
    Ok(ok(cap / denominator, "倍", details))
}
pub(super) fn run(function: &str, a: &Value) -> R<Value> {
    Ok(match function {
        "pe_deducted_annualized" => return pe(a, true),
        "pe_ttm_from_parts" => return pe(a, false),
        "forward_pe" => {
            let p = n(a, "price")?;
            let e = n(a, "eps_forecast")?;
            let mut used = e;
            let mut details = json!({});
            if a["price_unit"].is_null() != a["eps_unit"].is_null() {
                return Err("price_unit 与 eps_unit 要么都给，要么都不给".into());
            }
            if a["price_unit"].is_null() {
                no_fx(a)?;
            } else {
                let pu = text(a, "price_unit", "")?;
                let eu = text(a, "eps_unit", "")?;
                let pc = price_ccy(pu, false)?;
                let ec = price_ccy(eu, true)?;
                details = json!({"currency":pc,"price_unit":pu,"eps_unit":eu});
                if pc != ec {
                    let (rate, _, _) = fx(a)?;
                    used = convert(e, ec, pc, a)?;
                    details["fx_rate"] = v(rate);
                    details["fx_unit"] = a["fx_unit"].clone();
                    details["eps_converted"] = v(used);
                } else {
                    no_fx(a)?;
                }
            }
            if p <= 0. {
                details["price"] = v(p);
                nm("倍", "价格 ≤ 0", details)
            } else if used <= 0. {
                details["eps"] = v(e);
                nm("倍", "一致预期 EPS ≤ 0,前瞻 PE 无意义", details)
            } else {
                details["price"] = v(p);
                details["eps"] = v(e);
                ok(p / used, "倍", details)
            }
        }
        "fx_convert" => {
            let (amt, from) = amount(a, "value", "unit")?;
            let (rate, quote, base) = fx(a)?;
            let to = if from == base {
                quote
            } else if from == quote {
                base
            } else {
                return Err("金额币种与汇率不对应".into());
            };
            let unit = match to {
                "CNY" => "元",
                "USD" => "美元",
                _ => "港元",
            };
            ok(
                convert(amt, from, to, a)?,
                unit,
                json!({"from_amount":v(amt),"from_currency":from,"to_currency":to,"fx_rate":rate,"fx_unit":a["fx_unit"]}),
            )
        }
        "forward_cagr" => {
            let x = n(a, "eps_t")?;
            let y = n(a, "eps_t_plus_n")?;
            let years = integer(a, "years", 2, 1, 10)?;
            if x <= 0. || y <= 0. {
                nm(
                    "小数",
                    "起点或终点 EPS ≤ 0(跨零),年化增速无意义",
                    json!({"eps_t":x,"eps_t_plus_n":y}),
                )
            } else {
                ok(
                    (y / x).powf(1. / years as f64) - 1.,
                    "小数",
                    json!({"eps_t":x,"eps_t_plus_n":y,"years":years}),
                )
            }
        }
        "growth_rate" => {
            let c = n(a, "current")?;
            let b = n(a, "base")?;
            let label = text(a, "label", "growth")?;
            if b <= 0. {
                nm(
                    "小数",
                    format!("{label}:基期值 ≤ 0,增速无意义"),
                    json!({"current":c,"base":b}),
                )
            } else {
                ok(
                    c / b - 1.,
                    "小数",
                    json!({"current":c,"base":b,"label":label}),
                )
            }
        }
        "ratio" => {
            let x = n(a, "numerator")?;
            let d = n(a, "denominator")?;
            let label = text(a, "label", "ratio")?;
            let unit = text(a, "unit_in", "")?;
            if d <= 0. {
                nm(
                    "小数",
                    format!("{label}:分母 ≤ 0,比率无意义"),
                    json!({"numerator":x,"denominator":d,"unit_in":unit}),
                )
            } else {
                ok(
                    x / d,
                    "小数",
                    json!({"numerator":x,"denominator":d,"label":label,"unit_in":unit}),
                )
            }
        }
        "peg" => {
            let p = n(a, "pe")?;
            let g = ratio_arg(a, "cagr", 5.)?;
            let det = json!({"pe":p,"cagr":g});
            if p <= 0. {
                nm("倍", "PE ≤ 0", det)
            } else if g <= 0. {
                nm("倍", "CAGR ≤ 0,PEG 无意义", det)
            } else {
                ok(p / (g * 100.), "倍", det)
            }
        }
        "pe_digestion_years" => {
            let p = n(a, "pe")?;
            let g = ratio_arg(a, "cagr", 5.)?;
            let anchor = n(a, "anchor")?;
            if anchor <= 0. {
                let mut e = error("锚必须 > 0");
                e["details"] = json!({"anchor":anchor});
                e
            } else if p <= 0. {
                nm("年", "PE ≤ 0", json!({"pe":p,"anchor":anchor}))
            } else if p <= anchor {
                ok(
                    0.,
                    "年",
                    json!({"pe":p,"cagr":g,"anchor":anchor,"below_anchor":true}),
                )
            } else if g <= 0. {
                nm(
                    "年",
                    "CAGR ≤ 0,PE 永远消化不到锚",
                    json!({"pe":p,"cagr":g,"anchor":anchor}),
                )
            } else {
                ok(
                    (p / anchor).ln() / (1. + g).ln(),
                    "年",
                    json!({"pe":p,"cagr":g,"anchor":anchor,"below_anchor":false}),
                )
            }
        }
        "pe_digestion_scenarios" => {
            let anchors =
                json!({"景气延续":30.,"中性减速":25.,"周期重定级_上沿":22.,"周期重定级_下沿":18.});
            let mut scenarios = json!({});
            let mut args = a.clone();
            let mut valid = false;
            let mut failed = false;
            for (name, anchor) in anchors.as_object().unwrap() {
                args["anchor"] = anchor.clone();
                let result = run("pe_digestion_years", &args).unwrap_or_else(error);
                valid |= result["status"] == "ok";
                failed |= result["status"] == "error";
                scenarios[name] = result;
            }
            if failed {
                let mut e = error("情景输入非法，见 scenarios");
                e["details"] = json!({"scenarios":scenarios});
                e
            } else {
                out(
                    if valid { "ok" } else { "not_meaningful" },
                    None,
                    "年",
                    if valid {
                        ""
                    } else {
                        "所有情景均无意义(见 scenarios)"
                    },
                    json!({"scenarios":scenarios,"anchors":anchors}),
                )
            }
        }
        "percentile_rank" => {
            let current = n(a, "current")?;
            let history = a["history"].as_array().ok_or("history 必须为数值列表")?;
            let exclude = boolean(a, "exclude_nonpositive", true)?;
            let mut values = Vec::new();
            let mut skipped = 0;
            for x in history {
                match num(x, "history") {
                    Ok(x) if !exclude || x > 0. => values.push(x),
                    _ => skipped += 1,
                }
            }
            let count = values.len();
            if count < 20 {
                nm(
                    "%",
                    format!("有效历史样本 {count} < 20"),
                    json!({"n":count,"skipped":skipped}),
                )
            } else if exclude && current <= 0. {
                nm("%", "当前值 ≤ 0", json!({"n":count,"skipped":skipped}))
            } else {
                values.sort_by(f64::total_cmp);
                let le = values.iter().filter(|&&x| x <= current).count();
                let median = if count % 2 == 1 {
                    values[count / 2]
                } else {
                    (values[count / 2 - 1] + values[count / 2]) / 2.
                };
                ok(
                    100. * le as f64 / count as f64,
                    "%",
                    json!({"n":count,"skipped":skipped,"min":values[0],"median":v(median),"max":values[count-1],"current":current}),
                )
            }
        }
        "consensus_dispersion" => {
            let lo = n(a, "low")?;
            let mean = n(a, "mean")?;
            let hi = n(a, "high")?;
            let mut d = json!({"low":lo,"mean":mean,"high":hi});
            if !(lo <= mean && mean <= hi) {
                let mut e = error("必须满足 low ≤ mean ≤ high");
                e["details"] = d;
                e
            } else if lo <= 0. || mean <= 0. {
                nm("", "min 或 mean ≤ 0", d)
            } else {
                d["range_over_mean"] = v((hi - lo) / mean);
                ok(hi / lo, "倍", d)
            }
        }
        "forward_vs_ttm_judgement" => {
            let f = ratio_arg(a, "forward_cagr_value", 5.)?;
            let t = ratio_arg(a, "ttm_yoy_value", 20.)?;
            let tol = if a.get("tolerance_pp").is_none() {
                10.
            } else {
                n(a, "tolerance_pp")?
            };
            if tol < 0. {
                let mut e = error("tolerance_pp 必须 ≥ 0");
                e["details"] = json!({"tolerance_pp":tol});
                e
            } else {
                let diff = (f - t) * 100.;
                let (cat, label) = if diff.abs() <= tol {
                    ("approx", "前瞻 ≈ TTM:增长已兑现,可信度高")
                } else if diff < 0. {
                    ("forward_below", "前瞻远低于 TTM:隐含大幅减速,需说明依据")
                } else {
                    (
                        "forward_above",
                        "前瞻远高于 TTM:预期偏高风险,需在手订单/产能/业绩预告等额外证据",
                    )
                };
                ok(
                    diff,
                    "百分点",
                    json!({"category":cat,"label":label,"forward_cagr":f,"ttm_yoy":t,"tolerance_pp":tol}),
                )
            }
        }
        _ => return Err("未知公式".into()),
    })
}
