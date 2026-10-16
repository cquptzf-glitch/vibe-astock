# Discovery fixtures

- `news_atom.xml` and `news_rss.xml`: frozen inputs from the legacy Python offline cases in `.agents/skills/data-access/scripts/tests/test_rss_feed.py`. Rust tests assert the same decoded titles and summary, and additionally prefer the Atom alternate link.
- `gpu_history.json`: synthetic Prometheus matrix based on the legacy `sources/industry.py` history/spot contract. Covers timestamps in seconds, NaN filtering, latest-point equality, and 7-day comparison. It is not a captured current market quote.
- `board_members.json`: public Eastmoney response captured on 2026-10-16 from `/api/qt/clist/get`, `fs=b:BK0475`, `pz=2`, `fltt=2`. Includes a total of 42 with only two returned rows, so tests cannot confuse page breadth with the full board. Contains only public market data.

- `market_daily_flow.json`: public 600519 Eastmoney `/api/qt/stock/fflow/daykline/get` response captured on 2026-10-16, secid=1.600519, lmt=5. Values are yuan; latest missing-value regression additionally freezes the synthetic legacy `test_audit_round3_sources.py::test_b3_fund_flow_latest_dash_is_missing_not_zero`.
- `market_minute.json`: same stock, `/api/qt/stock/fflow/kline/get`, klt=1, captured 2026-10-16; only the final 3 original rows are retained. These are cumulative intraday amounts, not incremental flows.
- `market_boards.json`: public Eastmoney `/api/qt/slist/get`, secid=1.600519, spt=3, pz=200, fltt=2, captured 2026-10-16, all 30 returned associations preserved. Includes missing f3 values.
- `market_anomaly.json`: public `dycalchis.eastmoney.com/price-anomaly/list`, pageSize=5/pageNo=1, captured 2026-10-16. Legacy `sources/eastmoney.py::em_price_anomaly` defines rule mapping; tests keep its o=2 historical marker.
- `market_monitor.json`: first 3 original rows of public `mobappconfig.securities.eastmoney.com/emcfg/stock_monitor.json`, captured 2026-10-16. Dates start on 2026-10-19, intentionally demonstrating upcoming versus active status.
- `market_kline.json`: public Eastmoney `/api/qt/stock/kline/get`, secid=1.600519, klt=101, fqt=1, lmt=65, captured 2026-10-16; prices are qfq, volume remains source units and ratios use the same series.

All `market_*.json` files contain public market data only, not user trades or account information. Unit tests do not call live providers.

Normal CI is offline for provider assertions. Live availability remains distinct from frozen parser compatibility and Android device verification.
