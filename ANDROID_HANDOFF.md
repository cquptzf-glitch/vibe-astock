# Android App 全面审查与后续开发交接

> 审查基线：`android-mvp` 分支，代码基线 `573429707470d8911f37733107e5002e314be9df`。
>
> 本文档用于后续开发 Agent / 开发者直接接手。目标不是重复描述已经完成的 Android MVP，而是明确：**当前已经有什么、还缺什么、哪些属于前端、哪些属于 Rust/Core、哪些属于 Android 原生/发布工程，以及下一步按什么顺序开发。**
>
> `android-mvp` 是唯一 Android 主开发线。不要重新创建第二套 Android 分支架构，也不要把旧 Node/Python runtime 塞进 APK。

---

## 1. 结论摘要

当前 Android 版已经完成了一个较完整的第一阶段本地化产品底座，不再只是“能打开页面”的 PoC。

已经具备的核心能力包括：

- Tauri 2 + React/TypeScript + Rust Core + SQLite 单 APK 架构；
- 手机内 Rust Core 直接访问行情数据源，不依赖 PC / Node / Python 常驻服务；
- SH / SZ / BJ 统一股票代码归一化和路由；
- 市场概览、涨停/连板、概念/行业轮动；
- 个股实时行情、前复权 K 线、基础价格结构分析；
- 股票代码 / 中文名 / 拼音搜索；
- 本地自选股；
- 由成交日志推导的本地持仓；
- 交易日志、成交明细编辑、复盘统计；
- 本地日复盘；
- OpenAI-compatible AI 对话和交易复盘；
- Android Keystore-backed AI API Key 安全存储；
- Android 原生通知权限和测试通知；
- 本地业务数据 JSON 备份、预检和事务恢复；
- Android 原生文本分享；
- ARM64 APK CI 构建、签名校验、SHA256 和 artifact 上传。

当前真正的缺口主要分为四类：

1. **P0 工程/发布完整性**：版本号漂移、正式签名/AAB、数据库迁移机制、真实设备自动化、CI 覆盖不足；
2. **P0/P1 Android 产品能力**：WorkManager 后台调度、真正的价格/风险提醒、前后台生命周期处理；
3. **P1 PC 核心功能迁移**：盘中盯盘、信号、板块详情、候选池、日历、研究、Lite Backtest、报告/笔记/Inbox 等；
4. **P1/P2 移动端体验与架构收口**：路由/深链、Android Back、代码分包、离线缓存、日志诊断、分享卡片等。

---

## 2. 当前架构基线

```text
React / TypeScript / Vite
          │
          │ fetch('/api/...')
          ▼
desktop/src/mobileBridge.ts
          │ Tauri invoke
          ▼
src-tauri/src/commands.rs
          │
          ▼
core-rust::AppCore
  ├─ symbol
  ├─ quote / kline
  ├─ market
  ├─ shortline
  ├─ rotation
  ├─ search
  ├─ watchlist
  ├─ journal / portfolio / review
  ├─ daily_review
  ├─ ai_config / ai_chat / ai_review
  └─ backup
          │
          ├─ HTTPS -> Tencent / Eastmoney / Model API
          └─ SQLite

Android Native/Tauri plugins
  ├─ Notification
  ├─ Dialog / FS
  ├─ Vibe Secure Storage -> Android Keystore
  └─ Vibe Share -> Android Sharesheet
```

必须继续坚持：

- Android 正式路径不运行 Node.js；
- Android 正式路径不运行 Python；
- 不恢复 localhost 本地业务服务器作为移动端核心；
- 前端通过稳定 Backend Contract 使用 Rust Core；
- 业务持久化优先 SQLite；
- 密钥只走 Android Keystore；
- 持仓继续由 fills 推导，不建立第二套 position 真相源。

---

## 3. 当前已实现功能矩阵

