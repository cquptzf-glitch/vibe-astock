#[cfg(test)]
mod tests {
    use serde_json::json;
    fn fixture() -> crate::AppCore {
        crate::AppCore::new(crate::test_http::temp_root("accounting"))
        .unwrap()
    }
    #[test]
    fn imported_opening_split_partial_sell_fifo_backup_and_restore_agree() {
        let app = fixture();
        let db = app.db_path();
        let csv="日期,类型,代码,数量,价格,费用,比例\n2026-01-05,期初持仓,600519,100,20,,\n2026-01-06,送转拆股,600519,,,,2\n2026-01-07,买入,600519,100,12,0,\n2026-01-08,卖出,600519,150,15,0,";
        let preview = crate::trade_import::preview(db, &json!({"text":csv})).unwrap();
        assert_eq!(preview["summary"]["problems"], 0);
        crate::trade_import::import(db, &json!({"text":csv})).unwrap();
        let rows = crate::journal_risk::load_entries(db).unwrap();
        let mut fees = crate::journal_settings::DEFAULT_FEES;
        fees.fifo = true;
        let r = crate::journal_risk::settle(rows[0].clone(), &fees, 0);
        assert_eq!(r.open_shares, 150.0);
        assert!((r.avg_cost - 1700.0 / 150.0).abs() < 1e-8);
        assert_eq!(
            crate::journal::view(db).unwrap()["rows"][0]["open_shares"],
            150.0
        );
        assert_eq!(
            crate::journal_problems::view(db).unwrap()["counts"]["oversell"]
                .as_u64()
                .unwrap_or(0),
            0
        );
        let backup = crate::backup::export(db).unwrap();
        let app2 = fixture();
        crate::backup::restore(
            app2.db_path(),
            &json!({"mode":"replace","confirm":true,"backup":backup}),
        )
        .unwrap();
        assert_eq!(
            crate::journal::view(db).unwrap()["rows"][0]["fills"],
            crate::journal::view(app2.db_path()).unwrap()["rows"][0]["fills"]
        );
        let rerun = crate::trade_import::preview(db, &json!({"text":csv})).unwrap();
        assert_eq!(rerun["summary"]["importable"], 0);
    }
    #[test]
    fn split_does_not_create_cash_or_a_false_return_jump() {
        use std::collections::{BTreeMap, HashMap};
        let app = fixture();
        let db = app.db_path();
        crate::journal::act(db,&json!({"action":"record","symbol":"600519","fills":[{"type":"opening","date":"2026-01-05","price":20,"shares":100},{"type":"split","date":"2026-01-06","price":2,"shares":1},{"type":"sell","date":"2026-01-07","price":11,"shares":200,"fee":0}]})).unwrap();
        let fees = crate::journal_settings::DEFAULT_FEES;
        let rows =
            crate::journal_risk::rows_of(crate::journal_risk::load_entries(db).unwrap(), &fees);
        let flows = crate::portfolio_perf::flows_of(&rows, &[], &fees);
        let closes = HashMap::from([(
            "600519".into(),
            BTreeMap::from([
                ("2026-01-05".into(), 20.0),
                ("2026-01-06".into(), 10.0),
                ("2026-01-07".into(), 11.0),
            ]),
        )]);
        let out = crate::portfolio_perf::compute(&flows, &closes, vec![]);
        assert_eq!(rows[0].realized_pnl, Some(200.0));
        assert!(out.to_string().contains("200"));
        assert!(
            out["warnings"].as_array().is_none_or(Vec::is_empty),
            "{out}"
        );
    }
    #[test]
    fn migration_preserves_deleted_fill_id_high_watermark() {
        let app=fixture();let db=app.db_path();let saved=crate::journal::act(db,&json!({"action":"record","symbol":"600519","fills":[{"type":"buy","date":"2026-01-05","price":20,"shares":100}]})).unwrap();
        let conn=rusqlite::Connection::open(db).unwrap();conn.execute("UPDATE sqlite_sequence SET seq=100 WHERE name='journal_fills'",[]).unwrap();conn.pragma_update(None,"user_version",12).unwrap();drop(conn);
        crate::schema::migrate(db).unwrap();crate::journal_mutation::append_fills(db,&json!({"id":saved["result"]["id"],"fills":[{"type":"sell","date":"2026-01-06","price":21,"shares":100}]})).unwrap();
        let conn=rusqlite::Connection::open(db).unwrap();let id:i64=conn.query_row("SELECT MAX(id) FROM journal_fills",[],|r|r.get(0)).unwrap();assert_eq!(id,101);
    }
    #[test]
    fn invalid_or_orphan_split_is_rejected_without_writes() {
        let app = fixture();
        let db = app.db_path();
        for ratio in [0.0, -1.0, 1001.0] {
            assert!(crate::journal::act(db,&json!({"action":"record","symbol":"600519","fills":[{"type":"buy","date":"2026-01-05","price":20,"shares":100},{"type":"split","date":"2026-01-06","price":ratio,"shares":1}]})).is_err());
        }
        assert!(crate::journal::act(db,&json!({"action":"record","symbol":"600519","fills":[{"type":"split","date":"2026-01-06","price":2,"shares":1}]})).is_err());
        assert_eq!(
            crate::journal::view(db).unwrap()["rows"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
