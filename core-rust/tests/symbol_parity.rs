use serde::Deserialize;
use vibe_astock_core::normalize_stock_symbol;

#[derive(Debug, Deserialize)]
struct Fixture {
    legacy_source: String,
    legacy_blob_sha: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    input: String,
    expected: Option<Expected>,
    #[serde(default)]
    error: bool,
}

#[derive(Debug, Deserialize)]
struct Expected {
    digits: String,
    market: String,
    normalized: String,
    tencent: String,
    eastmoney: String,
}

#[test]
fn legacy_symbol_contract_is_frozen() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/legacy_symbol_parity.json")).unwrap();

    assert_eq!(
        fixture.legacy_source,
        ".agents/skills/data-access/scripts/common.py"
    );
    assert_eq!(
        fixture.legacy_blob_sha,
        "2021bb1f4706acf597d51bb497e35410a3baad83"
    );

    for case in fixture.cases {
        let result = normalize_stock_symbol(&case.input);
        if case.error {
            assert!(
                result.is_err(),
                "legacy parity case {:?} must be rejected",
                case.input
            );
            continue;
        }

        let actual = result
            .unwrap_or_else(|error| panic!("legacy parity case {:?} failed: {error}", case.input));
        let expected = case
            .expected
            .expect("valid parity case needs expected result");
        assert_eq!(actual.digits, expected.digits, "digits for {}", case.input);
        assert_eq!(
            actual.market.as_str(),
            expected.market,
            "market for {}",
            case.input
        );
        assert_eq!(
            actual.normalized, expected.normalized,
            "normalized for {}",
            case.input
        );
        assert_eq!(
            actual.tencent, expected.tencent,
            "Tencent code for {}",
            case.input
        );
        assert_eq!(
            actual.eastmoney, expected.eastmoney,
            "EastMoney secid for {}",
            case.input
        );
    }
}
