//! Evidence-bound, topologically executed plans. This is JSON data, never code.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
pub const MAX_NODES: usize = 12;
fn pointer<'a>(value: &'a Value, path: &str) -> R<&'a Value> {
    if path.len() > 300 || (!path.is_empty() && !path.starts_with('/')) {
        return Err("引用路径须为不超过三百字节的 JSON 指针".into());
    }
    value
        .pointer(path)
        .ok_or_else(|| format!("引用路径不存在:{path}"))
}
fn projection(value: &Value, binding: &Value) -> R<Value> {
    if binding.get("column").is_some() && binding.get("columns").is_some() {
        return Err("column 与 columns 不能同时使用".into());
    }
    if binding.get("column").is_none()
        && binding.get("columns").is_none()
        && binding.get("where").is_none()
    {
        return Ok(value.clone());
    }
    let rows = value.as_array().ok_or("投影输入必须是源数组")?;
    let mut out = Vec::new();
    let where_fields = match binding.get("where") {
        None => None,
        Some(w) => Some(
            w.as_object()
                .filter(|m| m.len() <= 8)
                .ok_or("where 须为至多八个 JSON 指针的精确匹配条件")?,
        ),
    };
    let columns = match binding.get("columns") {
        None => None,
        Some(v) => Some(
            v.as_object()
                .filter(|m| !m.is_empty() && m.len() <= 8)
                .ok_or("columns 须为至多八列的源路径映射")?,
        ),
    };
    for row in rows {
        if let Some(fields) = where_fields {
            let mut matched = true;
            for (path, expected) in fields {
                if expected.is_array() || expected.is_object() || expected.to_string().len() > 100 {
                    return Err("where 只支持短标量精确匹配，不执行表达式".into());
                }
                if pointer(row, path)? != expected {
                    matched = false;
                }
            }
            if !matched {
                continue;
            }
        }
        if let Some(cols) = columns {
            let mut mapped = json!({});
            for (name, path) in cols {
                if name.len() > 40 {
                    return Err("投影字段名过长".into());
                }
                mapped[name] =
                    pointer(row, path.as_str().ok_or("投影列必须是 JSON 指针")?)?.clone();
            }
            out.push(mapped);
        } else if let Some(path) = binding.get("column") {
            out.push(pointer(row, path.as_str().ok_or("column 须为 JSON 指针")?)?.clone());
        } else {
            out.push(row.clone());
        }
    }
    Ok(Value::Array(out))
}
fn resolve(
    binding: &Value,
    key: &str,
    config: bool,
    evidence: &Value,
    records: &BTreeMap<String, Value>,
) -> R<(Value, Option<Value>)> {
    if !binding.is_object() {
        return if config {
            Ok((binding.clone(), None))
        } else {
            Err(format!("{key} 必须绑定证据或前序计算，不能由模型填写数值"))
        };
    }
    let map = binding.as_object().unwrap();
    if map.keys().any(|k| {
        ![
            "evidence",
            "calculation",
            "path",
            "column",
            "columns",
            "where",
        ]
        .contains(&k.as_str())
    }) {
        return Err("输入引用包含未许可字段".into());
    }
    let ev = binding["evidence"].as_str();
    let cid = binding["calculation"].as_str();
    if ev.is_some() == cid.is_some() {
        return Err("必须且只能指定 evidence 或 calculation".into());
    }
    let path = binding["path"].as_str().ok_or("输入引用缺少 path")?;
    let (root, origin) = if let Some(id) = ev {
        let source = evidence
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == id)
            .ok_or_else(|| format!("不是本轮证据:{id}"))?;
        (
            &source["data"],
            json!({"type":"evidence","id":id,"path":path,"source":source["source"],"fetched_at":source["fetched_at"],"truncated":source["truncated"]==true}),
        )
    } else {
        let id = cid.unwrap();
        let record = records
            .get(id)
            .ok_or_else(|| format!("计算依赖未完成:{id}"))?;
        if record["output"]["status"] != "ok" {
            return Err(format!("上游计算没有可用结果:{id}"));
        }
        (
            &record["output"],
            json!({"type":"calculation","id":record["calculation_id"],"path":path}),
        )
    };
    let value = projection(pointer(root, path)?, binding)?;
    Ok((value, Some(origin)))
}
fn canonical(v: &mut Value) {
    match v {
        Value::Number(n) if n.as_f64() == Some(0.) => *v = json!(0),
        Value::Object(m) => {
            for v in m.values_mut() {
                canonical(v)
            }
        }
        Value::Array(a) => {
            for v in a {
                canonical(v)
            }
        }
        _ => {}
    }
}
fn identifier(function: &str, inputs: &Value, refs: &Value) -> String {
    let mut identity = json!({"identity_version":"mobile-calc-v1","calc_version":VERSION,"function":function,"inputs":inputs,"inputs_refs":refs});
    canonical(&mut identity);
    let digest = Sha256::digest(serde_json::to_vec(&identity).unwrap());
    format!("calc-{}", &format!("{digest:x}")[..16])
}
pub fn run(evidence: &Value, previous: &[Value], plan: &Value) -> Value {
    let Some(nodes) = plan.as_array().filter(|s| s.len() <= MAX_NODES) else {
        return json!({"records":[],"issues":["计算计划须为至多十二节点的列表"],"verified":false});
    };
    if serde_json::to_vec(plan).map_or(true, |s| s.len() > 64 * 1024) {
        return json!({"records":[],"issues":["计划超过六十四 KiB"],"verified":false});
    }
    let mut ids = HashSet::new();
    let mut pending = BTreeMap::new();
    for node in nodes {
        let id = node["id"].as_str().unwrap_or("");
        if id.is_empty()
            || id.len() > 40
            || !id.starts_with(|c: char| c.is_ascii_alphabetic())
            || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || !ids.insert(id.to_owned())
            || node.as_object().is_none_or(|o| {
                o.keys()
                    .any(|k| !["id", "function", "args"].contains(&k.as_str()))
            })
        {
            return json!({"records":[],"issues":["节点 id 非法/重复，或含目录外字段"],"verified":false,"rejected_plan":plan});
        }
        pending.insert(id.to_owned(), node.clone());
    }
    let mut available = BTreeMap::new();
    for record in previous {
        if let Some(id) = record["calculation_id"].as_str() {
            available.insert(id.to_owned(), record.clone());
        }
    }
    let mut records = Vec::new();
    let mut issues = Vec::new();
    let mut gaps = Vec::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .find(|(_, node)| {
                node["args"].as_object().is_none_or(|args| {
                    args.values()
                        .filter_map(|v| v["calculation"].as_str())
                        .all(|id| !pending.contains_key(id))
                })
            })
            .map(|(id, _)| id.clone());
        let Some(id) = ready else {
            for (id, node) in pending {
                let output = error("计算依赖成环，节点未执行");
                let cid = identifier(
                    node["function"].as_str().unwrap_or(""),
                    &json!({}),
                    &json!({"bindings":node["args"],"resolved":{}}),
                );
                issues.push(json!({"node":id,"reason":output["reason"]}));
                records.push(json!({"calculation_id":cid,"calc_version":VERSION,"node_id":id,"function":node["function"],"bindings":node["args"],"inputs":{},"inputs_refs":{},"executed":false,"output":output}));
            }
            break;
        };
        let node = pending.remove(&id).unwrap();
        let function = node["function"].as_str().unwrap_or("");
        let mut args = json!({});
        let mut refs = json!({});
        let outcome = (|| -> R<Value> {
            let desc = descriptor(function).ok_or("计算函数不在目录内")?;
            let bindings = node["args"].as_object().ok_or("节点 args 必须是对象")?;
            for (key, binding) in bindings {
                if !desc.params.contains(&key.as_str()) {
                    return Err("计算参数不在目录内".into());
                }
                let (value, reference) = resolve(
                    binding,
                    key,
                    desc.config.contains(&key.as_str()),
                    evidence,
                    &available,
                )?;
                args[key] = value;
                if let Some(reference) = reference {
                    refs[key] = reference;
                }
            }
            if refs.as_object().is_none_or(|o| o.is_empty()) {
                return Err("计算节点至少需要一个真实来源".into());
            }
            if serde_json::to_vec(&args).map_or(true, |s| s.len() > 96 * 1024) {
                return Err("单节点来源输入超过九十六 KiB".into());
            }
            Ok(execute(function, &args))
        })();
        let executed = outcome.is_ok();
        let output = outcome.unwrap_or_else(error);
        let cid = identifier(
            function,
            &args,
            &json!({"bindings":node["args"],"resolved":refs}),
        );
        let mut record = json!({"calculation_id":cid,"node_id":id,"function":function,"calc_version":VERSION,"bindings":node["args"],"inputs":args,"inputs_refs":refs,"executed":executed,"output":output});
        if serde_json::to_vec(&record).map_or(true, |s| s.len() > 220 * 1024) {
            record["output"] = error(
                "节点产物超过完整审计的预算，未接受该结果；请缩小输入窗口。没有截断已接受报告。",
            );
            record["output_rejected"] = json!(true);
        }
        if record["output"]["status"] == "error" {
            issues
                .push(json!({"node":id,"calculation_id":cid,"reason":record["output"]["reason"]}));
        } else if record["output"]["status"] == "not_meaningful" {
            gaps.push(json!({"node":id,"calculation_id":cid,"reason":record["output"]["reason"]}));
        }
        available.insert(id, record.clone());
        available.insert(cid, record.clone());
        records.push(record);
    }
    json!({"records":records,"issues":issues,"gaps":gaps,"verified":issues.is_empty()})
}
fn same_output(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => {
                (a - b).abs() <= f64::EPSILON * 64. * a.abs().max(b.abs()).max(1e-280)
            }
            _ => false,
        },
        (Value::Object(a), Value::Object(b)) => {
            a.keys().eq(b.keys())
                && a.iter()
                    .all(|(k, a)| k == "reason" || b.get(k).is_some_and(|b| same_output(a, b)))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_output(a, b))
        }
        _ => a == b,
    }
}
/// Recheck restored records before showing or using their numerical output.
pub fn verify_record(record: &Value) -> R<()> {
    if !record.is_object() || record["calc_version"] != VERSION {
        return Err("计算记录格式或函数版本不支持".into());
    }
    let function = record["function"].as_str().ok_or("计算记录缺少函数名")?;
    let cid = identifier(
        function,
        &record["inputs"],
        &json!({"bindings":record["bindings"],"resolved":record["inputs_refs"]}),
    );
    if record["calculation_id"] != cid {
        return Err("计算记录的输入/依赖身份校验失败".into());
    }
    if record["executed"] == true && record["output_rejected"] != true {
        let actual = execute(function, &record["inputs"]);
        if !same_output(&actual, &record["output"]) {
            return Err("保存的计算输出与重新计算不一致".into());
        }
    } else if record["output"]["status"] != "error" || !record["output"]["value"].is_null() {
        return Err("未执行或未接受的节点不能含有效数值".into());
    }
    Ok(())
}
pub fn contract() -> Value {
    json!({"maximum_nodes":MAX_NODES,"node":{"id":"本阶段唯一英文标识","function":"计算目录函数名","args":{"参数名":{"evidence":"E1","path":"/源数据路径"},"依赖参数":{"calculation":"前序节点id或历史calc-id","path":"/value"},"序列参数":{"evidence":"E2","path":"/源数组","columns":{"period":"/period","value":"/values/科目名"}}}},"rules":"数值和序列必须绑定本轮证据或前序计算；仅 literal_parameters 允许声明单位、周期等配置。支持 column 标量投影和 where JSON指针精确筛选。记录原始绑定、已解析输入、来源、版本、依赖和 display；error/not_meaningful 不得当数值引用。单位配置必须与证据口径一致；计划不执行任何代码、文件或网络。"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ytd_chain_is_topological_bound_and_repeatable() {
        let e = json!([{"id":"E1","source":"synthetic","data":{"reports":[{"period":"2026-03-31","profit":10},{"period":"2026-06-30","profit":35}],"cap":1000}}]);
        let nodes = json!([
            {"id":"pe","function":"pe_deducted_annualized","args":{"total_market_cap":{"evidence":"E1","path":"/cap"},"cap_unit":"亿元","latest_quarter_deducted_profit":{"calculation":"latest","path":"/value"},"profit_unit":"亿元"}},
            {"id":"latest","function":"latest_quarter","args":{"single_quarters":{"calculation":"quarters","path":"/details/series"},"unit":"亿元","money":true}},
            {"id":"quarters","function":"quarterize","args":{"cumulative":{"evidence":"E1","path":"/reports","columns":{"period":"/period","value":"/profit"}},"unit":"亿元","money":true}}
        ]);
        let result = run(&e, &[], &nodes);
        assert_eq!(result["verified"], true, "{result}");
        let records = result["records"].as_array().unwrap();
        assert_eq!(records.last().unwrap()["output"]["value"], 10.);
        let repeated = run(&e, &[], &nodes);
        assert_eq!(result, repeated);
        let mut changed = e.clone();
        changed[0]["data"]["cap"] = json!(1200);
        assert_ne!(
            result["records"][2]["calculation_id"],
            run(&changed, &[], &nodes)["records"][2]["calculation_id"]
        );
    }
    #[test]
    fn invented_inputs_unknown_refs_cycles_and_nonmeaningful_sources_never_become_numbers() {
        let e = json!([{"id":"E1","data":{"n":12}}]);
        for args in [
            json!({"numerator":12,"denominator":2}),
            json!({"numerator":{"evidence":"E9","path":"/n"},"denominator":{"evidence":"E1","path":"/n"}}),
            json!({"numerator":{"calculation":"a","path":"/value"},"denominator":{"evidence":"E1","path":"/n"}}),
        ] {
            let result = run(&e, &[], &json!([{"id":"a","function":"ratio","args":args}]));
            assert_eq!(result["verified"], false, "{result}");
        }
    }
}