| 模块 | 前端 | Rust/Core | Android Native | 当前状态 |
| --- | --- | --- | --- | --- |
| 市场概览 | `MarketOverview` | `market.rs` | - | 已完成 MVP |
| 涨停/连板 | `ShortlinePanel` | `shortline.rs` | - | 已完成 MVP |
| 概念/行业轮动 | `RotationPanel` | `rotation.rs` | - | 已完成 MVP |
| 个股实时行情 | `MobileApp` / `StockDetailPanel` | `quote.rs` | - | 已完成，需 parity hardening |
| 前复权 K 线 | `StockDetailPanel` | `quote.rs` | - | 已完成，需 parity hardening |
| 股票搜索 | `MobileApp` | `search.rs` | - | 已完成 |
| SH/SZ/BJ 统一路由 | UI 支持 | `symbol.rs` | - | 已完成并有 legacy fixture |
| 自选股 | `MobileApp` | `storage.rs` | - | 已完成 |
| 交易日志/成交 | `JournalPanelV2` | `journal.rs` / `journal_mutation.rs` | - | 已完成移动版 |
| 持仓 | `PortfolioPanel` | 由 fills 推导 | - | 已完成移动版 |
| 交易统计/纪律/风险复盘 | `JournalReviewPanel` | `journal_review.rs` | - | 已完成移动版 |
| 日复盘 | `DailyReviewPanel` | `daily_review.rs` | - | 已完成简化移动版 |
| AI 通用聊天 | `AiChatPanel` | `ai_chat.rs` | Keystore 注入 Key | 已完成基础版 |
| AI 交易复盘 | `AiReviewPanel` | `ai_review.rs` | Keystore 注入 Key | 已完成基础版 |
| AI 配置 | AI/设置 UI | `ai_config.rs` | Secure Storage | 已完成 |
| 通知权限/测试通知 | `SettingsPanel` | - | Notification plugin | 已接入，未形成实际业务提醒 |
| 备份/恢复 | `SettingsPanel` | `backup.rs` | Dialog + FS | 已完成 v1 |
| 分享 | 个股详情 | - | `vibe-share` | 已完成纯文本 v1 |
| APK CI | - | core tests | Android build | 已运行并通过 |

---

# 4. P0：必须先修的工程与发布问题

## 4.1 版本号已经出现三套不同真相源

当前基线可以同时看到：

- `src-tauri/Cargo.toml`：`0.1.12`
- `src-tauri/tauri.conf.json`：`0.1.12`
- Rust Core `/health` 与 User-Agent：仍是 `0.1.11-mobile`
- `MobileApp.tsx` 顶部：仍显示 `v0.1.9`
- `.github/workflows/android.yml`：APK/tag/artifact 仍命名 `v0.1.11`

这已经导致“代码是 v0.1.12，CI artifact 名称是 v0.1.11”的实际漂移。

### 必须改

建立**唯一版本源**，推荐：

```text
src-tauri/tauri.conf.json.version
        ↓ build.rs / env / script
Cargo package version
Rust Core health/product version
Mobile UI displayed version
GitHub Actions artifact/tag/title
```

不要再手改 4~5 个版本字符串。

### 验收

一次 bump 后自动保证：

```text
Tauri version == Cargo version == /health version == UI version == artifact version == release tag
```

---

## 4.2 GitHub Release 已落后于代码

当前代码已经进入 v0.1.12，但发布流水线仍是 v0.1.11 命名；最近普通 push 只构建 artifact，release step 因没有 `[publish-android]` 被跳过。

后续必须把“开发构建”和“发布构建”分开：

- `android-ci.yml`：每次 push，构建测试 APK；
- `android-release.yml`：tag / workflow_dispatch，生成正式 release；
- release workflow 从版本文件读取版本，不硬编码；
- 不要依赖 commit message magic string 决定产品版本。

---

## 4.3 正式签名 / AAB 尚未完成

当前 APK 仍是 debug-signing 路径，适合直接侧载测试，但不适合作为正式产品发行。

缺少：

- persistent private release keystore；
- GitHub Secret/Environment 管理签名材料；
- release variant；
- AAB；
- Play/internal testing 对应构建；
- signing certificate 固定与升级策略。

建议继续保留 debug APK CI，但增加独立的 production signing workflow。

---

## 4.4 SQLite 没有正式 schema migration 框架

现在多个模块在启动时使用 `CREATE TABLE IF NOT EXISTS` 初始化。早期 MVP 可接受，但随着版本升级会出现：

- 新字段无法仅靠 `CREATE TABLE` 升级；
- 索引/约束修改不可控；
- 旧版用户升级路径不可测试；
- backup schema 和 SQLite schema 容易分叉。

