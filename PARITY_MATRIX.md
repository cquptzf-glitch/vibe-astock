# Android / Rust migration parity matrix

This file is the consolidation gate for the Android migration. The `android-mvp` branch is the single active Android development line. Legacy Node/Python behavior remains an oracle until the matching Rust path has frozen fixture/golden coverage and CI is green.

For current unfinished scope, see `ANDROID_REMAINING.md`; development increments and verification evidence are recorded in `ANDROID_HANDOFF.md`.

## Branch consolidation

The earlier `refactor/tauri-rust-android` branch explored a parallel Rust/Tauri architecture. Reusable work from that branch was migrated selectively into `android-mvp`; parallel transport shells or duplicate market stacks must not be restored.

## Deletion gate for legacy runtime code

Legacy code can only be removed after all relevant rows are **Verified** and these checks pass together:

```text
Legacy tests PASS
Rust unit/integration tests PASS
Parity fixtures/goldens PASS
Desktop CI PASS
Android APK PASS
```

## Matrix

| Area | Legacy oracle | Active implementation | Verification | Status |
| --- | --- | --- | --- | --- |
| Stock symbol normalization / SH-SZ-BJ routing | `.agents/skills/data-access/scripts/common.py` (`norm_ticker`, `natural_market`, `tencent_code`, `em_secid`) blob `2021bb1f4706acf597d51bb497e35410a3baad83` | `core-rust/src/symbol.rs` | `legacy_symbol_parity.json` + `symbol_parity.rs` | **Verified / migrated from old refactor branch** |
| Realtime quote | Legacy `fetch_quote.py` | `core-rust/src/quote.rs` | Current Rust coverage; frozen provider parity still to be ported | **Migrated, parity hardening pending** |
| Daily qfq K-line | Legacy `fetch_kline.py` | `core-rust/src/quote.rs` | Current Rust coverage; frozen provider parity still to be ported | **Migrated, parity hardening pending** |
| Symbol search | No equivalent legacy runtime contract | `core-rust/src/search.rs` using shared symbol routing | Rust unit tests + Tencent Smartbox parsing | **Rust-only extension** |
| Market breadth | Legacy market/data scripts | `core-rust/src/market.rs` | Rust Core + Android CI | **Migrated** |
| Limit-up / consecutive limit | Legacy shortline endpoints | `core-rust/src/shortline.rs` | Rust Core + Android CI | **Migrated** |
| Emotion review · 今日情绪 (`cn_shortline_emotion` / `_facts` / `_board_ladder` / `_firstboard` / `_cycle`) | `shortline_emotion.py`, `shortline_facts.py`, `shortline_boards.py` | `core-rust/src/emotion.rs` + `EmotionPanels.tsx` (`/mobile/shortline/emotion`, `/mobile/shortline/cycle`) | Rust unit tests on frozen pools + ignored live probe; frontend rendering on a frozen real 2026-10-16 response; browser smoke | **Migrated for mobile; themes use Eastmoney industry (PC uses THS reasons), loss/feedback settled-only** |
| Emotion review · 近 5 天 / 情绪历史 / 明日验证 | `orchestrator/src/finance/shortline/{weekly,history,verification,verify_metrics}.ts` | `core-rust/src/emotion_history.rs` + `HistoryPanels.tsx` (`/mobile/shortline/{weekly,history,verify}`, POST verify); SQLite v5 `mobile_verifications` (backed up) + `mobile_shortline_days` cache | Rust unit tests (readings, percentile/extremes, noise gate, direction/baseline/late/summary, save transaction, cache round-trip) + ignored live probe; frontend rendering on frozen real responses; browser smoke | **Migrated for mobile; rotation by Eastmoney industry; history starts at Eastmoney retention (~28 days) and grows from local cache** |
| Emotion review · AI 复盘 / AI 周报 / 涨停样本 / 结构变化 | PC `/shortline/:tab` views | Not migrated | None | **Pending** |
| 盯盘 · 竞价与盘中路径 / 昨日梯队实时 / 大票涨幅 | PC `/watch/intraday`, `shortline_watch` | Not migrated | None | **Pending** |
| Sector rotation | Legacy rotation endpoints | `core-rust/src/rotation.rs` | Rust Core + Android CI | **Migrated** |
| Watchlist | No equivalent mobile SQLite contract | `core-rust/src/storage.rs` | Rust build/Android UI | **Rust-only extension** |
| Trading journal / fills | Legacy PC journal flows | `journal.rs` + `journal_mutation.rs` | Rust Core + Android UI | **Migrated for mobile** |
| Portfolio projection | Legacy position views | `journal_review.rs` + `PortfolioPanel.tsx` | Derived from fills; no duplicate position DB | **Migrated for mobile** |
| Daily review | Legacy review flows | `daily_review.rs` + `DailyReviewPanel.tsx` | Rust unit tests + local SQLite | **Migrated for mobile; PC feature depth still larger** |
| AI journal review | Legacy provider/review paths | `ai_review.rs` | Payload preview + opt-in scope + Rust/Android CI | **Migrated for mobile** |
| General AI chat | No direct legacy mobile equivalent | `ai_chat.rs` | Local history + explicit-message-only context | **Rust-only extension; streaming/tools/threads pending** |
| Android Keystore credential storage | N/A | `tauri-plugin-vibe-secure-storage` + `secure_ai.rs` | Rust regression coverage + Android build; device smoke still needed | **Integrated; device hardening pending** |
| Android notifications | N/A | Tauri notification plugin + reminder rules/native worker | Permissions UI + WorkManager business notifications; notification targets now routed | **Mobile extension; device smoke pending** |
| File import/export / backup | Legacy desktop flows | `core-rust/src/backup.rs` + Dialog/FS + `SettingsPanel` | schema validation + transactional restore tests; backup v5 adds verification conditions, v1–v4 still restorable | **Migrated as mobile versioned JSON backup** |
| Android native sharing | Legacy share-card flows are richer | `tauri-plugin-vibe-share` + `nativeShare.ts` | Android build + text share path | **Integrated text sharing; image/share-card parity pending** |
| WorkManager / background scheduler | Desktop scheduler/alerts concepts | reminders.rs + tauri-plugin-vibe-reminders | Rust rule/snapshot tests; native scheduling and notification route JUnit in CI | **Mobile extension; Doze/device timing pending** |
| Intraday watch / abnormal | Legacy PC watch flows | monitor.rs + MonitorPanel.tsx | Batch quote/missing fields/risk-state tests; 60-second foreground refresh | **Mobile watch implemented; full abnormal parity pending** |
| Industry news | Legacy RSS sources/rss.py and rss_sources.json | news.rs + NewsPanel.tsx | Legacy RSS/Atom fixtures, source failure and time filtering tests | **Mobile extension using shared source catalog** |
| Industry GPU signals | Legacy industry.py GPU thermometer | industry_signals.rs + GpuRentPanel.tsx | Legacy history/spot contract fixture, NaN/stale/conflict tests | **Spot/history migrated; Kalshi forward pending** |
| Stock price/volume observations | Mobile deterministic extension (PC Signals is the GPU thermometer) | observations.rs + StockInsightsPanel.tsx, shared quote K-line | Exact-window/crossover/previous-bar tests + mobile rendering | **Single-stock daily observations implemented; batch/intraday scanner pending** |
| Stock daily/minute fund flow | sources/eastmoney.py and mappers.py | stock_context.rs + StockInsightsPanel.tsx | Public frozen daily/minute samples + legacy missing-latest regression | **Daily and intraday cumulative views migrated; source fallback/cache pending** |
| Anomaly / monitored stocks | sources/eastmoney.py em_price_anomaly/em_stock_monitor | anomalies.rs + RiskPanel.tsx | Public fixtures, BSE/rules/missing/state window tests | **Source risk lists implemented; not full intraday abnormal parity** |
| Stock to board associations | sources/eastmoney.py eastmoney_concept_blocks | stock_context.rs + StockInsightsPanel.tsx | Public 30-association fixture + missing/truncation + route returns | **Reverse association migrated; full classification parity pending** |
| Sector detail / constituents | Eastmoney public board endpoints | boards.rs + BoardDetailPanel.tsx | Public constituent fixture + identity/missing/order tests; mobile route | **Mobile detail/pagination/history implemented; provider parity hardening ongoing** |
| Candidates | Legacy PC candidates flows | research.rs + CandidatePanel.tsx | Basic deterministic mobile candidate route; frozen legacy coverage pending | **Partial mobile implementation** |
| Calendar | Legacy trade/calendar flows | calendar.rs + CalendarPanel.tsx | Local event CRUD tests; full exchange/announcement calendar parity pending | **Mobile extension; partial PC scope** |
| Stock research deterministic data | profile/financials/announcements/estimates/valuation scripts | research.rs + ResearchPanel.tsx | Basic profile, financials, announcements and EPS estimates routes | **Partial; valuation history and research workflow pending** |
| Lite backtest | Python backtest/ | backtest.rs + BacktestPanel.tsx | Rust SMA/bar tests; next-open execution; no taxes/fees/slippage | **Mobile Lite extension; advanced parity pending** |
| Notes / reports / inbox | Legacy desktop local/product flows | documents.rs + LibraryPanel.tsx (separate routes, keyword/category/unread filters) | SQLite CRUD + versioned backup tests + filter unit test | **Mobile local document extension; auto-archive pending** |
| Debate / deep research | Node/Agent/Python workflows | Not migrated | None | **Pending / redesign rather than runtime port** |
| Mobile presentation / settings | Mobile extension | presentation.ts + AppearanceSettings + MobileHeader + shared theme tokens | v0.1.18 CI: 431 frontend tests; browser theme/density/navigation, single-row stock tabs / 3-column metrics and 18 fixture screenshots; Android #38926573218 passed | **Implemented and preview released; device visual/lifecycle checks pending** |
| Mobile information architecture | PC `lib/nav.ts` sidebar | workspace.ts + WorkspaceNavigation.tsx (5 groups, sub-tabs, searchable directory) | Render tests for group tabs / nav / directory; every entry has a route; smoke checks one-row nav and sub-tabs; Android CI #38975425413 passed | **Implemented and preview released (0.1.19); device checks pending** |
| Mobile routes / Back / notification navigation | N/A | mobileRoutes.ts + nativeNavigation.ts + reminder intent handoff | JS history/StrictMode tests, Rust target tests, Kotlin route tests; Android CI and device checks recorded in handoff | **Mobile extension; device smoke pending** |
| Full PC backend contract | `orchestrator/` + Python scripts | Mobile Rust Core only covers selected domains | End-to-end parity suite pending | **Pending** |
| Production Android release | Desktop release is separate | Debug-signed ARM64 APK CI + immutable GitHub preview Release | APK build/signature/SHA256 verification; publishing only after tests | **Preview delivery implemented; production signing/AAB/device upgrades pending** |

## Rules for subsequent migrations

1. `android-mvp` is the only active Android implementation branch.
2. Read `ANDROID_HANDOFF.md` before starting a new migration so already completed work is not duplicated.
3. Freeze unstable/live provider responses into fixtures before replacing mature parsers.
4. Reuse proven behavior only when it fits the active architecture; do not restore duplicate Tauri shells, duplicate frontend transports, or a second quote/K-line stack.
5. Compare canonical output including null/error behavior, market routing, defaults, sort order, timezone handling and numeric precision.
6. Preserve source/provenance metadata separately from UI convenience types.
7. New mobile-only capabilities must be labelled as extensions rather than used as evidence that a legacy PC module is already equivalent.
8. Keep holdings derived from journal fills; do not add a second positions truth store.
9. Keep AI API keys out of SQLite/localStorage and backups; use the Android Keystore boundary.
10. Do not bundle Node or Python runtimes into the production APK to obtain parity.
