# Android / Rust migration parity matrix

## 当前入口：0.1.40 手机布局优化（2026-10-25，本地候选）

本轮代码版本 0.1.40 / Android 配置 versionCode 1040；按维护者选择先完成本地界面预览，不等待 CI/APK。最后有发布回执的 APK 仍为 0.1.38，0.1.39 与本轮 0.1.40 均不能当作已发布安装包。此前 GitHub workflow_dispatch 返回 Actions has been disabled for this repository；本轮未再次调度或修改 Actions 权限。

### 已完成

- 底部保留首页、盯盘、复盘、研究、我的五个主栏目；设置归入“我的 / 应用与数据”，设置页保留返回入口与“我的”高亮。
- 复盘分为市场复盘、复盘记录、交易日志、交易分析；研究分为个股研究、产业资讯。页签只展示当前分组，目录始终可达；全部 42 个目录入口（不含目录自身）各有唯一分组，原有路由未删除。
- 首页前置股票搜索与常用功能，消息可直接打开；市场详情、情绪和自选行情随后展示。目录支持分类、关键词搜索、清除与从当前栏目带入分类。
- 主要导航、搜索和目录筛选即使在紧凑模式仍保留至少 44px 触控区，底栏至少 48px；320px 目录改为单列，保留横竖安全区间距。页面切换回到顶部。
- 问 AI / 分享图改为页面内工具栏，避免悬浮遮住行情。交易日志补边距，把“记一笔”作为主操作置于表格导入前，手机成交表单按两列排布。交易核算、保存逻辑和数据与模型处理未改。

### 本地验证与交付

前端 578 项、Rust Core 312 项 / 28 ignored、符号 parity 1 项及发行脚本 9 项通过；TypeScript、桌面与独立移动构建通过。Edge 四尺寸 320/360/390/768 × 明暗主题：320 张页面截图、24 张实际图表 PNG 和 40 张视口布局预览通过；检查导航触控区、目录分类/搜索/返回、设置归属、320px 表单与宽度、原有行情/研究/回测流程及分享载荷，人工检查明暗主题布局。

本地截图来自公开冻结/合成数据，不是真实行情或用户账户，不等同于真机安装验收。未重跑未改动的编排器/Python 全套；上轮 Windows 契约 5 项失败、Python 环境缺依赖及全量 Rust fmt / 严格 Clippy 未通过的边界保留，不计作本轮通过。SQLite v13 / 业务备份 v11 不变；预览固定签名配置未改。历史数据、研究全量等价、核算边界、源站与真机/正式身份缺口继续保留。


## 0.1.39 图表分享迁移（2026-10-25）

本轮候选版本 0.1.39 / versionCode 1039，本地完成、已推送；GitHub 平台拒绝启动 Actions，未发布。最后已发布版本仍为 0.1.38。

- 个股多周期 K 线、分时/五日分时、回测策略权益曲线支持生成真实 PNG：保留当前可见区间、指标、十字线、主题及来源，先预览再调用系统文件分享。回测曲线明确为历史假设，不是真实账户权益。
- 图片完整保留源站回退、异常数据、缓存失败提示及历史规则覆盖说明；说明超出图片容量时拒绝导出，不静默裁掉缺口。只取显式图表 SVG，不抓整个页面，不加载外部图像。
- K 线平移后无十字线时，信息和导出读数改用最后一根可见 K 线；数据、区间、指标或缓存提示变化后旧预览失效。
- 本地前端 577 项通过，类型与桌面/移动构建通过；Edge 三尺寸 × 明暗主题 216 张截图及 18 张实际 PNG 导出通过。SQLite v13 / 业务备份 v11 不变，交易数据与模型处理保持现状。

图表分享补齐以上三类；其他公开页仍用文字摘要卡。特殊历史制度、行业自动事实源/完整旧研究工作流等价、现金分红核算边界、源站连通/工程整理以及真机/正式身份仍未全部验收。用户已明确暂时没有设备，继续完成 CI 和预览交付。


当前交付：**0.1.38 已发布（2026-10-19）**，功能构建 `b629dc5`；Android #39150050100 和跨平台 #39150049996 全部成功。最终包内版本 0.1.38 / 1038，APK 与固定证书一致。最新限制以 [ANDROID_REMAINING.md](ANDROID_REMAINING.md) 为准；完整最终回执在本文末。

This file is the consolidation gate for the Android migration. The `android-mvp` branch is the single active Android development line. Legacy Node/Python behavior remains an oracle until the matching Rust path has frozen fixture/golden coverage and CI is green.

For current unfinished scope, see `ANDROID_REMAINING.md`; development increments and verification evidence are recorded in `ANDROID_HANDOFF.md`.

### 本地验证（0.1.39）

前端 577 项通过，TypeScript 检查与桌面/独立移动构建通过；Edge 三尺寸 × 明暗主题 216 张截图、18 张实际 PNG 导出通过。核对 PNG 尺寸/签名、原生分享载荷与预览一致、K 线平移读数、指标切换失效、私有/嵌入内容拒绝；人工查看明暗图片。发行脚本 9 项通过。SQLite v13 / 业务备份 v11 保持不变。