### 下一步

至少建立：

```text
PRAGMA user_version
migration 1 -> 2
migration 2 -> 3
...
```

或引入轻量 Rust migration 层。

每次迁移必须测试：

- 空数据库；
- 上一正式版本数据库；
- 重复启动幂等；
- 迁移失败不破坏原库；
- backup restore 与新 schema 一致。

---

## 4.5 Android 真实设备测试仍不足

当前 CI 能证明：

- TS typecheck/build；
- Rust Core host tests；
- Android project init；
- ARM64 APK build；
- APK signature 校验。

但不能证明 Android 运行时能力真的可用。

尤其应在真机/模拟器补测试：

- Keystore 首次保存/读取/删除/升级迁移；
- Dialog SAF `content://` 文件导入导出；
- Notification Android 13+ 权限；
- Android Sharesheet；
- WebView -> Tauri IPC；
- Activity kill / recreate；
- 后台/前台切换；
- 备份恢复后页面数据刷新；
- 网络断开/恢复。

建议至少增加一个 Android emulator smoke workflow；native plugin 最好加 Kotlin instrumentation tests。

---

# 5. P0/P1：Android 原生产品能力缺口

## 5.1 WorkManager / 后台调度：未实现

这是当前最明显的 Android 产品能力缺口。

现在通知只做了：

- 权限查询；
- 权限申请；
- 手动测试通知。

还没有真正实现：

- 定时刷新；
- 收盘后自动复盘任务；
- 自选/持仓风险检查；
- 后台价格条件提醒；
- 重启后任务恢复；
- 网络约束/电量约束；
- Android Doze 下的调度策略。

### 推荐实现

新增原生插件/模块：

```text
src-tauri/tauri-plugin-vibe-scheduler/
  Android/Kotlin -> WorkManager
```

Rust 保存“提醒规则/任务定义”，Kotlin 只负责 Android 调度和唤醒。

不要把复杂业务规则复制到 Kotlin。

---

## 5.2 价格/风险提醒：未实现业务闭环

应建立本地 SQLite reminder 表，例如：

```text
id
symbol
kind              # price_above / price_below / pnl / stop / review
threshold
enabled
last_triggered_at
cooldown_minutes
created_at
updated_at
```

第一阶段建议只做：

1. 自选股价格突破提醒；
2. 持仓止损位/目标位接近提醒；
3. 收盘后“该写日复盘”提醒。

避免第一版做高频实时后台盯盘；Android 对常驻网络和电池都有约束。

---

## 5.3 前台持续监控 / Foreground Service：未实现

如果未来明确要“交易时段持续盯盘”，需要单独设计：

- Foreground Service；
- 常驻通知；
- 用户显式开启/关闭；
- 轮询频率；
- 流量和耗电预算；
- 系统杀进程后的恢复。

这不是 WorkManager 的同一种用法，不要混在一个实现里。

---

## 5.4 分享仍只是文本 v1

当前个股分享是系统 Sharesheet + 文本。

后续可补：

- 个股行情分享卡图片；
- 日复盘分享卡；
- 交易复盘摘要分享卡；
- JSON/Markdown 报告文件分享。

但优先级低于后台任务和核心业务迁移。

---

# 6. P1：前端仍缺少的主要业务页面

PC 版 finance router 当前仍有大量 Android 没有对应移动页面的域。

## 6.1 盘中盯盘 / Watch / Intraday

PC：

```text
/watch
/watch/intraday
```

Android 当前只有市场总览 + 个股查询，没有完整盘中盯盘工作台。

建议移动版不要原样搬 PC 大屏，而是做三层：

1. 异动列表；
2. 自选实时状态；
3. 点击进入个股详情。

后端先迁移必要的 abnormal / intraday deterministic endpoint。

---

## 6.2 Signals：未迁移

PC：

```text
/signals
/signals/:tab
```

Android 尚无信号中心。

建议先做 deterministic signals，不要一开始把 AI 解释也绑进去：

- 量价异动；
- 均线/突破类信号；
- 涨跌停/连板衍生信号；
- 自选/持仓信号过滤。

---

## 6.3 Sectors / SectorDetail：只有轮动摘要，没有完整板块详情

