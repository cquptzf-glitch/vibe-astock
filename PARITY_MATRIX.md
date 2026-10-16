# Android / Rust migration parity matrix

This file is the consolidation gate for the Android migration. The `android-mvp` branch is the single active Android development line. Legacy Node/Python behavior remains an oracle until the matching Rust path has frozen fixture/golden coverage and CI is green.

## Branch consolidation

The earlier `refactor/tauri-rust-android` branch explored a parallel Rust/Tauri architecture. Reusable work from that branch is migrated selectively into `android-mvp`; parallel transport shells or duplicate market stacks are not merged wholesale.

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

| Area | Legacy oracle | Active Rust implementation | Verification | Status |
| --- | --- | --- | --- | --- |
| Stock symbol normalization / SH-SZ-BJ routing | `.agents/skills/data-access/scripts/common.py` (`norm_ticker`, `natural_market`, `tencent_code`, `em_secid`) blob `2021bb1f4706acf597d51bb497e35410a3baad83` | `core-rust/src/symbol.rs` | `legacy_symbol_parity.json` + `symbol_parity.rs` | **Verified / migrated from old refactor branch** |
| Realtime quote | Legacy `fetch_quote.py` | `core-rust/src/quote.rs` | Current Rust unit/build coverage; frozen provider parity still to be ported | **Migrated, parity hardening pending** |
| Daily qfq K-line | Legacy `fetch_kline.py` | `core-rust/src/quote.rs` | Current Rust unit/build coverage; frozen provider parity still to be ported | **Migrated, parity hardening pending** |
| Symbol search | No equivalent legacy runtime contract | `core-rust/src/search.rs` using shared symbol routing | Rust unit tests + Tencent Smartbox parsing | **Rust-only extension** |
| Market breadth | Legacy market/data scripts | `core-rust/src/market.rs` | Rust Core + Android CI | **Migrated** |
| Limit-up / consecutive limit | Legacy shortline endpoints | `core-rust/src/shortline.rs` | Rust Core + Android CI | **Migrated** |
| Sector rotation | Legacy rotation endpoints | `core-rust/src/rotation.rs` | Rust Core + Android CI | **Migrated** |
| Watchlist | No equivalent mobile SQLite contract | `core-rust/src/storage.rs` | Rust unit/build coverage | **Rust-only extension** |
| Trading journal / fills | Legacy PC journal flows | `journal.rs` + `journal_mutation.rs` | Rust Core + Android UI | **Migrated for mobile** |
| Portfolio projection | Legacy position views | `journal_review.rs` + `PortfolioPanel.tsx` | Derived from fills; no duplicate position DB | **Migrated for mobile** |
| Daily review | Legacy review flows | `daily_review.rs` + `DailyReviewPanel.tsx` | Rust unit tests + local SQLite | **Migrated for mobile** |
| AI journal review | Legacy provider/review paths | `ai_review.rs` | Payload preview + opt-in scope + Rust/Android CI | **Migrated for mobile** |
| General AI chat | No direct legacy mobile equivalent | `ai_chat.rs` | Local history + explicit-message-only context | **Rust-only extension** |
| Android notifications | N/A | Tauri notification plugin | Android compile/runtime permission UI | **Integrated; device validation pending** |
| File import/export / backup | Legacy desktop flows | Not yet migrated | None | **Pending** |
| Android Keystore credential storage | N/A | Current key storage remains app-private SQLite | None | **Pending before stable release** |
| Lite backtest | Python `backtest/` | Not yet migrated | None | **Pending** |
| Full PC backend contract | `orchestrator/` + Python scripts | Mobile Rust Core only covers selected domains | End-to-end parity suite pending | **Pending** |

## Rules for subsequent migrations

1. `android-mvp` is the only active Android implementation branch.
2. Freeze unstable/live provider responses into fixtures before replacing mature parsers.
3. Reuse proven behavior from the old refactor branch only when it fits the active architecture; do not restore duplicate Tauri shells, duplicate frontend transports, or a second quote/K-line stack.
4. Compare canonical output including null/error behavior, market routing, defaults, sort order, timezone handling and numeric precision.
5. Preserve source/provenance metadata separately from UI convenience types.
6. New mobile-only capabilities must be labelled as extensions rather than used as evidence that a legacy PC module is already equivalent.