改动前本机 Windows 全量编排器基线 1291 通过 / 128 失败 / 14 跳过，类型检查通过；系统 Python 3.14 缺 numpy/mypy，Python 基线未通过。它们不计为本轮验证通过；未改编排器和 Python 业务代码，Linux/macOS 全套及 Windows 契约以新 CI 回执为准。既有全量 Rust fmt / 严格 Clippy 未通过仍保留。

本机补充：Rust Core 312 项通过 / 28 ignored，符号 parity 另 1 项通过。Windows CI 同款契约子集 113 通过 / 5 失败 / 6 跳过，失败为 WorkBuddy 检测及目录联接/清理；未将其计为通过，也未为前端增量改动旧桌面环境路径。

### CI 发布阻塞（2026-10-25）

功能提交 `d35597ff50fc93bd6677294f484a63fc390835de` 已推送到 `android-mvp`。GitHub 未为 push 创建工作流；显式请求 Android workflow_dispatch 返回 HTTP 422：`Actions has been disabled for this repository.`。随后只读核实仓库 Actions 权限为 `enabled=true`、`allowed_actions=all`，工作流为 active；平台拒绝原因尚未确认，未擅自切换仓库权限。

因此 0.1.39 没有 CI 运行编号、APK 或公开 Release；最后已发布版本仍为 0.1.38，不能将本地 577 项/截图检查当作 APK 验收。已请求维护者在 GitHub Actions 页面处理平台限制，解除后对本提交启动 Android（publish_preview=true）与跨平台工作流，核对固定签名和内部版本 0.1.39 / 1039，再补发布回执。恢复构建前确认缓存签名密钥存在，缺失时拒绝换身份。

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

## Current mobile matrix — 0.1.40 local candidate (2026-10-25)

| Area | Mobile implementation | Evidence | Status |
| --- | --- | --- | --- |
| Mobile navigation / layout | five primary tabs, scoped workspace sections, searchable directory, inline page tools, phone journal form | 578 frontend tests; 320 Edge screenshots across four widths and two themes, including 40 layout previews; all 42 directory entries covered once | Implemented and locally verified; device / APK / CI deferred |
| Deterministic calc / evidence DAG | deterministic_calc 0.5.0 (24 functions), graph, research_calculations, CalculationAudit | 388 frozen Python cases with oracle source hashes; reject fabricated inputs/cycles/wrong claims/corrupt restores; cancellation/backup tests; 180-screen browser smoke | Pure function numerical parity verified within fixture scope; bounded mobile JSON DAG implemented; legacy file/CLI identity and full provider workflows not equivalent |
| AI staged research / debate / emotion / weekly / industry | research_workflow + research-methodology.json | resume/cancel/late-result/math rejection/retry/archive tests; browser workflow | Implemented mobile snapshot workflow with evidence-bound calculation plans and durable audits; full desktop source/workflow parity pending |
| Statements / consensus / valuation | research_extended | parser tests; live Sina/THS/Baidu probes | Implemented; provider continuity and real-device UX pending |
| Historical market regimes | historical_metadata / backtest_rules / BacktestRuleAudit | live BaoStock ST + IPO probes; date/currency/status/paging rejection; deterministic regime tests | Implemented SH/SZ daily ST/tradestatus and IPO windows; BJ/relisting/delisting exceptions remain source gaps |
| Industry measurement library | industry_library / IndustryLibrary | atomic import, dedup/conflicts, cutoff, backup, explicit research opt-in; SSR and browser import/save | Implemented user-curated observations across 20 sectors; not an independently verified full-market database |
| Signed distribution / App Links | android-distribution.yml + scripts/android + native owned-host routing | 9 helper tests; native route checks; 0.1.38 CI debug/release APK and AAB fixture smoke passed | Build/configuration implemented; real owner key, hosted domain, store and device acceptance absent |
| Advanced backtest | backtest_advanced / backtest_rules / ai_backtest | deterministic resampling, WFO no future dependence, rebalance cash bounds; live single/multi run | Implemented SH/SZ historical ST/IPO metadata; special relisting/delisting/BJ source coverage remains partial |
| Corporate actions / opening positions | journal_fills v13 + unified settle; backup v11 | FIFO/partial sell/restore/no fake cash flow tests | Implemented; registered splits normalize MFE prices/quantities to exit-share basis; unknown actions and cash-dividend caveats remain |
| Scan / Android service | market_scan + MarketMonitorService | parsing and native source compile | Full-market quote/minute sampling; official month calendar gates native sampling and rollover stops; exchange tick feed and device lifecycle unverified |
| Calendar / event reminders | calendar_sources + native rules | complete-month validation, reminder/backup/done state tests; live SZSE | Implemented SZSE month semantics; special exchange-specific coverage pending |
| General AI chat | ai_chat + AiChatPanel / SQLite threads | native streaming/cancel/history and frontend regressions; cross-platform CI | Implemented streaming, stop, threads and bounded tools; real model/device quality not certified |
| Documents / controlled chat | document_import + chat_tools/chat_documents | file parsers, arithmetic round trip, budget, citation refusal tests | Implemented bounded retrieval and original PDF-page citations (blank pages preserved, wrong/unrecalled pages rejected); scanned PDFs/OCR unsupported |
| Public-page assistant / share / offline | PublicPageActions / provider_cache | SSR/Edge and offline API tests; manual refresh bypass and failed-envelope fallback regression | Implemented text-summary cards and labeled historical cache; K-line/intraday/backtest SVG chart PNG previews and exact native share payloads added in 0.1.39 |
| Supply-chain / event probability | industry_revenue / probability / Kalshi GPU ladder | parsing tests; quote types, source gaps explicit | Implemented bounded scans; Kalshi/Polymarket transport failed locally; four live monthly-revenue providers passed |
| Independent mobile artifact | mobileMain + Vite mobile pre-transform | builds + 320-screen / 24-PNG Edge smoke; current CI deferred by maintainer | 0.1.40 local frontend/layout verification passed; APK and CI deferred; 0.1.38 remains the last published preview; device acceptance pending |