Android 当前 `rotation.rs` 只能算板块轮动入口，并不等同 PC：

```text
/sectors
/sectors/:key
```

仍缺：

- 板块成分股；
- 板块详情；
- 龙头/涨跌分布；
- 板块历史趋势；
- 个股到板块的反向关联。

---

## 6.4 Candidates：未迁移

PC `/candidates` 尚无 Android 对应。

建议候选池直接建立在：

```text
signals + shortline + rotation + watchlist
```

之上，而不是复制一套独立股票数据。

---

## 6.5 Calendar：未迁移

PC `/calendar` 尚无 Android 对应。

可先做 Lite 版：

- 交易日；
- 财报/公告事件；
- 用户自定义验证日期；
- 与系统通知/WorkManager 联动。

这是很适合移动端的能力，优先级应高于完整 Research UI。

---

## 6.6 Research / 个股资料：大部分未迁移

Android 个股页目前核心还是：

```text
quote + qfq kline + 简单均线/区间结构
```

PC 数据层还有大量未迁移 deterministic 数据：

- company/profile；
- financials；
- estimates；
- announcements；
- PE history / valuation；
- 更多历史/基本面端点；
- Research 页面工作流。

建议先迁移“数据”，再做 Research UI。

### 推荐顺序

1. 公司概况；
2. 财务摘要；
3. 公告；
4. 估值历史；
5. 一致预期；
6. 最后再做 AI 深度研究。

---

## 6.7 Backtest：未迁移

PC `/backtest` 仍依赖 Python 体系；Android Rust Core 没有 backtest 模块。

不要把 Python backtest 打入 APK。

建议做 `Lite Backtest`：

- 日线级；
- 单标的或小股票池；
- SMA/EMA/突破/止损止盈等基础规则；
- 本地 Rust 执行；
- 输出收益曲线、最大回撤、胜率、交易次数；
- 限制最大 bars / symbols，避免手机资源失控。

复杂研究回测继续留桌面端。

---

## 6.8 Debate：未迁移

PC `/debate` 是较重 AI/Agent 功能。

移动版不建议第一时间完整复制多 Agent 辩论。

如果要做，建议先实现：

```text
“多角度分析” = 同一个 API provider 的结构化多观点 prompt
```

不要先移植桌面的 Agent Runner / CLI subscriptions。

---

## 6.9 MyReports / Notes / Inbox：未迁移

PC 仍有：

```text
/my-reports
/notes
/inbox
```

移动端目前没有统一内容库。

推荐下一阶段建立一个通用本地对象：

```text
mobile_documents
id
kind        # note/report/inbox/research
symbol
 title
content_md
metadata_json
created_at
updated_at
```

这样 Notes/Report/AI Research 不需要各建一套存储。

---

# 7. P1：当前已存在页面的功能深度缺口

## 7.1 自选股

当前：基本增删和打开个股。

可补：

- 批量刷新；
- 自定义排序；
- 分组/标签；
- 涨跌幅/价格直接显示；
- 提醒规则入口；
- 导入已有自选。

不要复制持仓表到自选表。

---

## 7.2 持仓

当前设计正确：持仓由成交 fills 推导。

不要增加第二套 positions CRUD。

可继续补：

- 手续费/税费配置；
- 账户/现金维度（如产品确实需要）；
- 分批卖出成本法明确化；
- 导入成交记录 preview；
- 数据异常检测（负持仓、重复成交、日期异常）；
- 持仓风险提醒。

PC backend 已存在更丰富的 holdings/import/cost-method 概念，可以选择性迁移，而不是整套复制。

---

## 7.3 日复盘

移动版是“市场快照 + 今日观察 + 明日验证”的简化版本。

PC DailyReview 明显更丰富，后续可补：

- 情绪周期；
- AI 情绪复盘；
- 周复盘；
- review tiers；
- 复盘历史列表；
- 可分享复盘卡；
- 自动生成但必须用户确认保存的明日验证项。

---

## 7.4 AI Chat

当前 Android AI Chat 是基础 OpenAI-compatible chat：

- 单历史流；
- 非流式；
- 不默认附加用户本地数据；
- 没有 tool calling；
- 没有 research agent；
- 没有线程/会话管理。

