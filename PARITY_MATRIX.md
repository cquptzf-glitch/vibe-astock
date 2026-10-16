# Android / Rust migration parity matrix

This file is the consolidation gate for the Android migration. The `android-mvp` branch is the single active Android development line. Legacy Node/Python behavior remains an oracle until the matching Rust path has frozen fixture/golden coverage and CI is green.

For the full current gap analysis and recommended continuation order, see `ANDROID_HANDOFF.md`.

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
| Sector rotation | Legacy rotation endpoints | `core-rust/src/rotation.rs` | Rust Core + Android CI | **Migrated** |
| Watchlist | No equivalent mobile SQLite contract | `core-rust/src/storage.rs` | Rust build/Android UI | **Rust-only extension** |
| Trading journal / fills | Legacy PC journal flows | `journal.rs` + `journal_mutation.rs` | Rust Core + Android UI | **Migrated for mobile** |
| Portfolio projection | Legacy position views | `journal_review.rs` + `PortfolioPanel.tsx` | Derived from fills; no duplicate position DB | **Migrated for mobile** |
| Daily review | Legacy review flows | `daily_review.rs` + `DailyReviewPanel.tsx` | Rust unit tests + local SQLite | **Migrated for mobile; PC feature depth still larger** |
| AI journal review | Legacy provider/review paths | `ai_review.rs` | Payload preview + opt-in scope + Rust/Android CI | **Migrated for mobile** |
| General AI chat | No direct legacy mobile equivalent | `ai_chat.rs` | Local history + explicit-message-only context | **Rust-only extension; streaming/tools/threads pending** |
| Android Keystore credential storage | N/A | `tauri-plugin-vibe-secure-storage` + `secure_ai.rs` | Rust regression coverage + Android build; device smoke still needed | **Integrated; device hardening pending** |
| Android notifications | N/A | Tauri notification plugin | Android compile/runtime permission UI | **Integrated permission/test channel; business reminders pending** |
| File import/export / backup | Legacy desktop flows | `core-rust/src/backup.rs` + Dialog/FS + `SettingsPanel` | schema validation + transactional restore tests | **Migrated as mobile versioned JSON backup** |
| Android native sharing | Legacy share-card flows are richer | `tauri-plugin-vibe-share` + `nativeShare.ts` | Android build + text share path | **Integrated text sharing; image/share-card parity pending** |
| WorkManager / background scheduler | Desktop scheduler/alerts concepts | Not yet implemented on Android | None | **Pending / high priority** |
| Intraday watch / abnormal | Legacy PC watch flows | Not yet migrated | None | **Pending** |
| Signals | Legacy PC signals flows | Not yet migrated | None | **Pending** |
| Sector detail / constituents | Legacy PC sectors flows | Rotation summary only | None for full detail | **Partial / pending** |
| Candidates | Legacy PC candidates flows | Not yet migrated | None | **Pending** |
| Calendar | Legacy trade/calendar flows | Not yet migrated | None | **Pending** |
| Stock research deterministic data | profile/financials/announcements/estimates/valuation scripts | quote/K-line only on mobile | None for remaining endpoints | **Pending** |
| Lite backtest | Python `backtest/` | Not yet migrated | None | **Pending; implement in Rust, do not bundle Python** |
| Notes / reports / inbox | Legacy desktop local/product flows | Not yet migrated | None | **Pending; redesign as mobile-local document storage** |
| Debate / deep research | Node/Agent/Python workflows | Not migrated | None | **Pending / redesign rather than runtime port** |
| Full PC backend contract | `orchestrator/` + Python scripts | Mobile Rust Core only covers selected domains | End-to-end parity suite pending | **Pending** |
| Production Android release | Desktop release is separate | Debug-signed ARM64 APK CI | APK build/signature verification | **Not production-ready: release signing/AAB/version unification pending** |

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