Previous 0.1.36 verification (historical): Rust 300 + symbol parity 1; frontend 569; frozen Python oracle 388 cases; Edge screenshots 180; desktop/mobile builds pass. Android CI #39123017859 passed and published the preview; cross-platform fixture follow-up is recorded below.

Validation remains scoped: local Core/frontend checks do not certify Android lifecycle, official signing, full legacy parity or all public providers. See ANDROID_REMAINING.md for outstanding items and release receipts.


<details>
<summary>Earlier baseline matrix (historical; superseded rows use the current matrix above)</summary>

## Matrix

| Area | Legacy oracle | Active implementation | Verification | Status |
| --- | --- | --- | --- | --- |
| Stock symbol normalization / SH-SZ-BJ routing | `.agents/skills/data-access/scripts/common.py` (`norm_ticker`, `natural_market`, `tencent_code`, `em_secid`) blob `2021bb1f4706acf597d51bb497e35410a3baad83` | `core-rust/src/symbol.rs` | `legacy_symbol_parity.json` + `symbol_parity.rs` | **Verified / migrated from old refactor branch** |
| Realtime quote | Legacy `fetch_quote.py` | `core-rust/src/quote.rs` | Current Rust coverage; frozen provider parity still to be ported | **Migrated, parity hardening pending** |
| Daily qfq K-line | Legacy `fetch_kline.py` | `core-rust/src/quote.rs` | Current Rust coverage; frozen provider parity still to be ported | **Migrated, parity hardening pending** |
| Stock chart (multi-period K-line, intraday, snapshot with order book) | PC 个股深挖 K 线 / brokerage-app conventions | `core-rust/src/chart.rs` (`/mobile/stock/{kline,intraday,snapshot}`) + `KlineChart.tsx`, `QuoteBoard.tsx`, `chartIndicators.ts` | Rust parser tests on real Eastmoney/Tencent samples + ignored live probe (all periods, BJ, Tencent fallback); indicator formula tests; rendering on frozen real responses; browser smoke with tap/drag/indicator/intraday interactions | **Implemented and preview released (0.1.21, CI #38989460626); Eastmoney↔Tencent fallback; device checks pending** |
| Symbol search | No equivalent legacy runtime contract | `core-rust/src/search.rs` using shared symbol routing | Rust unit tests + Tencent Smartbox parsing | **Rust-only extension** |
| Market breadth | Legacy market/data scripts | `core-rust/src/market.rs` | Rust Core + Android CI | **Migrated** |
| Limit-up / consecutive limit | Legacy shortline endpoints | `core-rust/src/shortline.rs` | Rust Core + Android CI | **Migrated** |
| Emotion review · 今日情绪 (`cn_shortline_emotion` / `_facts` / `_board_ladder` / `_firstboard` / `_cycle`) | `shortline_emotion.py`, `shortline_facts.py`, `shortline_boards.py` | `core-rust/src/emotion.rs` + `EmotionPanels.tsx` (`/mobile/shortline/emotion`, `/mobile/shortline/cycle`) | Rust unit tests on frozen pools + ignored live probe; frontend rendering on a frozen real 2026-10-16 response; browser smoke | **Migrated for mobile; themes use Eastmoney industry (PC uses THS reasons), loss/feedback settled-only** |
| Emotion review · 近 5 天 / 情绪历史 / 明日验证 | `orchestrator/src/finance/shortline/{weekly,history,verification,verify_metrics}.ts` | `core-rust/src/emotion_history.rs` + `HistoryPanels.tsx` (`/mobile/shortline/{weekly,history,verify}`, POST verify); SQLite v5 `mobile_verifications` (backed up) + `mobile_shortline_days` cache | Rust unit tests (readings, percentile/extremes, noise gate, direction/baseline/late/summary, save transaction, cache round-trip) + ignored live probe; frontend rendering on frozen real responses; browser smoke; Android CI #38983459066 passed | **Migrated for mobile (0.1.20 preview); rotation by Eastmoney industry; history starts at Eastmoney retention (~28 days) and grows from local cache** |
| Emotion review · 涨停样本 | `orchestrator/src/finance/shortline/samples.ts` (v2.0.0 `duanxian/backtest.py`) | `core-rust/src/samples.rs` + `SamplesPanel.tsx` (`/mobile/shortline/samples`); SQLite v6 `mobile_samples_days` cache (public data, not backed up) | Rust unit tests (stats nulls, 7 strategy filters, seal-time buckets, regime thirds, equity compounding, rule-of-day limit-up judgement, cache round-trip) + ignored live probe (14 days / 786 samples, second call from cache 148 ms); frontend rendering on a frozen real response; browser smoke | **Implemented and preview released (0.1.22, CI #39002403555); after-the-fact caveat always shown; window starts at Eastmoney retention and grows from local cache** |
| Emotion review · AI 复盘 / AI 周报 | PC `/shortline/:tab` views | Not migrated | None | **Pending** |
| Emotion review · 题材事件树 + 首板涨停原因 | `orchestrator/src/finance/shortline` theme tree over THS limit-up reasons | `core-rust/src/theme_tree.rs` + `ThemeTree.tsx` (`GET /mobile/shortline/theme-tree`); SQLite v7 `mobile_ths_reasons` cache (public data, not backed up) | Rust unit tests (reason cleaning, tag split, paging/date check, tree states) + ignored live probe (59/61 reasons covered); frontend rendering on a frozen real response; browser smoke | **Implemented (0.1.23); reasons come from THS editors; whole tree marked unavailable when no reason string is reachable (never falls back to industries)** |
| Emotion review · 涨停分档 (`review_tiers`) | PC review tiers block | `core-rust/src/review_tiers.rs` + `ReviewTiers.tsx` (`GET /mobile/review/tiers`), shown in the market panel | Rust unit tests + frozen real response + browser smoke | **Implemented (0.1.23)** |
| Emotion review · 结构变化 | PC `/shortline/structure` (field drift, regime events) | `core-rust/src/structure.rs` + `StructurePanel.tsx` (`GET/POST /mobile/review/structure`); SQLite v7 `mobile_structure_days` cache (not backed up) + `mobile_regime_events` (backed up) | Rust unit tests (window medians, shift thresholds, field drift, event validation, backup round-trip incl. older backups) + frozen real response (only 4 prior days available, comparison honestly unavailable) + browser smoke | **Implemented (0.1.23); market comparison needs enough locally cached days (Eastmoney keeps ~1 month); regime events are manual only** |
| Emotion review · 昨日炸板股修复 + 反馈矩阵「昨日炸板」 | PC `loss_effect.prev_broken_recovery` (computation source not found in repo; semantics taken from UI fields) | `core-rust/src/prev_broken.rs` + `PrevBroken.tsx` (`GET /mobile/shortline/prev-broken`) | Rust unit tests (result buckets via limit prices, review-day-only quotes, coverage) + ignored live probe + frozen real response (7 stocks, 2026-10-16) + browser smoke | **Implemented (0.1.23) as a separate card; not reconciled item-by-item against PC; unavailable when the quote snapshot is not the review day** |
| 盯盘 · 实时动态 (alert feed, yesterday's ladder, large caps, turnover top 10) | `orchestrator/src/finance/shortline/watch.ts`, `shortline_watch.py` | `core-rust/src/watch.rs` + `MonitorPanel.tsx` (`POST /mobile/watch/poll`, `GET /mobile/watch/alerts`); SQLite v6 `mobile_watch_alerts` (30 days) + `mobile_watch_turnover` (public data, not backed up) | Rust unit tests (phases, 3-minute surge rule + cooldown, seal/open, Tencent parsing, trading-day/quote-day, Sina lists, ladder rejection of empty/bad pools, holdings merge, SQLite round-trip) + ignored live probe; frontend rendering on a frozen real response; browser smoke | **Implemented and preview released (0.1.22, CI #39002403555); foreground-only polling; turnover top 10 is snapshotted after close from Sina (PC uses the TDX package)** |
| 盯盘 · 竞价与盘中路径 | `orchestrator/src/finance/shortline/intraday.ts` | `core-rust/src/intraday.rs` + `IntradayPanel.tsx` (`GET/POST /mobile/intraday`); SQLite v6 `mobile_intraday_snapshots` / `_status` (public data, not backed up) | Rust unit tests (slot windows, open-vs-latest basis, stale-quote rejection, live counts with missing pools, insert-only snapshots, trading-day rules) + ignored live probe (weekend); frontend rendering; browser smoke | **Implemented and preview released (0.1.22, CI #39002403555); slots only saved while the app is open on the watch pages; no background scheduler** |
| 复盘 · 每日复盘市场面板 | `page_queries.ts` review blocks (`em_market_trend`, `em_market_margin`, `chinabond_yield_curve`, `em_daily_dragon_tiger`, `sina_board_flow`, `sina_turnover_rank`, global quotes) | `core-rust/src/market_panel.rs` + `MarketPanel.tsx` (`GET /mobile/review/market`) | Rust unit tests (trend maths, partial-day turnover, provider parsers, ChinaBond header guard, snapshot-day rule) + ignored live probe (all 9 blocks); frontend rendering on a frozen real response; browser smoke | **Implemented and preview released (0.1.22, CI #39002403555); CSI 300 PE/dividend yield + bond spreads added in 0.1.23 (CSIndex xls via calamine); sector flow uses Sina basis** |
| 交易日志 · 风险与纪律 + 设置 | `orchestrator/src/finance/journal/{risk,stats,episodes,defs,fees}.ts` | `core-rust/src/journal_risk.rs`, `journal_settings.rs` + `JournalRiskPanel.tsx` (`GET /mobile/journal/risk`, `GET/POST /mobile/journal/settings`); SQLite v6 `mobile_journal_settings` (backed up) | Rust unit tests (moving-average settlement, equity/drawdown, rolling windows, discipline, per-rule checked/unchecked, half-open position spans, at-risk floor, grouping, settings validation, backup round-trip incl. legacy backups); frontend rendering on a **synthetic** journal fixture; guard test that the page registers no AI context; browser smoke | **Implemented and preview released (0.1.22, CI #39002403555); only closed records count as realized (same as 交易统计); after the release it also excludes non-A-share symbols like the desktop journal (merged, unreleased)** |
| 交易日志 · 模式卡 | `orchestrator/src/finance/journal/modes.ts` + `view.ts` (`mode_save` / `mode_delete`) | `core-rust/src/journal_modes.rs` + `JournalModesPanel.tsx` (`GET/POST /mobile/journal/modes`); SQLite v7 `mobile_journal_modes` (**backed up**, backup v7) | Rust unit tests (version-in-force lookup incl. same-day, per-version segmentation by entry day and playbook-in-force, comparison only with enough samples, new-version-next-day, rename-only keeps version, over-long text rejected not truncated by character count, backup round trip + v1–v6 restore with no cards + invalid card rejected); frontend rendering on a **synthetic** fixture; no-AI-context guard; browser smoke | **Implemented, merged after the 0.1.22 release (unreleased; schema/backup v7); only closed trades and manual entries have a pnl % to segment** |
| 交易日志 · 复查清单 + 最大浮盈浮亏 | `journal/{inbox,excursion}.ts` | `core-rust/src/journal_inbox.rs`, `journal_excursion.rs` + `JournalExcursionPanel.tsx` (`GET /mobile/journal/inbox`, `GET /mobile/journal/excursion?mode=cache or fetch`); SQLite v7 `mobile_journal_bars` (public daily bars, forever cache, not backed up) | Rust unit tests (MFE/MAE/capture/certain ranges, give-back, summary medians and bias note, fetch once → cached forever → re-fetch only if cached before the sell-day close, per-trade failure isolation, inbox flags relative to the user's own medians and rules, cache-only inbox) + ignored live Tencent probe; frontend on synthetic fixture; no-AI-context guard; browser smoke | **Implemented, merged after the 0.1.22 release (unreleased; schema/backup v7); closed trades only; no split ledger so >21% one-day close drops mark a suspected split (large splits only)** |
| 交易日志 · 入场环境分组 | `journal/environment.ts` | `core-rust/src/journal_env.rs` + `JournalEnvPanel.tsx` (`GET /mobile/journal/environment`); SQLite v7 `mobile_journal_env_days` (public pools, not backed up) + the emotion-history day cache | Rust unit tests (board position classes incl. pool-unavailable vs non-limit-up, thirds of the user's own days, unsettled entry days left out, pool cache only when both pools complete, gap lines by reason) + ignored live probe against real upstream (second call from cache 55 ms); frontend on synthetic fixture; browser smoke | **Implemented, merged after the 0.1.22 release (unreleased; schema/backup v7); history limited to Eastmoney retention plus local cache** |
| 交易日志 · 看错还是没执行好(判断 × 执行) | `journal/attribution.ts` | `core-rust/src/journal_attribution.rs` + `JournalAttributionPanel.tsx` (`GET /mobile/journal/attribution?date=`), built on `emotion_history::verify_view` track days | Rust unit tests (strict-majority verdict, entry-day alignment, luck cell, zero-pnl days and no-amount trades left out, unavailable until something is reconciled, date parameter rules); frontend on synthetic fixture; browser smoke | **Implemented, merged after the 0.1.22 release (unreleased; schema/backup v7); covers the ~60 trading days the verification page can reconcile** |
| 交易日志 · 问题记录与修复 | `journal/{episodes,rows}.ts` issue lists (orphans / skipped ledger entries / non-A symbols) | `core-rust/src/journal_problems.rs` + `JournalProblemsPanel.tsx` (`GET/POST /mobile/journal/problems`) | Rust unit tests (date-ordered replay, each problem class, edit/delete rules: not worse, at least one fill, entry date follows earliest fill, non-A records not deletable) ; frontend on synthetic fixture; browser smoke | **Implemented, merged after the 0.1.22 release (unreleased; schema/backup v7); mobile-specific problem classes, not a 1:1 port of the desktop lists** |
| Sector rotation | Legacy rotation endpoints | `core-rust/src/rotation.rs` | Rust Core + Android CI | **Migrated** |
| Watchlist | No equivalent mobile SQLite contract | `core-rust/src/storage.rs` | Rust build/Android UI | **Rust-only extension** |
| Trading journal / fills | Legacy PC journal flows | `journal.rs` + `journal_mutation.rs` | Rust Core + Android UI | **Migrated for mobile** |
| Portfolio projection | Legacy position views | `journal_review.rs` + `PortfolioPanel.tsx` | Derived from fills; no duplicate position DB | **Migrated for mobile** |
| Daily review | Legacy review flows | `daily_review.rs` + `DailyReviewPanel.tsx` | Rust unit tests + local SQLite | **Migrated for mobile; PC feature depth still larger** |
| AI journal review | Legacy provider/review paths | `ai_review.rs` | Payload preview + opt-in scope + Rust/Android CI | **Migrated for mobile** |
| General AI chat | No direct legacy mobile equivalent | `ai_chat.rs` | Local history + explicit-message-only context | **Rust-only extension; streaming/tools/threads pending** |
| Android Keystore credential storage | N/A | `tauri-plugin-vibe-secure-storage` + `secure_ai.rs` | Rust regression coverage + Android build; device smoke still needed | **Integrated; device hardening pending** |
| Android notifications | N/A | Tauri notification plugin + reminder rules/native worker | Permissions UI + WorkManager business notifications; notification targets now routed | **Mobile extension; device smoke pending** |
| File import/export / backup | Legacy desktop flows | `core-rust/src/backup.rs` + Dialog/FS + `SettingsPanel` | schema validation + transactional restore tests; backup v5 adds verification conditions, v6 adds journal fee/rule/account-size settings (v1–v5 still restorable; older backups reset the settings to defaults), v7 adds journal mode cards (v1–v6 still restorable; older backups restore with no cards; the two journal market caches are not backed up) | **Migrated as mobile versioned JSON backup** |
| Android native sharing | Legacy share-card flows are richer | `tauri-plugin-vibe-share` + `nativeShare.ts` | Android build + text share path | **Integrated text sharing; image/share-card parity pending** |
| WorkManager / background scheduler | Desktop scheduler/alerts concepts | reminders.rs + tauri-plugin-vibe-reminders | Rust rule/snapshot tests; native scheduling and notification route JUnit in CI | **Mobile extension; Doze/device timing pending** |
| Intraday watch / abnormal | Legacy PC watch flows | monitor.rs (batch snapshot, kept for the watchlist/portfolio panels) + watch.rs (alert feed, see above) | Batch quote/missing fields/risk-state tests (risk state shared via `monitor::risk_state`) | **Superseded on the 盯盘 page by watch.rs (0.1.22); whole-market abnormal scan still pending** |
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


</details>

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


### 0.1.35 候选兼容性复核（2026-10-18）

Android 时间处理改用 java.util.Calendar / 严格 SimpleDateFormat，避免 minSDK 24/25 调用 API 26 的 java.time；外部导航与时间解析直接编译运行 15 项断言通过。首轮跨平台 CI 仅 mobileMain.tsx 未登记为组装根导致边界断言失败，已登记双平台入口，定向 13 项通过；等待 CI 复跑。174 张浏览器截图复跑通过。本地旧桌面/Python 全套存在失败，不能写作通过；实际跨平台结果后续补记。


### 0.1.35 PDF 原始页码补齐（2026-10-18）

PDF 采用逐页提取，含空白页的原始页序随正文和来源标记一起进入现有业务备份，不改备份格式。检索只返回单页内匹配片段和真实 citation，模型漏引、错页、引用未召回页均拒绝；旧导入文本不猜页码，编辑后按普通文本处理。双页 PDF（第一页空白）生成、导入、备份恢复、检索及错页拒绝端到端测试通过。Rust 288 项 + 1 项 parity、前端 565 项与移动构建通过。273f479 的跨平台六组 CI 已全部通过；本次 PDF 增量仍需后续 CI。台系四家公司真实月营收已跑通，事件概率接口连接重置仍未通过。


### 0.1.35 缓存刷新复核（2026-10-18）

手动刷新明确传 refresh=1，保留原证券、日期与筛选参数，不再被正常 TTL 缓存拦住。HTTP 200 中的 failed/error 结果也触发带陈旧标记的旧公开快照回退，不覆盖好缓存；参数 400 错误不回退掩盖，部分成功仍保留可用数据。新增刷新参数与缓存回退回归；Rust 289 项 + 1 项 parity、前端 566 项、移动构建通过，PDF 增量的 ARM64 目标检查通过。最终 APK 与发布回执待最新 CI。


最终功能提交 `69eb810`：跨平台 [#39098061530](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061530) 六组已全部成功；本机最新候选 174 张截图通过。Android [#39098061563](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061563) 的 APK / 原生测试 / 发布结果继续等待最终回执。


## 0.1.35 最终发布回执（2026-10-18）

0.1.35 已发布：[Android 0.1.35 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.35)（标签固定在 `69eb81066a3e639b34f1431e934cc19bc6df131b`；[Android CI #39098061563](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061563) 的 APK 构建、原生 Gradle 单元测试、APK 签名校验与发布全部成功；[跨平台 #39098061530](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061530) 六组全部成功）。ARM64 APK 29,104,663 bytes（27.76 MiB），SHA256 `262c6b14f16b7b6b5bf58ebce27c344639cd9f7c393de93be993d2f6557cc1ff`。仍为 debug-signed 预览版，未做真机验收。

云端日志已核对：Rust 289 项 + 1 项 parity、前端 566 项、Chromium 174 张截图；Linux 编排器 1429 项通过 / 4 跳过、Python 311 + 583 + 783 项通过、mypy 115 个源文件通过。本机 Edge 同样 174 张截图通过。本节最终回执优先于前面的候选阶段“待 CI”记录；文档回执提交不改变已发布 APK 标签。


0.1.36 首轮跨平台 CI #39123017948 的 Linux / Node 24 并发槽位测试返回 `agent_probe_failed`，Linux / Node 22 已通过。检查发现测试专用 fixture-flags.json 会被重复原地写入，而多个 CLI 探针子进程同时读它，存在读到空/不完整 JSON 的竞态；具体子进程 stderr 未由该错误保留，不能声称日志已证明唯一原因。后续测试修复改为相同内容不重写、变更原子替换；不改产品登录探针与容量逻辑。本机定向 9 项通过、1 项平台跳过，修复后的跨平台 CI #39123695067 六组全部成功，Linux 并发分支已通过。Android APK 仍从功能提交 46fbc52 构建。


## 0.1.36 发布验证回执

0.1.36 已发布：[Android 0.1.36 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.36)，标签固定在功能提交 `46fbc52e176fd48a857600984a302325e5928d6d`。[Android CI #39123017859](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39123017859) 的 APK 构建、原生 Gradle 单元测试、签名校验和发布全部成功。ARM64 APK 30,577,839 bytes（29.16 MiB），SHA256 `69385bf3372d0876573885f48e8b8fa067ed70e78bd5f6f82f7703dea499586b`。仍为 debug-signed 预览版，未做真机验收。

云端 Android 日志核对：388 例 oracle 散列校验、Rust 300 项 + 符号 parity 1 项、前端 569 项、Chromium 180 张截图均通过。本机 Edge 同样 180 张截图通过。首轮跨平台六组有五组成功，Linux / Node 24 的既有并发 fixture 测试失败；测试夹具修复提交 `19d15f3`（不改变 APK 功能）对应[跨平台 CI #39123695067](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39123695067) 的 Windows/macOS/Linux × Node 22/24 六组全部成功。Linux / Node 24 日志核对：原并发槽位测试通过、编排器 1429 项通过 / 4 跳过、前端 569 项通过、mypy 115 个源文件通过、Python 311 + 583 + 783 项通过。本节最终回执优先于前文候选/首轮失败记录；回执文档提交不改变已发布标签。

## 0.1.37 candidate validation

Local Core 310 + symbol parity 1, frontend 574, 198 Edge screenshots, ARM64 check, five distribution helper tests and live BaoStock historical ST/IPO probes pass. Android native JUnit/APK/release-AAB fixture smoke and final publication receipt pending. Existing 388 calc oracle fixtures retained. Formal signing and website verification require the actual owner identity/domain, and device acceptance remains separate.

0.1.37 收尾修复：指数日历不可用时，也用已取得的逐日日状态补齐停牌日期；缺价日不成交、按最近已知收盘结转。新增对应回归测试，最终测试项数以 CI 回执为准。


先前候选提交 7e5dbab 的[跨平台 CI #39143283909](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39143283909) 六组全部成功。Linux / Node 24 日志核对：编排器 1429 项通过 / 4 跳过、前端 574 项通过、mypy 115 个源文件通过、发行帮助脚本 5 项通过、388 例 calc oracle 散列匹配、Python 311 + 583 + 783 项通过。Android APK/release-AAB 验证与发布回执仍待补记。

产业快照预算修复：研究仅附带至多二十条、二十八 KiB 的观测；数值/单位/日期/ID/出处不裁改，摘录至多四百字并标记，省略条数、原有冲突数和无效记录数明确返回。完整观测仍在资料库及备份，避免长摘录使整份来源被排除；不是截断已完成报告。


候选提交 `93336e0` 的[跨平台 CI #39144585337](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39144585337) 六组全部成功；Android CI #39144585327 的 debug APK、原生 JUnit、release APK/AAB 构建完成，但新增签名解析校验失败，发布被阻止。定位为 Build Tools 37 的签名输出格式变更；已补旧/新格式解析及错证书、多签名、来源盖章不能冒充应用证书的回归测试，发行脚本六项通过，待重跑签名及发布回执。


工程检查回执（2026-10-19）：全量 Rust `cargo fmt --check` 未通过（已有格式差异）；`cargo clippy --all-targets --no-deps -- -D warnings` 未通过（库 60 项、含测试 70 项 lint）。这两项不计为通过。Build Tools 37 本机一次性合成签名包验证通过，新证书解析器读取并核对了真实工具输出；不代表设备安装升级验收。


提交 `10d3a87` 的[跨平台 CI #39146642264](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39146642264) 六组成功。Linux / Node 24：编排器 1429 项通过 / 4 跳过、前端 574 项、mypy 115 个文件、发行脚本 6 项、388 例 oracle 散列及 Python 311 + 583 + 783 项均通过。Android CI #39146642305 的 ARM64 debug APK 和原生单元测试已通过，等待 release APK/AAB 签名验证和发布。


0.1.37 已发布（2026-10-19）：[预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.37)，功能提交 `10d3a87a71defced2543c69f23eb512fdeececa5`。[Android CI #39146642305](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39146642305) 构建/发布均成功；一次性测试密钥 release APK/AAB 签名验证通过；[跨平台 #39146642264](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39146642264) 六组成功。Rust 312 + parity 1、前端 574、移动截图 198、原生 JUnit 通过。

APK：30,702,295 bytes；SHA256：`1797176695e6d2217001c2f2856c74e1f89337bdcbb456b134e8f310351255fb`。实际 APK 证书 `1C:D5:71:CD:1D:9D:F5:B3:CC:1D:ED:F8:E4:82:8F:8C:08:07:85:79:DF:7A:39:10:67:6F:BD:67:17:68:75:27`，与缓存守卫证书不同；因此 **0.1.37 尚未建立可保证的后续预览升级身份**。不将守卫证书误写成已发布包证书，不追改已发布版本；0.1.38 显式绑定 Gradle 密钥并核对最终 APK。

0.1.38 在此基础上修复预览升级身份：显式指定 Gradle debug keystore；发布前核对最终 APK 与守卫证书、包名、版本、ARM64 和 debug 属性，校验后上传公开回执。新增两项回归，帮助脚本 8 项通过，等待 CI。


0.1.38 候选 `e448383` 的跨平台 #39148052991 六组成功；Android #39148053065 的 debug/release APK、AAB、原生 JUnit 和测试签名验证通过，最终包证书与守卫一致（7C:2B:B8:8E:E4:E1:C6:20:29:93:5B:92:6F:F0:0F:2C:43:2C:5C:B7:76:AE:1E:8E:95:29:12:78:13:B2:35:8F），但新增版本检查阻止发布。直接检查已发布 0.1.37 的二进制 manifest，确认内部仍是默认 versionName=1.0 / versionCode=1。修复为在 tauri.conf.json 显式填写 0.1.38；构建前与 Cargo 对照、最终 debug/release APK 核对 0.1.38 / 1038；帮助脚本 9 项通过，待新 CI。以后升级必须同步 tauri.conf.json 与两处 Cargo 版本。


## 0.1.38 最终发布回执

0.1.38 已于 2026-10-19 发布：[Android 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.38)。实际功能构建提交：`b629dc5c18b4f7271122d990043bb8bf8300415b`。

- [Android CI #39150050100](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39150050100)：build-apk / publish-preview 均成功。ARM64 debug APK、原生 JUnit、一次性测试密钥 release APK/AAB 签名与版本检查通过；最终预览 APK 证书与缓存守卫/固定公开指纹一致。
- [跨平台 CI #39150049996](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39150049996)：六组全部成功。Linux / Node 24 日志核对：编排器 1429 项通过 / 4 跳过；前端 574 项；mypy 115 个源文件；发行脚本 9 项；388 例 calc oracle 散列匹配；Python 311 + 583 + 783 项通过。
- Rust：312 项通过、28 项 ignored，符号 parity 另 1 项通过；移动端截图 198 张通过。原生本机路由/时区 20 项断言的既有回执保留。
- APK：`vibe-astock-android-mvp-v0.1.38-arm64.apk`，30,700,727 bytes（29.28 MiB）；SHA256：`187757bc658a0b033abc11edf70c52aee646345742518015705e49d7e3899ef3`。
- Android 内部包名 `com.vibeastock.mobile`，versionName `0.1.38`，versionCode `1038`。发布后又从公开 APK 中独立提取二进制 manifest，用 aapt 核对相同字段及 debug 属性；不只检查文件名或 Cargo 版本。
- 最终 APK / 守卫证书 SHA256：`7C:2B:B8:8E:E4:E1:C6:20:29:93:5B:92:6F:F0:0F:2C:43:2C:5C:B7:76:AE:1E:8E:95:29:12:78:13:B2:35:8F`；公开固定值见 [preview-signing.json](docs/android/preview-signing.json)。缓存密钥缺失或证书不匹配即停止发布，不静默换身份。

0.1.37 及以前未建立同一可保证的预览签名身份，且 0.1.37 包内版本仍为默认 1.0 / 1；迁移前先导出业务备份。0.1.38 的证书与版本检查通过不代表真机覆盖升级验收。正式维护者密钥、商店、自有域名 App Links、真实设备仍未验收。用户本轮已明确暂时没有设备，先完成 CI 和预览交付；不再以设备连接阻塞本轮。

全量 Rust fmt / 严格 Clippy 检查仍未通过（库 60 项、含测试 70 项 lint），不计为 CI 已通过检查。交易数据与模型处理保持原状；SQLite v13 / 业务备份 v11 不变。