这是安全、简单的 MVP，但不是 PC Agent 能力的等价替代。

建议按顺序增强：

1. streaming/cancel；
2. 多会话/thread；
3. 用户显式选择“附加当前股票/当前持仓摘要”；
4. deterministic tools（quote/kline/financials）；
5. 最后才考虑 deep research。

必须保持“本地数据显式授权后才发给模型”。

---

# 8. P1：Rust/Core 后端缺口

当前 `AppCore` 路由主要覆盖：

```text
health/product
data summary
backup
watchlist
journal/review/portfolio projection
daily review
AI config/chat/review
symbol search
market overview
shortline
rotation
quote/kline
```

而 PC 仍有大量 Node/Python contract 尚未迁移。

建议把后端缺口按三类处理。

## 8.1 第一类：应该迁移成 Rust deterministic Core

优先：

- trade calendar；
- announcements；
- company profile；
- financials；
- estimates；
- valuation / PE history；
- signals；
- abnormal / intraday；
- sector detail / constituents；
- candidates；
- Lite Backtest；
- alerts/reminders state。

原则：先冻结 PC/Python 输出 fixture，再移植 parser/mapper。

---

## 8.2 第二类：应重新设计为移动端本地能力

不必追求 PC API 逐字节一致：

- notes；
- reports；
- inbox；
- mobile notification rules；
- backup；
- share；
- local AI thread；
- local document library。

这类在 parity matrix 中要标记为 **Mobile extension**，不要伪装成“PC 已完全迁移”。

---

## 8.3 第三类：不建议直接迁移

- Node Agent Runner；
- Python 子进程 orchestration；
- 本机 CLI Claude/Codex/WorkBuddy subscription runtime；
- 桌面远程访问服务器；
- 完整 Python backtest runtime。

如果移动端需要相似产品能力，应基于手机约束重新设计。

---

# 9. Provider / 数据质量仍需补强

## 9.1 Quote/Kline parity hardening 未完成

`symbol` 已有 legacy fixture，但 Quote/Kline 还缺成熟的 frozen provider fixtures。

必须补：

- Tencent quote fixture；
- Eastmoney quote fixture；
- Eastmoney K-line fixture；
- suspended/盘前/无成交；
- SH/SZ/BJ；
- ST/特殊代码；
- 上游字段缺失；
- timeout/HTTP failure；
- fallback 行为；
- 单位和数值精度。

避免 CI 直接依赖实时供应商响应做断言。

---

## 9.2 缓存/限流/重试策略不完整

当前 quote `/fetch` 明确返回 `cached: false`。

后续应该统一网络策略：

```text
connect timeout
request timeout
retry only idempotent GET
exponential backoff
per-provider rate limit
memory cache
optional SQLite TTL cache
```

尤其是后台任务启用后，不做 rate limit 很容易被数据源封锁。

---

# 10. 前端架构与性能问题

## 10.1 MobileApp 仍是 state-based 单页壳，没有移动端 Router

当前 tab 主要由 React `useState` 驱动。

缺点：

- Android Back 行为难做；
- 无深链；
- 无页面恢复；
- 无 route-level lazy load；
- 从通知点击无法精确进入 symbol/reminder/report。

建议引入轻量 mobile router：

```text
/mobile/market
/mobile/stock/:symbol
/mobile/shortline
/mobile/rotation
/mobile/my/watch
/mobile/my/hold
/mobile/journal
/mobile/review
/mobile/ai
/mobile/settings
```

可以使用 memory/hash routing，关键是建立“页面状态 = route”这一层。

---

## 10.2 Android 构建仍静态 import 桌面 router

`desktop/src/main.tsx` 同时静态 import：

- `MobileApp`
- PC `router`
- PC notes/watch hydration code

即便 Android 运行时不走桌面分支，静态 import 仍会进入 Vite module graph，增加 bundle 和 chunk 体积。

建议：

```ts
if (isTauriMobile) {
  const { MobileApp } = await import('./mobile/MobileApp')
} else {
  const { router } = await import('./verticals/finance/router')
}
```

或者建立独立 `mobile-main.tsx` entry。

这是下一轮包体/启动优化的重要点。

---

## 10.3 存在可清理的旧移动组件

目前同时存在：

