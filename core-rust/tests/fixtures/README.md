# Discovery fixtures

- `news_atom.xml` and `news_rss.xml`: frozen inputs from the legacy Python offline cases in `.agents/skills/data-access/scripts/tests/test_rss_feed.py`. Rust tests assert the same decoded titles and summary, and additionally prefer the Atom alternate link.
- `gpu_history.json`: synthetic Prometheus matrix based on the legacy `sources/industry.py` history/spot contract. Covers timestamps in seconds, NaN filtering, latest-point equality, and 7-day comparison. It is not a captured current market quote.
- `board_members.json`: public Eastmoney response captured on 2026-10-16 from `/api/qt/clist/get`, `fs=b:BK0475`, `pz=2`, `fltt=2`. Includes a total of 42 with only two returned rows, so tests cannot confuse page breadth with the full board. Contains only public market data.

Normal CI is offline for provider assertions. Live availability remains distinct from frozen parser compatibility and Android device verification.
