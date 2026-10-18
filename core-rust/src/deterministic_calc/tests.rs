use super::*;
fn compare(actual: &Value, expected: &Value, path: &str, issues: &mut Vec<String>) {
    if path.ends_with("/reason") {
        return;
    }
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if (a - b).abs() > 1e-9_f64.max(b.abs() * 1e-9) {
                issues.push(format!("{path}: {a} != {b}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, b) in b {
                if k == "reason" {
                    continue;
                }
                match a.get(k) {
                    Some(a) => compare(a, b, &format!("{path}/{k}"), issues),
                    None => issues.push(format!("{path}/{k} missing")),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                issues.push(format!("{path}: array length"));
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{i}"), issues);
            }
        }
        _ => {
            if actual != expected {
                issues.push(format!("{path}: {actual} != {expected}"));
            }
        }
    }
}
#[test]
fn python_050_frozen_oracle_matches_all_registered_functions() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/calc_parity.json")).unwrap();
    let mut failures = Vec::new();
    let mut functions = std::collections::HashSet::new();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["function"].as_str().unwrap();
        functions.insert(name);
        let actual = execute(name, &case["args"]);
        let expected = &case["output"];
        let mut issues = Vec::new();
        if expected["status"] == "error" {
            if actual["status"] != "error" {
                issues.push(format!("expected error, got {actual}"));
            }
        } else {
            compare(&actual, expected, "", &mut issues);
        }
        if !issues.is_empty() {
            failures.push(format!("{}: {}", case["name"], issues.join("; ")));
        }
    }
    assert_eq!(functions.len(), FUNCTIONS.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn catalog_is_complete_and_money_is_never_guessed() {
    assert_eq!(FUNCTIONS.len(), 24);
    let result = execute(
        "pe_ttm_from_parts",
        &json!({"total_market_cap":100,"cap_unit":"港元","ttm_profit":10,"profit_unit":"元"}),
    );
    assert_eq!(result["status"], "error");
    assert_eq!(
        execute(
            "forward_cagr",
            &json!({"eps_t":1,"eps_t_plus_n":2,"years":true})
        )["status"],
        "error"
    );
    assert_eq!(
        execute("growth_rate", &json!({"current":1e308,"base":1e-308}))["status"],
        "error"
    );
    assert_eq!(execute("system", &json!({}))["status"], "error");
    assert_eq!(
        execute(
            "ratio",
            &json!({"numerator":1,"denominator":2,"url":"file:///x"})
        )["status"],
        "error"
    );
}