- `JournalPanel.tsx`
- `JournalPanelV2.tsx`

实际 `JournalWorkspace` 使用 V2。

后续应确认旧版没有引用后删除，避免 Agent 修改错文件。

同类工作建议做一次 `desktop/src/mobile` dead-code audit。

---

# 11. 安全与隐私后续项

已经做对的部分：

- API Key 已迁移 Android Keystore-backed secure storage；
- Core API 不回显 Key；
- backup 明确排除 Key/Keystore/model config；
- AI Chat 默认不自动上传本地交易数据。

仍需补：

1. Android Auto Backup policy：明确 secure ciphertext / DB 哪些允许系统云备份；
2. HTTP model base URL 策略：当前 Core 允许 `http://`，要明确 LAN 模型场景和 cleartext network policy；
3. backup 文件虽然没有 API Key，但交易日志/AI 聊天本身仍是敏感数据，可考虑可选加密备份；
4. 增加“清除全部本地数据”能力，并明确 Key 是否同时清除；
5. release build 禁止 verbose secret logging；
6. 自定义 native plugin 做异常路径和输入长度 fuzz/边界测试。

---

# 12. CI / 测试体系缺口

当前 Android workflow 主要是：

```text
npm ci
npm run build          # TS typecheck + Vite build
cargo test core-rust
cargo tauri android init
cargo tauri android build
apksigner verify
artifact upload
```

还缺：

- `npm test --prefix desktop`（至少挑移动端共享 contract tests）；
- `cargo fmt --check`；
- `cargo clippy -- -D warnings`（可逐步启用）；
- `cargo test` for Tauri host/custom plugin crates；
- Kotlin unit/instrumentation tests；
- Android emulator smoke test；
- backup compatibility tests across versions；
- DB migration upgrade tests；
- UI interaction tests；
- dependency audit；
- release AAB/signing verification。

不要把“APK 编译成功”当作“Android 功能已验证”。

---

# 13. 文档状态问题

`PARITY_MATRIX.md` 当前已经过期，至少两项仍写错：

- File import/export / backup 仍写 Pending，但 v0.1.11 已完成；
- Android Keystore 仍写 Pending / SQLite，但 v0.1.10 已完成 Keystore 迁移。

后续每完成一个里程碑都必须同步：

```text
ANDROID_HANDOFF.md
PARITY_MATRIX.md
version/release notes
```

否则下一个 Agent 会重复开发已经存在的功能。

---

# 14. 建议开发路线图

## Phase A：先收口工程基线（必须先做）

### A1. 版本 single-source-of-truth

修复：

- Tauri 0.1.12；
- Core 0.1.11；
- UI 0.1.9；
- workflow 0.1.11。

### A2. 更新 `PARITY_MATRIX.md`

把 Keystore / Backup / Share 更新为真实状态。

### A3. SQLite migrations

建立版本和升级测试。

### A4. CI 扩展

加入 fmt / clippy / plugin tests / emulator smoke 的最小集合。

### A5. 前端 entry split

让 Android 不再静态打入完整 PC router/module graph。

---

## Phase B：完成“像 Android App”的关键能力

### B1. WorkManager plugin
### B2. Reminder SQLite contract
### B3. 价格/止损/复盘通知
### B4. Notification click -> mobile route/deep link
### B5. Android Back + route state

完成 Phase B 后，通知插件才从“能发测试通知”升级成真正业务功能。

---

## Phase C：迁移最有价值的 PC deterministic 功能

推荐顺序：

1. Calendar；
2. Stock profile / financial summary；
3. Announcements；
4. Signals；
5. Intraday abnormal/watch；
6. Sector detail；
7. Candidates；
8. valuation / estimates。

每个模块遵循：

```text
PC oracle -> fixture -> Rust parser/mapper -> Core API -> mobile UI -> CI
```

不要先写 UI 再临时拼数据。

---

## Phase D：研究与生产力功能

1. AI streaming + cancel；
2. AI threads；
3. 显式 attach stock/portfolio context；
4. Lite Backtest；
5. Notes / Reports / Inbox 共用 document storage；
6. share card；
7. Deep Research Lite。

---

# 15. 每个新模块的统一验收模板

后续 Agent 开发任何模块，都要至少满足：

## Backend

- 输入有明确 schema；
- symbol 统一走 `SymbolKey`；
- timeout / upstream / empty data 有明确错误语义；
- 不重复实现 market routing；
- 有 unit tests；
- live provider parser 有 fixture；
- SQLite 写入有 transaction / migration 考虑。

## Frontend

- loading / empty / partial / error 四种状态都有 UI；
- 手机触控尺寸合理；
- 不因为一个子请求失败让整页崩掉；
- 页面能从 route/deep link 恢复；
- 不直接访问 Node/Python contract；
- 本地敏感数据发送 AI 前必须用户显式知情。

## Android

- 权限拒绝可降级；
- Activity recreation 不崩；
- Android 7+ 基线验证；
- Android 13+ notification 验证；
- 至少一台 ARM64 真机验证。

## CI

- frontend build 通过；
- Rust tests 通过；
- Android APK 通过；
- 新 native plugin 有对应测试；
- artifact version 与 app version 一致。

---

# 16. 明确禁止的重复设计

后续开发不要再做以下事情：

1. **不要建立第二个 Android 主分支实现。**
2. **不要再写第二套股票代码/市场判断。** 全部走 `symbol.rs / SymbolKey`。
3. **不要给 quote、search、watchlist 各自写 market parser。**
4. **不要新建独立持仓数据库。** 持仓继续由 fills 推导。
5. **不要把 API Key 放回 SQLite/localStorage。**
6. **不要把 Node runtime 打进 APK。**
7. **不要把 Python/pandas 回测 runtime 打进 APK。**
8. **不要再做第二套 Tauri transport/shell。**
9. **不要为了“PC parity”把桌面大屏页面原样塞进手机。** 复用 contract，重做移动交互。
10. **不要删除 legacy oracle，直到对应 Rust 功能 fixture/parity 验证完成。**

---

# 17. 当前关键文件地图

```text
ANDROID_REFACTOR_PLAN.md       原始 Android 架构目标
PARITY_MATRIX.md               PC/Rust 行为迁移矩阵（当前需更新）
ANDROID_HANDOFF.md             本文档，后续开发入口

core-rust/src/lib.rs           Mobile API route 总入口
core-rust/src/symbol.rs        唯一股票代码/市场路由
core-rust/src/quote.rs         Quote + Kline
core-rust/src/market.rs        市场概览
core-rust/src/shortline.rs     涨停/连板
core-rust/src/rotation.rs      板块轮动
core-rust/src/search.rs        股票搜索
core-rust/src/storage.rs       自选 SQLite
core-rust/src/journal*.rs      日志/成交/统计/持仓推导
core-rust/src/daily_review.rs  日复盘
core-rust/src/ai_*.rs          AI config/chat/review
core-rust/src/backup.rs        业务数据 backup/restore

desktop/src/mobile/MobileApp.tsx
                               移动首页/主导航/个股查询
desktop/src/mobile/JournalWorkspace.tsx
                               交易/日复/统计/AI/设置入口
desktop/src/mobile/SettingsPanel.tsx
                               通知/备份/隐私状态
desktop/src/mobile/StockDetailPanel.tsx
                               个股 K 线结构 + 分享
desktop/src/mobile/nativeShare.ts
                               Android/Web 分享桥

desktop/src/mobileBridge.ts   /api -> Tauri IPC 兼容层

src-tauri/src/commands.rs      Shell 命令/Keystore 注入边界
src-tauri/src/secure_ai.rs     AI secure credential 边界
src-tauri/tauri-plugin-vibe-secure-storage/
                               Android Keystore plugin
src-tauri/tauri-plugin-vibe-share/
                               Android Sharesheet plugin
src-tauri/capabilities/        Tauri permissions

.github/workflows/android.yml  Android CI（当前版本信息需收口）
```

---

# 18. 下一位 Agent 的推荐第一批任务

如果没有新的产品优先级指令，按以下顺序继续：

### Task 1 — Version + docs + CI hygiene

- 创建单一版本源；
- 修复 UI/Core/workflow 版本漂移；
- 更新 `PARITY_MATRIX.md`；
- 删除确认无引用的 `JournalPanel.tsx`；
- 增加 fmt/clippy/关键 plugin tests。

### Task 2 — SQLite migrations

- 引入 `user_version`；
- 给现有 schema 建 baseline migration；
- 加旧库升级测试。

### Task 3 — Android scheduler MVP

- WorkManager plugin；
- reminder schema；
- 收盘复盘提醒；
- 一个价格条件提醒；
- 通知点击返回对应页面。

### Task 4 — Mobile route/deep-link

- stock route；
- my/journal/settings route；
- Android Back；
- notification deep link。

### Task 5 — Calendar + Stock Research Lite

先迁移 deterministic endpoint，不先做 AI。

---

# 19. 当前可作为“完成”的定义

当前 Android 版可以定义为：

> **功能型 Android MVP 已成立，但尚未达到 Production Ready，也尚未达到 PC 核心能力完整迁移。**

达到下一阶段“Beta Ready”至少应完成：

- 版本/发布体系收口；
- release signing/AAB；
- SQLite migrations；
- Keystore/backup/share/notification 真机 smoke；
- WorkManager + 至少一类真实通知；
- mobile routing/back；
- Quote/Kline fixture hardening；
- Calendar + 一组 Research deterministic 数据；
- CI 不只验证“能编译 APK”。

达到“功能接近桌面核心工作流”还需要：

- intraday/watch；
- signals；
- sectors detail；
- candidates；
- research data；
- Lite Backtest；
- reports/notes；
- 更完整 AI workflow。

---

## 20. 交接规则

后续每一轮开发结束都应更新本文档中的状态，至少记录：

```text
完成了什么
新增了哪些 Core route
新增了哪些 SQLite schema/migration
新增了哪些 native permission/plugin
对应测试是什么
APK CI run 是否成功
是否已发布
还有什么已知问题
```

这样下一次“继续开发”可以直接从真实状态接手，而不是重新猜测仓库已经做到哪里。

---

## 2026-10-16 开发增量：盯盘工作台

已完成并进入 Android 主线：

- core-rust/src/monitor.rs：新增本地盯盘快照 API，统一读取自选与由成交流水推导的持仓。
- 使用腾讯批量行情一次拉取最多 50 个标的；批量失败的标的复用现有 Tencent → EastMoney fallback。
- 价格、涨跌幅、成交额、涨跌停价、行情日期均允许缺失，不把缺失显示成 0。
- 持仓同时计算最新市值、浮盈亏、浮盈亏百分比和止损/目标状态。
- 仅当行情日期为当天时才判定止损/目标/涨跌停状态，避免周末和旧行情误报。
- desktop/src/mobile/MonitorPanel.tsx：新增“自选动态 / 持仓动态”移动盯盘页面。
- 页面打开后每 60 秒刷新，连续采样达到约 3 分钟后可标记“急拉 / 急跌”（阈值 1.5%）；离开页面自动停止。
- 与现有“短线 → 涨停池”合并为同一工作区，避免增加底部导航压力。
- 当前版本推进至 0.1.13。

下一步仍按本文档路线推进：Android 原生提醒真机验收 → mobile route/back/deep-link → 产业资讯/产业信号 → 更完整 Signals/Intraday → deterministic Research 数据 → Lite Backtest。
### 2026-10-16 后续增量：盯盘 + Lite 回测

- 已新增 `/mobile/monitor`：自选/持仓批量行情、涨跌停价、当日行情校验、止损/目标状态。
- App「短线」工作区现在默认进入「盯盘」，同时保留「涨停池」。
- 盯盘页面每 60 秒刷新；连续采样约 3 分钟后对 ≥1.5% 的价格变化标记急拉/急跌。
- 已新增 `/mobile/backtest` 与 BacktestPanel：SMA 趋势交叉 Lite 回测，信号使用前一交易日收盘、下一交易日开盘执行。
- Lite 回测由 Rust 在 Android 内计算，不引入 Python；结果包含策略收益、买入持有、超额收益、最大回撤、胜率、交易次数和权益曲线。
- Lite 回测明确不计手续费、印花税、滑点，不作为桌面完整 Agent 回测的替代品。

后续优先：先完成当前 Android CI 的 Rust/Gradle 真机链路验证，再继续产业资讯/产业信号、完整资金流/异动、估值历史、研究数据和高级回测。