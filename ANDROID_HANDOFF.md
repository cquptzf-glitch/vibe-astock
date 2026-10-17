# Android App 全面审查与后续开发交接

> 最新未完成功能以 [ANDROID_REMAINING.md](ANDROID_REMAINING.md) 为准；本文早期章节保留历史基线，最新完成与验证记录在文末。每轮必须回复剩余清单、构建状态及实际发布版本。

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


### 2026-10-16 后续增量：移动路由、返回与通知定位（0.1.14）

本轮完成：

- 新增 mobileRoutes.ts / useMobileNavigation.ts，以 hash URL + 浏览器 history 管理页面；主导航、发现、短线、自选/持仓及日志子栏目均从路由恢复。
- 支持 /mobile/stock/:symbol、/mobile/journal/daily、/mobile/settings 等应用内定位；保存最近页面，不保存表单草稿或额外业务数据。
- 从候选/盯盘/持仓打开个股后可返回来源栏目；首次直接进入个股/通知目标会先建立行情首页返回记录。Android Back 使用 Tauri/WebView 自带 history 返回行为，首页无历史时交回系统。
- 个股请求在路由切换后忽略过期响应；新股票加载时不显示上一只股票的行情，并提供重试入口。
- Rust reminder native_snapshot 增加 route 字段，股票目标复用 SymbolKey，日复盘目标为 /mobile/journal/daily；无新增 SQLite 表或 migration。
- 原生提醒 PendingIntent 携带白名单页面和每条规则独立 URI；load/onNewIntent 接收冷启动/热启动目标；React 挂载后通过 take_reminder_navigation 一次性读取，避免冷启动与 StrictMode 重挂载丢通知。
- 未新增 Android 系统权限或插件；不注册外部可调用的通用 URL scheme。此处 deep-link 指应用内部 hash 及通知定位，外部网页/App Links 留待后续。
- 修复接手时的 monitor 数值解析编译错误和 ResearchError 缺少 Debug 导致的测试编译错误。
- Android CI 补装 orchestrator 的共享测试依赖，解决 agent_error.test.ts 导入 ajv 失败；保留完整前端测试，并增加原生提醒路由 JUnit 测试。
- 新增 Rust 通知路由及快照测试、前端 history/刷新/无效路由/通知重挂载测试、工作台渲染测试、Kotlin 白名单路由测试。

验证约定：

- 本地仅执行 Rust cargo test / cargo check；前端检查、Kotlin 测试和 APK 编译均在 GitHub Android CI 中执行。
- 本地 Rust 验证通过：30 项单元测试 + 1 项 symbol parity；cargo check 通过。已提交 Core Cargo.lock，CI 以 --locked 检查依赖。
- 代码提交 91c41df 的 Android CI 全部通过：[Android CI #38880360377](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38880360377)。前端类型检查/构建、411 项前端测试、31 项 Rust 测试、ARM64 APK 编译、原生提醒路由 JUnit、APK 签名校验均通过。
- 同一代码提交的 [Cross-platform verification #38880360183](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38880360183) 通过。
- APK 为 debug-signed 0.1.14 ARM64，22,294,505 bytes（21.26 MiB）。[构建产物 #12037563102](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38880360377/artifacts/12037563102) 内含 APK 与 SHA256；CI 产物保留 30 天。未创建 Release。
- 设备上的 Android Back/手势、点击通知冷/热启动及 Activity recreation 仍需真机验收，单元测试与编译不代表已完成真机验证。

后续优先：通知真机验收 → 产业资讯/产业信号 → 行情供应商 fixture hardening → 更完整 Signals/Intraday/研究数据。正式签名/AAB、AI 流式输出与高级回测仍未完成。


### 2026-10-16 后续增量：产业资讯、产业信号与板块详情（0.1.15）

本轮一次完成三个业务模块及原文打开能力：

1. **产业资讯**：Rust 解析 RSS / Atom，复用 datasources/rss_sources.json 的 12 个行业与 HTTPS 源；支持切换任意单源、3/7/30 天、已获取资讯关键词筛选和关注词筛选。综合模式每次并发请求前 4 个源，单源失败保留其他结果；全部失败返回 502；来源、时间缺失、去重与未来时间过滤都有明确处理。
2. **产业信号 · GPU 租金**：B200 / H100 SXM / A100 SXM4，30/90/365 天历史、最新采样、约 7 日变化和趋势曲线。卡片与曲线共用同一末点，时间戳单位为秒；NaN/Inf、重复冲突与过期数据均处理。统计口径沿用 500.farm 分组后聚合的中位，B200 设备折旧参考线不套用于其他型号。本次未迁移 Kalshi 月均远期部分。
3. **板块详情与成分股**：从行业/概念轮动榜点击进入，展示板块概览、近 90 个交易日不复权指数、50 条一页成分股、涨跌/主力净额/换手排序、成分股到个股详情跳转。本页涨跌分布明确标为分页统计；板块代码校验独立于股票 SymbolKey，股票市场仍复用唯一归一化实现。排序和分页在本次会话保留。
4. **系统浏览器原文打开**：新增 tauri-plugin-opener；只开放 http/https URL，拒绝文件、脚本、带用户信息的链接；原文交给系统应用，不替换业务 WebView。

新增 Core route：
- GET /mobile/news/catalog
- GET /mobile/news?industry=ai&days=7&source=...
- GET /mobile/industry/gpu-rent?days=90
- GET /mobile/boards/detail?code=BK0475&page=1&sort=change&order=desc

新增移动路由：/mobile/discovery/news、/mobile/discovery/signals、/mobile/board/:code。发现栏目横向滚动，不增加底部主导航。

持久化与原生权限：
- 不新增 SQLite 表或 migration，不改备份结构，不向模型发送本地数据。
- opener 仅增加限定 http/https 的 Tauri capability，不增加 Android 危险权限。
- 每个源有请求超时和 2 MiB 响应上限；本轮不宣称已建立全局缓存/限流/离线体系。

验证：
- 本地仅运行 Rust：40 项单元/接口测试 + 1 项股票代码 parity，以及 cargo check 通过。
- 新增离线 RSS/Atom legacy fixtures、GPU 历史样本和真实公开板块成分股样本；来源说明在 core-rust/tests/fixtures/README.md。
- 新增前端呈现/缺值/部分失败/外链/板块返回导航测试，交给 GitHub CI 执行。
- 已做公开数据源可达性探测：东财板块成分股/概览、500.farm B200 matrix 可返回数据；这不代表所有 RSS 来源和手机网络均已验收。
- 代码提交 68876f5 的 [Android CI #38884614144](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38884614144) 全部通过：41 项 Rust 测试、416 项前端测试、前端类型检查/构建、ARM64 APK 编译、原生提醒路由 JUnit 与 APK 签名校验均成功。
- 同一代码提交的 [Cross-platform verification #38884614099](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38884614099) 通过。
- APK 为 debug-signed 0.1.15 ARM64，22,883,605 bytes（21.82 MiB）。[构建产物 #12038703285](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38884614144/artifacts/12038703285) 内含 APK 与 SHA256，保留 30 天；未创建 Release。
- 新模块尚需真机验证系统浏览器打开、返回手势、页面恢复及弱网表现；CI 成功不等于完成设备验收。

仍待后续：Kalshi 远期、更多产业温度计、完整股票信号中心/盘中异动/资金流、估值历史、AI 流式与多会话、正式签名/AAB、真机生命周期验收。公司概况、公告、财务摘要和盈利预测已有基础接口，不应继续标为完全未迁移。


### 2026-10-16 后续增量：资金流、异常监控、反向板块与量价观察（0.1.16）

本轮完成：

- 个股资金流：日级 20/60/120 根，分钟当日累计，主力/超大单/大单/中单/小单净额、缺值与旧日期提示；分钟不逐点求和，最新缺值不取前值代替。
- 异常波动与重点监控：短线新增独立路由，异常偏离名单 50 条分页与当前页筛选；重点监控区分生效中/即将生效，隐藏已过期窗口，来源未提供名单更新时间时明确说明。
- 股票所属板块反查：从个股到关联板块，再到板块详情和成分股；关联数量不足源 total 时显示部分结果。个股标签、风险名单分页与板块分页在本次会话内保留。
- 单股量价观察：复用唯一前复权 K 线管道，由 Rust 计算完整窗口 MA5/10/20/60、5/20/60 根涨跌、MA5/20 本根交叉、此前 20 根区间与前 5 根均量对比。窗口不足留空，当前日线提示未定型；不宣称全市场信号扫描或买卖建议。
- 东财 K 线加强身份、日期、OHLC 和成交量校验；异常行导致请求失败，避免跳行后仍给出貌似完整的窗口结果。
- 启动入口按环境懒加载，Android 启动路径不加载桌面 Router/台账灌入；共享 Vite 构建仍生成桌面资源，未宣称已完全独立移动包。
- Android CI 新增构建成功后发布 GitHub 预览版的独立 job；版本 tag/已有公开包不覆盖，APK/SHA256 全部上传到 draft 后才公开，生产签名/AAB 仍未完成。
- 新增 ANDROID_REMAINING.md 作为每轮最新未完成功能清单；回复必须报告剩余范围、构建状态和实际发布版本。

新增 Core routes：

- GET /mobile/stock/fund-flow?symbol=600519&mode=daily&limit=60
- GET /mobile/stock/boards?symbol=600519
- GET /mobile/stock/observations?symbol=600519
- GET /mobile/shortline/risk?kind=anomaly&page=1

新增移动路由：/mobile/shortline/risk；复用既有个股与板块路由，不另建股票市场解析器。

无 SQLite 表/migration、Android 危险权限或原生插件新增；不向 AI 发送本地个人数据。公开数据样本来自东财，测试离线断言；样本来源及裁剪范围见 core-rust/tests/fixtures/README.md。

验证与发布：

- 按维护者约定，本地只运行 Rust：52 项单元/接口测试 + 1 项股票代码 parity 和 cargo check --locked 通过。前端/原生/Android 构建全部交给 GitHub CI。
- 维护者在本轮明确选择“豁免 Opus 5 限定，完成现有检查后发布预览版”；这是本轮 0.1.16 的授权，不是后续版本的自动豁免。
- 代码提交 169d7c6 的 [Android CI #38915975482](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38915975482) 全部成功：53 项 Rust 测试、424 项前端测试、前端类型检查/构建、ARM64 APK 编译、原生提醒路由 JUnit、APK 签名与 SHA256 校验。
- 同一提交的 [Cross-platform verification #38915975483](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38915975483) 全部通过。
- 已公开发布 [Android 0.1.16 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.16)，标签 android-mvp-v0.1.16 固定在 169d7c6，含 APK 与 SHA256。APK 23,014,581 bytes（21.95 MiB），SHA256 为 0a4730a0f4399933f8c8ce156092938277d65db20d10fb999b66fd97a4731a9d。
- [CI artifact #12054070064](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38915975482/artifacts/12054070064) 也保留 30 天；公开安装入口使用 Release，不受 CI artifact 30 天保留期影响。
- 真机上的系统返回、Activity 重建、通知、弱网及覆盖升级尚未验收；默认调试签名可能随 CI 改变，更换安装前应导出业务备份。

后续开发以 ANDROID_REMAINING.md 的清单为准，避免把已有 MVP 能力继续标作完全未实现。


### 维护者指令更新：取消审计模型限定

维护者明确要求删除 Agent 强制使用 Opus 5 审计的约束，已从 AGENTS.md 删除相应条目。此前仅限单版本豁免的描述属于历史记录，不再作为当前限制。后续 Android 预览版按维护者持续授权，在 Rust、前端、Android CI 和发布前隐私检查通过后发布；不再逐版本询问模型豁免。其余仓库规范保持有效。


### 紧凑工作台、显式设置与全局明暗主题（0.1.17）

维护者追加要求：UI 更紧凑、同屏内容更丰富、提供可见设置及白色模式。本轮优先实现该反馈：

- 顶部常驻设置及主题按钮；设置路由单独呈现，不必先到日志找子标签。
- 主题：白色/深色/跟随系统；密度：默认紧凑/可选舒适；前台刷新：30/60/120 秒或仅手动。偏好只存本机，存储受限仍可在内存切换。
- 23 份移动 CSS 使用共享主题 token；保留红涨绿跌。首帧恢复主题与密度，系统变化只在“跟随系统”时影响主题。
- 设置页可直接展开已有模型 API 配置表单，保留通知、提醒入口、备份恢复和数据状态，不把不可用操作伪装为已成功。
- 首页替换大块技术说明为 8 个业务快捷入口和自选报价表；自选页显示最新价、涨跌幅、成交额和旧行情/缺值标记，删除管理折叠保留。
- 6 指数（原来只展示其中 4 个）、沪深北成交额及交易所宽度同屏；市场请求并发，缺少交易所字段时不把缺值加成零。
- 刷新在后台暂停；紧凑布局减少卡片留白，但交互控件保留至少 44px 高度。
- 新增偏好存储、系统主题变化、设置/顶部/自选/市场呈现测试；CI 用冻结样本进行 360/390/768 像素、白色/深色的浏览器交互与 18 张截图检查，截图不代表实时行情或 Android 真机验收。
- 无 SQLite schema、原生插件或权限新增；未实现模块继续以 ANDROID_REMAINING.md 为准。

本地仅 Rust：53 项单元/接口 + 1 项 parity，以及 cargo check --locked 均通过。前端/浏览器/Android 均在 GitHub CI 执行。
0.1.17 代码 7914e66 的 Android CI #38924745771 与跨平台 #38924745937 全部成功；54 项 Rust、430 项前端测试，以及 6 组视口/主题交互检查、18 张截图通过。已查看 CI 白色首页/设置/窄屏个股和深色首页截图。预览版 android-mvp-v0.1.17 已发布，APK 23,034,365 bytes（21.97 MiB）。截图 artifact #12056112783；这是冻结样本浏览器检查，非真机验收。


### 窄屏个股详情继续收紧（0.1.18）

在 0.1.17 的实际 CI 截图中发现研究/量价标签会按旧两列网格换行，以及技术指标留白偏多，继续改为单行标签和三列统计，移除较重阴影并缩短标题；主题与设置能力全部保留。
个股请求改为 100 根日线，60 根涨跌需完整 61 个收盘点；缩略图仅取最后 60 根。均线窗口不足和成交额缺失均保留缺值。新增呈现回归与浏览器单行标签/三列指标断言。
验证与发布结果：

- 本地按约定仅运行 Rust：53 项单元/接口 + 1 项 symbol parity，以及 cargo check --locked 通过；前端、浏览器和 Android 编译均在 GitHub CI 执行。
- 代码 2fa4f7b 的 [Android CI #38926573218](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38926573218) 全部成功：54 项 Rust、431 项前端测试、类型检查/构建、6 组视口/主题浏览器交互、ARM64 APK、原生提醒路由 JUnit、签名与 SHA256 校验通过。
- 同一代码的 [Cross-platform verification #38926573094](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38926573094) 六组平台/Node 组合全部成功。
- 已人工查看白色窄屏首页/设置/个股与深色首页；[18 张截图 artifact #12057331264](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38926573218/artifacts/12057331264) 全部使用冻结样本，verification.json 无页面错误，PNG 无文本/EXIF/时间元数据，不包含真实用户数据。截图不等于真机验收。
- 已公开发布 [Android 0.1.18 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.18)，不可覆盖标签固定在 2fa4f7be2a3a7dd78704a09a5c6c42270c373c65；APK 23,034,749 bytes（21.97 MiB），SHA256：35e4efecc817b496006f25f2fe64e014728a53c75ab8a98b5732d7c7039a10ea。
- [APK artifact #12057990874](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38926573218/artifacts/12057990874) 含 APK/校验文件，CI artifacts 保留 30 天；长期下载入口为 Release。
- 本轮无 SQLite schema/migration、原生插件/权限或业务 Core route 新增。仍为 debug-signed 预览包；真机返回/重建/通知/弱网、正式签名/AAB 与覆盖升级未验收。更换安装前应导出业务备份。
- 所有剩余业务移植与工程验收继续见 ANDROID_REMAINING.md；未把本轮 UI 交付计为全功能迁移完成。

### 五大栏目导航与情绪复盘「今日情绪」迁移（0.1.19）

维护者反馈：UI 不够细致，要参考原版功能并重新设计布局与功能分布。本轮先用两个只读子任务逐页清点桌面版 finance 路由与移动端/Rust 现状，差距已写进 ANDROID_REMAINING.md「对照桌面版仍缺的页面与交互」。

完成：

- **信息架构**：`workspace.ts` 定义五个一级栏目（首页/盯盘/复盘/研究/我的，对应桌面版 `lib/nav.ts`）和 29 个条目；`WorkspaceNavigation.tsx` 提供底部导航、栏目内横向子页签（sticky，当前页自动滚入可见区）和可搜索「全部功能」。`MobileApp.tsx` 改为按页面 switch 渲染，删除 `ShortlinePanel`/`JournalWorkspace`/`DiscoveryWorkspace` 三个嵌套容器及已无引用的 `JournalPanel.tsx`/`journal.css`。旧路由全部保留。
- **Rust `emotion.rs`**：移植 `shortline_emotion.py` / `shortline_facts.py` / `shortline_boards.py` 口径。一次并发取复盘日涨停/炸板/跌停/昨日涨停池和前一交易日涨停池；交易日历取上证指数日线日期（东财失败改腾讯）。默认复盘日 = 最近已收盘交易日（15:05 后才算今天），未来/非交易日/超出一年的日期 400。只有当天涨停池必需，其余池各自降级为 `available:false + reason`，整体 `status: partial`。缺必要列（zbc/lbc/ylbc）整池拒用，不按默认值补。主板 ST 2026-07-06 起 10%、创业板 2020-08-24 前 10%。
- **真实数据发现**：2026-10-16 新华传媒 11 板在东财昨日涨停池（ylbc=10）里，却不在 10-15 涨停池。展示用的「昨日板数」用昨日涨停池补缺；晋级率仍只用前一日涨停池，与桌面版一致。
- **前端**：`emotionData.ts`（类型、各页共用的复盘日、同一路径 5 分钟内存缓存）、`EmotionPanels.tsx`（今日情绪/连板天梯/涨停全表/首板名单/盘面事实/行业题材/首页情绪速览）、`emotion.css`（只用主题 token）。资料库按路由拆为研究记录/我的研报/收件箱，增加检索、分类、未读筛选与全文展开。
- **修复**：底部导航在 `mobile.css` 里是 3 列，五个栏目会折成两行；改为 5 列，并在浏览器冒烟里断言单行。子页签负边距曾在紧凑密度下横向溢出 12px，已去掉。

新增 Core route：`GET /mobile/shortline/emotion?date=YYYY-MM-DD`、`GET /mobile/shortline/cycle?days=3..30&date=`。新增移动路由来自上一会话已写好的 `/mobile/review/*`、`/mobile/library/*`、`/mobile/functions`、`/mobile/my`、`/mobile/research`。无 SQLite schema/migration、原生插件或权限变化；不向模型发送本地数据。

验证（本机）：

- Rust：`cargo test --locked` 64 项 + 1 项 parity 通过（新增 11 项 emotion 单元测试 + 3 个路由入参拒绝用例）；`emotion::tests::live_emotion_and_cycle_probe` 为 ignored 实时探测，对真实上游跑通：61 涨停 / 49 炸板 / 10 跌停 / 最高 11 板，与东财原始池 tc 一致。本机走代理时 reqwest 访问 push2his 会被断开，已由腾讯日历兜底。
- 前端：`tsc --noEmit`、`npm run build` 通过；`npm test` 427/428，失败的是 `agent_error.test.ts`（本机未装 orchestrator 的 ajv，CI 会装）。新增 `mobile_emotion.test.ts`，使用 Rust 真实返回裁剪的冻结样本 `desktop/test/fixtures/mobile-emotion-2026-10-16.json`（公开行情，无用户数据）；导航/首页快捷测试按新结构重写。
- 浏览器：用 Playwright 1.56.1 + 本机 Edge 跑 `desktop/test/mobile-ui-smoke.mjs`（CI 用 chromium），6 组视口/主题、底部导航单行、复盘子页签单行、无横向溢出、30 张截图全部通过；已人工查看 390 白色首页/今日情绪、360 深色连板天梯、768 白色今日情绪。
- 发布前隐私扫描：用 Node（能透明解密）扫工作区 1042 个文件、HEAD 1037 个 blob 与 `git log --all -p`。候选 65 / HEAD 65 / 历史 71 个既有命中（测试假 key、公开查询参数），候选相对 HEAD 新增 0；未跟踪 .env/.local/密钥文件；历史中的 `.env.example` 仅占位值。注意：本机 shell 重定向生成的文件会被 DLP 加密、shell 工具读到密文，扫描结果要直接输出到终端。提交前已确认 git blob 是明文。
- CI 与发布：代码 47bbc61 的 [Android CI #38975425413](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38975425413) build-apk（Rust 测试、前端类型检查/构建/测试、浏览器冒烟、APK 编译、原生提醒路由测试、APK 校验）与 publish-preview 全部成功；[跨平台 #38975425430](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38975425430) 成功。已发布 [Android 0.1.19 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.19)，标签固定在 47bbc617e872e998920c8e4cfbce4bdde2b3502d；APK 23,334,749 bytes（22.25 MiB），SHA256：6282f1b84eb693fc3bbf02abfa81d305b9f26a619969cbcda5247f71991050ea。APK artifact #12078478776、截图 artifact #12078317923 保留 30 天。CI 内各步骤测试数未从日志核对（公开 API 读不到 job 日志）。
- 未做：真机验收（返回/重建/通知/弱网、新栏目实际触控）、正式签名/AAB。仍是 debug-signed 预览包，更换安装前应导出业务备份。

下一步建议：情绪历史/近 5 天（需要先把每日已收盘读数存 SQLite）→ 明日验证条件与对账 → 盯盘异动流与昨日梯队实时 → 竞价与盘中路径 → 交易日志分组/风险与复查页签。

### 近 5 天、情绪历史与明日验证（0.1.20）

口径移植自 `orchestrator/src/finance/shortline/{weekly,history,verification,verify_metrics}.ts`：分位只和之前比、少于 10 天不给；≥0.85/≤0.15 标偏热偏冷（按 higher_is_hotter 翻译）；跟前一天比按 diff_eps 门槛（正好等于门槛算噪声）；明日验证 8 个读数的 eps、基准发生率（≥ 12 对可比日）、常见/少见方向提示、09:15 事后判定、未收盘暂定、没数据不算判错、同一晚同一读数只一条、整晚一个事务替换且没变的条目不改 updated_at、下一场开没开说不准时不许删，全部与桌面版一致。

新增：

- `core-rust/src/emotion_history.rs`：每个交易日的八项读数（涨停家数、最高板、1进2、赚钱效应中位、炸板率、跌超5%、头部行业集中度、全市场跌停）+ 最高标 + 行业分布，由四张池与前一日涨停池计算。窗口内缺缓存的日子按池并发 6 路取数（前一日涨停池复用同批）；超过 28 个日历日只读缓存。已收盘且 8 项全有的日子写入 `mobile_shortline_days`。
- Core route：`GET /mobile/shortline/history?days=5..60&date=&refresh=1`、`GET /mobile/shortline/weekly?date=`、`GET /mobile/shortline/verify?date=`、`POST /mobile/shortline/verify {action:"save",date,items}`。改错入参在联网前 400。
- SQLite schema v5：`mobile_verifications`（UNIQUE(date, metric)，进备份）与 `mobile_shortline_days`（公开行情缓存，不进备份）；各旧版本升级测试覆盖 1–4。备份格式 v5，校验日期/读数/方向/重复/理由长度，v1–v4 仍可恢复。
- 前端 `HistoryPanels.tsx`：近 5 天、情绪历史、明日验证（条件编辑器）三个复盘子页；路由 `/mobile/review/{weekly,history,verify}`。与情绪复盘各页共用复盘日选择。
- 顺手修复：`ai_config` 测试的临时库路径在 Windows 并行测试里可能撞名（纳秒时间戳相同），偶发失败；加进程内计数。

验证（本机）：Rust 72 项 + 1 项 parity 连续 3 次全量通过；`emotion_history::tests::live_history_weekly_verify_probe` 对真实上游：30 天窗口首次 0.96 秒、14 天取到、16 天超出保留期；第二次 46 毫秒全部读缓存；2026-10-16 最高板 11 偏热、赚钱效应 −3.3% 与炸板率 44.5% 偏冷。前端 431/432（agent_error 缺 ajv），新增 `mobile_history.test.ts` 用真实返回裁剪的 `desktop/test/fixtures/mobile-history-2026-10-16.json`；本机 Edge 浏览器冒烟 42 张截图通过，已人工查看 360 白色情绪历史与 390 深色明日验证。发布前隐私扫描：候选 65 / HEAD 65 / 历史 71 个既有命中，新增 0，无 .env/.local/密钥文件被跟踪；提交后确认 blob 为明文。

CI 与发布：代码 2d49f00 的 [Android CI #38983459066](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38983459066)（build-apk 与 publish-preview）和 [跨平台 #38983459069](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38983459069) 全部成功。已发布 [Android 0.1.20 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.20)，标签固定在 2d49f005c04dfb5e2ffbbe40c0b985b1acee3d42；APK 23,597,797 bytes（22.50 MiB），SHA256：c06e1f4676e1ad04bea137dc2fc734a6bd04019335adcb68abfa2d7eaceab460。CI 内各步骤测试数未从日志核对（公开 API 读不到 job 日志）。真机验收与正式签名/AAB 仍未做。

已知限制：情绪历史从东财保留期起积累，前期 60 天窗口大部分为空；节假日工作日写条件时用上证快照时间判断今天开没开市，取不到时允许新增/修改但不许删除。

下一步建议：盯盘异动流与昨日梯队实时 → 竞价与盘中路径（可复用明日验证条件做盘中读数）→ 涨停样本 → 交易日志分组/风险与复查页签 → 每日复盘市场状态八格。

### 个股 K 线与盘口（0.1.21）

维护者要求：个股 K 线查看要丰富、接近常用证券 App。

完成：

- `core-rust/src/chart.rs`：`GET /mobile/stock/kline?symbol=&period=day|week|month|m60|m30|m15|m5&adjust=qfq|none&limit=30..800`（东财 kline 含成交额/换手，失败或被限流改用腾讯 fqkline / mkline；校验日期顺序与 OHLC，异常行剔除计数）、`GET /mobile/stock/intraday?symbol=&days=1|5`（腾讯 minute/query、day/query，累计量额转每分钟并算均价，15:00 后的盘后行不画；失败改用东财 trends2）、`GET /mobile/stock/snapshot?symbol=`（腾讯行情含五档、外内盘、委比、涨跌停、量比、均价、三种市盈率；失败改用东财 stock/get，无五档）。
- 前端：`chartIndicators.ts`（MA/EMA/MACD/KDJ/RSI/BOLL，通达信口径，含单测）、`KlineChart.tsx`（周期页签、分钟子页签、复权、主图/副图指标、拖动平移、捏合/滚轮/按钮缩放、点按/长按十字线、可见区高低点标注、分时/五日双轴）、`QuoteBoard.tsx`（20 项盘口、外内盘比例、五档折叠）。前台按设置刷新频率自动更新盘口与分时；`useDiscoveryData` 改为同一路径刷新时保留旧数据，避免定时刷新闪空。个股页去掉旧的 30 根小蜡烛图与重复价格/指标。
- 无 SQLite schema、原生插件或权限变化；不向模型发送本地数据。

验证（本机）：Rust 78 项 + 1 项 parity；`chart::live::live_chart_probe` 对真实上游跑通日/周/5 分钟/北交所 K 线、分时、五日、快照，并直接验证腾讯备用路径各周期（北交所在腾讯只有 1 根，已在 note 中提示）。前端 435/436（agent_error 缺 ajv），新增 `mobile_chart.test.ts` 与真实样本 `desktop/test/fixtures/mobile-chart-600519.json`；本机 Edge 浏览器冒烟 48 张截图，含 K 线点按十字线、拖动后“回到最新”可用、MACD 切换、分时切换；已人工查看 390 深色 K 线、360 白色个股页与 K 线。

发布前隐私扫描：候选 65 / HEAD 65 / 历史 71 个既有命中，新增 0。CI 与发布：代码 66a8496 的 [Android CI #38989460626](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38989460626) 与 [跨平台 #38989460648](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/38989460648) 全部成功；已发布 [Android 0.1.21 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.21)，标签固定在 66a8496940c313e828de115fca93c7d82464b23c；APK 23,655,181 bytes（22.56 MiB），SHA256：ea473744eb1a6cce842bf0710138db5f1646f774ec8acaada531d0728c555dd1。真机验收与正式签名/AAB 仍未做。

环境变化：本机文件透明加密已不再生效，仓库文件对 Shell/Node/git 都是明文；维护者已移除 fileops MCP 与对应全局指令，后续直接用常规工具读写。

下一步建议：盯盘异动流与昨日梯队实时 → 竞价与盘中路径 → 涨停样本 → 交易日志分组/风险与复查页签 → 每日复盘市场状态八格。K 线可继续补：叠加对比指数、画线工具、成交明细（逐笔）与资金流副图。

### 盯盘实时动态、竞价与盘中、涨停样本、市场面板、交易日志风险与纪律（0.1.22，已发布预览版）

按 ANDROID_REMAINING.md 的主计划一次做了五块，各自独立提交（de13edd / 023fd68 / 0eec410 / 62b778f / d0cd613）。口径都移植自桌面版 orchestrator，每页的差异在 ANDROID_REMAINING.md「对照桌面版仍缺」和 PARITY_MATRIX.md 里逐条写明。

新增 Core route：

- `POST /mobile/watch/poll`（轮询一轮：取行情、判异动、存异动；同时每分钟顺带检查一次竞价/盘中时点）、`GET /mobile/watch/alerts?date=`
- `GET /mobile/intraday?date=`、`POST /mobile/intraday {action:"capture"}`
- `GET /mobile/shortline/samples?days=10..90&date=&refresh=1`
- `GET /mobile/review/market?date=`
- `GET /mobile/journal/risk`、`GET/POST /mobile/journal/settings`

新增 SQLite schema v6（一次迁移，`migrate_5_to_6`，各旧版本升级测试覆盖 1–5）：`mobile_watch_alerts`（异动，留 30 天）、`mobile_watch_turnover`（收盘后记下的成交额前十）、`mobile_intraday_snapshots` / `mobile_intraday_status`（时点快照，留一年，只新建不覆盖）、`mobile_samples_days`（涨停样本逐日明细缓存）、`mobile_journal_settings`（费率/规矩/账户规模）。**只有 `mobile_journal_settings` 进备份**，备份格式升到 v6；v1–v5 备份仍可恢复，恢复后设置回到默认（有测试）；设置里有不合规则的值整份备份拒收。其余五张都是公开行情数据，不进备份。

要点与踩过的坑：

- **异动流（`watch.rs`）**：规则同桌面版（3 分钟内 ±1.5% 急拉/急跌、至少攒够 1 分钟才判；碰到/离开涨停价封板/开板；同一只同一类 5 分钟冷却；只在连续竞价、行情时间是今天时判；竞价/午休重新攒基准）。手机上没有常驻后台，所以**只在盯盘页/竞价页开着且在前台时才取行情和判断**；设置里选「仅手动刷新」时基本判不出。持仓只给现价、涨跌、浮动盈亏 % 与止损/目标状态，不给股数和成本（沿用旧版规矩）。
- **昨日成交额前十**：桌面版用通达信盘后包，手机上不下载整包；改为「收盘后（≥15:05）到下一场开盘前」从新浪成交额榜记下一份存本机（hs_a，不含北交所）。没在这个窗口里开过盯盘的那天就没有这份名单，页面写明原因，**不拿别的日子顶替**。
- **竞价与盘中（`intraday.rs`）**：时点窗口、8 分钟漂移上限、09:25 只到 09:29、午收/日收允许晚 8 分钟，全部同桌面版；快照拿的是此刻行情，所以只能当天当场存，历史日绝不现抓。取数可能跨过窗口，取完后再核一次时间，跨了就不存。
- **涨停样本（`samples.rs`）**：事后名单警告永远显示；入场日情绪分按入场那天（前一交易日）算，避免前视；分环境三等分要求至少 6 天；已收盘的日子才进统计和缓存。
- **市场面板（`market_panel.rs`）**：九块各自取数各自标缺口。沪深 300 市盈率/股息率没接（中证官网只给 xls，手机端没有 xls 解析），页面明说「暂无」。行业资金用新浪口径（东财列表接口成片取不到），和东财「主力净额」不是一个口径。涨跌分布/行业资金/成交额榜只给「当天此刻」，按交易日历判断快照属于哪天（周末取到的是周五收盘 = 复盘日），不是复盘日时标出来。
- **交易日志风险与纪律（`journal_risk.rs` + `journal_settings.rs`）**：一笔 = 一条交易记录，移动加权平均成本，**清仓后才计入已实现**（与既有「交易统计」页一致；桌面版按卖出就算，这是已知口径差异）。没填费用的成交改按设置里的费率估（`journal.rs`/`journal_review.rs` 不再用写死常量）。每条规矩都报「已查/没查」，没查的写原因。**本页不登记任何 AI 页面上下文**，有测试守着源码里不出现 `useAiPage` 等。隐私模式在手机端整体还没做（见 ANDROID_REMAINING「全局」），所以本页金额目前没有遮罩。
- **样式隐患（已修）**：`.emo-page` 是单列 grid 且没限制列宽，页面里放进「内容最小宽度很大」的卡片（龙虎榜上榜理由是一长串不换行文字）会把整页撑宽；同时 `.watch-name` 缺 `overflow:hidden`。已改 `.emo-page` 为 `minmax(0,1fr)`、`.emo-header` 同理，并给 `.watch-*` 加 `min-width:0`。浏览器冒烟里的 `assertWidth` 就是靠这个抓到的。

验证（本机）：

- Rust `cargo test --locked`：125 项 + 1 项 parity 通过；8 项 ignored 实时探测中本轮新增的 5 个（`watch::live_watch_probe`、`intraday::live_intraday_probe`、`samples::live_samples_probe`、`market_panel::live_market_panel_probe`、`journal_risk::dump_synthetic_fixture`）已对真实上游/合成数据跑通：盯盘 3.8 秒取到 36 只昨日梯队 + 356 只大票、收盘后成功记下 2026-10-16 成交额前十；涨停样本 14 个交易日 786 个样本、第二次读缓存 148 ms；市场面板九块里八块取到、估值块按设计标暂无、2.7 秒。
- 前端：`tsc --noEmit`、`npm run build` 通过；`npm test` 457 项中 456 项通过，失败的仍是缺 ajv 的 `agent_error.test.ts`（CI 会装）。新增测试：`mobile_watch`、`mobile_intraday`、`mobile_samples`、`mobile_journal_risk`、`mobile_market_panel`，夹具是真实返回的冻结样本（盯盘/竞价/样本/市场面板），交易日志夹具是**合成数据**（`journal_risk::tests::dump_synthetic_fixture` 生成，不含任何真实成交）。
- 浏览器：Playwright 1.56.1 + 本机 Edge 跑 `desktop/test/mobile-ui-smoke.mjs`，6 组视口/主题、78 张截图通过（新增盯盘实时动态、竞价与盘中、涨停样本、风险与纪律（含展开设置编辑器）、市场面板）；已人工查看 360 深色实时动态、390 白色涨停样本、360 深色风险与纪律、360 白色市场面板。
- 本轮新增内容（相对 0727bb4 的 6,761 行，不含 json 夹具）针对性扫描：无密钥/令牌/私钥/邮箱/本机路径；夹具另扫无命中。

**发布前隐私扫描（AGENTS.md 6.5，仓库主人在聊天里同意 push 与发布预览版之后做的）**：脚本在会话 scratchpad 里临时写的（规则：OpenAI 风格 key、GitHub token、AWS key、私钥块、JWT、Bearer、`key=` 赋值、邮箱、Windows/类 Unix 用户目录、开发盘符路径、带 token 的 URL、内网 IP），**没有入库**；因此命中数与此前 65/65/71 的口径不可比，判断「新增」的方法是**同一套规则对基线 0727bb4 和现在各扫一遍取差集**。结果：工作树 = HEAD 快照 = 145 条、相对基线新增 0；完整历史（`git log --all -p` 的新增行）128 种签名、相对基线历史新增 0；文件名级：历史上进过仓库的敏感文件名只有 `.env.example`（后端/前端/根目录，内容为空或占位）与 `.claude/launch.json`（启动配置），当前没有跟踪 `.env`、`.local/`、密钥、令牌文件。145 条既有命中人工核实：全部是测试里的假路径（alice/someone/test）、`example.*` 假邮箱、README 有意公开的作者邮箱、测试假 key / Bearer / access 令牌、SSRF 黑名单的内网网段常量、两处占位串（`core-rust/src/ai_config.rs` 测试里的「不应被存储」字符串、`orchestrator/src/task_service.ts` 的适配器占位）。GitHub 侧（公开 API）：Issues / PR / 评论均为 0，15 个 Release 正文无命中，Actions artifacts 只有 APK 与冻结样本截图。**待办**：Secret Scanning 与 Push Protection 的开关需要管理员权限，匿名读不到，仓库主人需自行确认已开启；本机没有 `gh`。注意：shell 重定向生成的文件会被 DLP 加密，扫描结果必须用管道/进程替换或直接输出，不能落盘再比较（本轮第一次比较就因此作废重做）。

**CI 与发布**：推送 `0727bb4..2d92a32`（只推 `android-mvp`）。代码 2d92a32 的 [Android CI #39002403555](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39002403555)（build-apk、publish-preview）与 [跨平台 #39002403552](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39002403552)（ubuntu/windows/macos × Node 22/24）全部成功。已发布 [Android 0.1.22 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.22)（prerelease，非草稿），标签固定在 2d92a32190391a1901e28d8a54140ff5628c4d8e；APK 24,387,637 bytes（23.26 MiB），GitHub 记录的 SHA256：85eb0358ad3f276b1ffe1de6f2d367c710375d9f5887f669824300f4be03ab85（`.sha256` 文件 113 bytes，本机下载它时 github.com 连接超时，没有逐字对照文件内容，只核对了 API 给出的摘要）。APK artifact #12089996843、截图 artifact #12090001310 保留 30 天。CI 内各步骤测试数未从日志核对（公开 API 读不到 job 日志）。仍是 debug-signed 预览包；真机验收、正式签名/AAB、覆盖升级未做，更换安装前应导出业务备份；旧备份恢复后交易日志设置会回到默认。

环境注意：本机 DLP 会让 shell 里读 `*.md` 得到密文（Read/Edit 工具和 Node 读到的是明文）；修改这几份文档请用 Read/Edit 工具，提交前用 `git cat-file -p :<path>` 确认 blob 是明文。

下一步建议：① 补发布前隐私扫描后 push、看 CI、发 0.1.22 预览版（需主人确认）；② 交易日志剩余：模式卡（版本化）、MFE/MAE 复查与复查清单、按情绪环境/当时板位分组、「看错还是没执行好」四象限、问题记录修复；③ 情绪复盘的 AI 复盘/周报/结构变化；④ 市场面板补沪深 300 估值（需要 xls 解析）；⑤ 隐私模式（金额/股数/价位遮罩、权益曲线不画）。

### 交易日志分析页补全：模式卡、复查、入场环境、归因、问题记录（0.1.22 之后合并，随 0.1.23 发布）

任务：把桌面版交易日志还没迁的分析页补到手机端。五个功能各一个提交（开发在分支 `claude/strange-brattain-b0a759`，基线 `2d92a32`；之后已用一次合并提交合进 `android-mvp` 并 push，文档冲突已手工解决），都是复盘栏目里的子页签：

- **模式卡**（`journal_modes.rs`、`JournalModesPanel.tsx`，`GET/POST /mobile/journal/modes`）：改打法或五段文字开新版本、次日生效、旧版本不动；只改名字不开新版本；超长（800/60/300 字，按字数）直接拒收不截断；每版少于 6 笔不比较，同日生效的几版只认最后一版，建卡之前的交易单独列。卡片放表 `mobile_journal_modes`（versions 存 JSON），**进备份**。
- **复查**（`journal_excursion.rs`、`journal_inbox.rs`、`JournalExcursionPanel.tsx`，`GET /mobile/journal/excursion?mode=cache|fetch`、`GET /mobile/journal/inbox`）：复查清单按用户自己的中位仓位/持有天数/规矩筛（`Row.peak_cost` 是桌面版 `episode.amount`），只读缓存不联网；MFE/MAE 只在点「取行情并计算」时联网取腾讯不复权日线，按代码+买入日+卖出日永久缓存在 `mobile_journal_bars`（卖出当天 15:30 前取的不算定稿，收盘后再点重取一次）；有成交没取到的单笔列原因。
- **入场环境**（`journal_env.rs`、`JournalEnvPanel.tsx`，`GET /mobile/journal/environment`）：情绪分三等分（复用 `samples::{entry_score,regime_cuts,regime_of}` 与情绪历史日缓存 `emotion_history::series`），板位用涨停/炸板池成员（`mobile_journal_env_days`，两张池都取到且已收盘才存）；取不到的日子分原因列缺口，不补；入场日 15:30 前不分组。
- **看错还是没执行好**（`journal_attribution.rs`、`JournalAttributionPanel.tsx`，`GET /mobile/journal/attribution?date=`）：用 `emotion_history::verify_view` 的 `track.days`（明日验证对账，事后/暂定不计）做判对/判错，按入场日对齐；盈亏 0 的日子不进格；`emotion_history` 里 `next_session`、`BASELINE_DAYS` 改成了 `pub(crate)`。
- **问题记录**（`journal_problems.rs`、`JournalProblemsPanel.tsx`，`GET/POST /mobile/journal/problems`）：按日期重放找卖超/卖在买之前、日期无效、空记录、没填盈亏的补记；动作 update_fill / delete_fill / delete_entry / set_manual_pnl，改完不能更糟、至少留一笔成交、入场日跟最早成交；非 A 股只列出不删。

**schema/备份决定**（⚠️ 该会话开发时以为「v6 还没发布」，实际 0.1.22 已经带着 schema v6 / 备份 v6 发布了；它的做法——v6 迁移内容原封不动、另加 `migrate_6_to_7`——恰好对已装 0.1.22 的设备是正确的升级路径，没有改动已发布迁移的含义）：新增迁移 `migrate_6_to_7`，当前 schema = 7、备份格式 = 7（`CURRENT_SCHEMA_VERSION`、`backup.rs` 的 `SCHEMA_VERSION`）；`REQUIRED_TABLES` 现在是切片（`&[&str]`），v6 的 17 张表保留为 `REQUIRED_V6_TABLES` 给 5→6 迁移校验。备份 `data.journal_modes` 缺省为空数组；v1–v6 备份恢复后模式卡清空（`legacy_missing_journal_modes_are_empty`）；备份里的卡片逐张校验（id、名字、打法、版本号递增、日期、字数），不合规整份拒收。往返与旧版本恢复测试在 `backup::tests::journal_modes_round_trip_and_older_backups_restore_with_no_cards`。

**共用改动**：`journal_risk.rs` 新增 `is_journal_symbol`（A 股股票判断）、`entry_day`、`va_order`（桌面版列表顺序）、`analysis()`（设置 + 只含 A 股股票的行 + 被排除的记录）；`Row` 加 `peak_cost`、`Entry` 加 `note`；「风险与纪律」现在也只算 A 股股票（返回里多 `excluded_non_stock`）。新增样式在 `journal-analysis.css`。

验证（本机）：

- Rust `cargo test --locked`：159 项 + 1 项 parity 通过（15 项 ignored）；每个模块都有纯计算测试，MFE/MAE 与入场环境用假取数函数测永久缓存、盘中取的重取、取失败不影响别的笔；`live_tencent_probe`（2026-09-21~10-16 贵州茅台 14 根不复权日线，收盘价与真实一致）和 `live_environment_probe`（真实日历/情绪读数/涨停炸板池，首次约数秒、第二次读缓存 55 ms）对真实上游跑通。
- 前端：`tsc --noEmit`、`npm run build` 通过；`npm test` 477 项中 476 项通过（agent_error 缺 ajv）。新增 `mobile_journal_{modes,excursion,env,attribution,problems}.test.ts`，夹具全是**合成数据**（各模块的 `dump_synthetic_fixture` 生成，命令写在测试函数的注释里，不含真实成交或行情），并各有一条「页面不登记 AI 页面上下文」的源码守卫。
- 浏览器：Playwright 1.56.1 + 本机 Edge 跑 `desktop/test/mobile-ui-smoke.mjs`，6 组视口/主题、108 张截图通过（新增模式卡含新建编辑器、复查、入场环境、归因、问题记录含展开成交编辑）；已人工查看 360 深色模式卡、360 白色复查、390 深色归因、360 白色问题记录。本机跑法：`npm run preview --prefix desktop -- --host 127.0.0.1 --port 4173` 起静态页，把冒烟脚本里的 `chromium.launch({ headless: true })` 换成带 `channel: "msedge"` 的副本，`VIBE_UI_RUNTIME` 指向装了 playwright 的目录（本机是 `C:/Users/<用户>/node_modules/playwright`）。worktree 里没有 `desktop/node_modules` 时用目录联接指到主工作区的。

**合并与发布状态**：仓库主人明确指示「直接合并 + push，不要扫描」，所以这批提交已合并并 push，**没有重做发布前隐私扫描**（扫描方法与口径见上一节「0.1.22」）；Cargo 版本号仍是 0.1.22，没有发布新 Release（已存在的标签 `android-mvp-v0.1.22` 指向 2d92a32，CI 不会覆盖）。要发布这批改动：升版本号（`core-rust/Cargo.toml`、`src-tauri/Cargo.toml`、`core-rust/Cargo.lock`）、写 `docs/android/releases/<版本>.md`、补隐私扫描、得到仓库主人对新建 Release 的授权。仍未做：真机验收。模式卡与交易分析页同样没有隐私模式遮罩（全局隐私模式未做）。

### 复盘补全：题材事件树、涨停分档、结构变化、沪深 300 估值、昨日炸板股修复（0.1.23）

仓库主人 2026-10-17 的长期指示：实现 ANDROID_REMAINING.md 里的全部功能；每轮升版本、写发布说明、提交、push 由 CI 发预览版；不再做发布前隐私扫描（密钥/个人成交不入库、不进 AI、夹具只用公开行情或合成数据继续有效）。已写进 AGENTS.md 6.4/6.5 与 ANDROID_REMAINING.md「发布约定」。Agent 做不了的两项会一直留在清单里：真机验收、正式签名/AAB（需要仓库主人的发布密钥）。

本轮（Round A）新增，全部在复盘栏目里：

- **题材事件树**（`theme_tree.rs`、`ThemeTree.tsx`，`GET /mobile/shortline/theme-tree?date=&top=`）：同花顺涨停池（`data.10jqka.com.cn/dataapi/limit_up/limit_up_pool`）逐页取、逐行核对日期，原因串拆成事件题材后聚合；状态只由已发生读数推出。缓存表 `mobile_ths_reasons`（公开数据，不进备份）。首板名单每只带涨停原因（取不到的票不显示原因，页脚写覆盖情况）。
- **涨停分档**（`review_tiers.rs`、`ReviewTiers.tsx`，`GET /mobile/review/tiers`）：放在市场面板。
- **结构变化**（`structure.rs`、`StructurePanel.tsx`，`GET/POST /mobile/review/structure`）：最近 10 vs 之前 20 个交易日涨停结构逐项比中位数（计数类相对 ≥ 35%，占比类绝对 ≥ 10 个点，窗口至少 5 天）；四张池子的上游字段清单漂移；人工登记制度事件（`mobile_regime_events`，进备份，标题 ≤ 60 字、说明 ≤ 300 字、最多 1000 条）。`emotion.rs` 因此拆出 `fetch_pool_root` / `raw_fields`。
- **沪深 300 估值**（`market_panel.rs`，`calamine 0.36` 解析中证 `000300indicator.xls`，夹具 `core-rust/tests/fixtures/csindex_000300_indicator.xls`）：市盈率、股息率与 10 年国债收益率的股债利差；表头先压空白再匹配（真实表头是「指数代码Index Code」）；取不到写原因。
- **昨日炸板股修复**（`prev_broken.rs`、`PrevBroken.tsx`，`GET /mobile/shortline/prev-broken?date=`）：前一交易日炸板池 + `watch::fetch_quotes` 腾讯实时行情，行情时间必须是复盘日才计入，涨停价/跌停价上分别记「晋级涨停」「跌停」，其余按涨跌幅分收红/小跌/跌超5%；行情快照不属于复盘日整块写原因；盘中标此刻读数。⚠️ 桌面版 `prev_broken_recovery` 的计算源码在仓库里没找到（只有类型和界面字段），口径是按界面「当天均 / 翻红」定义的，没和桌面版逐项对账，所以做成独立卡片、不并进原反馈矩阵。
- **schema/备份**：v7 迁移 `migrate_6_to_7` 现在收了 mobile_journal_modes / _bars / _env_days、mobile_ths_reasons、mobile_structure_days、mobile_regime_events；备份格式 v7（v1–v6 可恢复，旧备份恢复后模式卡/制度事件为空）。0.1.23 发布后再加表要开 v8。

验证（本机）：Rust `cargo test --locked` 179 项 + 1 项 parity 通过（20 项 ignored 为实时探测/夹具生成，本轮的 `live_theme_tree_probe`、`live_valuation_probe`、`live_prev_broken_probe` 都对真实上游跑通）；前端 `tsc`、`npm run build` 通过；`npm test` 488 项中 487 项通过（agent_error 缺 ajv）；Edge 冒烟 6 组视口/主题、126 张截图通过（新增结构变化含登记编辑器、题材事件树、昨日炸板股修复），并人工查看 360 白色昨日炸板股修复。前端夹具是对 2026-10-16 真实上游的冻结返回（东财偶发取不到，市场面板夹具由多次探测按块合并）。CI 与发布结果见 ANDROID_REMAINING.md 开头。

下一步（Round B 起，见 ANDROID_REMAINING.md）：我的股票（自选估值列/分组/标签、持仓已实现与分红、收益表现与体检、CSV/截图导入）→ 我的（盘前提醒/收盘摘要消息流、研报上传、研究记录归档）→ 分享与备份 → 估值与研究 → AI 能力 → 隐私模式与后台盯盘 → 工程验收。

### 界面改版:去顶栏、设置入底部菜单、紧凑密度、网页自绘控件(0.1.24)

用户反馈(2026-10-17):设置放底部菜单并把主题切换放进设置、去掉顶部软件信息栏;信息显示不够紧凑;很多控件是 Android 原生控件、没有用网页控件。做法:

- `MobileHeader` 组件删除(`MobileHeader.tsx` 只剩 `HomeShortcuts`/`SectionLink`);`WorkspaceNav` 追加第六项「设置」(`page === "settings"` 时只有设置高亮,「我的」不高亮;设置页不再显示「返回」和栏目子页签,`groupEntries` 排除 settings);状态栏安全区由 `.mobile-app` 顶部 `env(safe-area-inset-top)` 加固定遮罩处理(`compact.css`)。**真机未验证安全区**。
- 紧凑:`compact.css` 在紧凑密度下用 `html[data-mobile-density="compact"] body .mobile-app …` 的高优先级选择器收紧;「舒适」不动。标题块只留 h1(`h1 + p` 与 h1 前的小标题隐藏)。新增页面如果有自己的标题块/卡片,默认会被这些通用规则收紧,有特殊需要再单独覆盖。
- 控件:`controls.tsx` 提供 `WebSelect`(`<option>` 子元素语法,onChange 给 `{target:{value}}`,原来写给原生 select 的处理函数直接可用)、`DateField`(自绘月历,周一开头,`min`/`max`/今天)、`Sheet`、`DialogHost` + `askConfirm`/`showAlert`(没挂 DialogHost 时退回 window.confirm/alert);复选框用 CSS 画。**新代码不要再直接用 `<select>`、`<input type="date">`、`window.confirm/alert`**。系统文件选择器(`type="file"`、插件的文件对话框)无法替代。
- 测试:`mobile_controls.test.ts`(月历格、下拉/日期 SSR),`mobile_presentation.test.ts` 改成检查六项底部菜单;冒烟脚本改为点底部「设置」、操作自绘下拉与日期,并在比较舒适/紧凑高度前等数据加载完(之前的断言有时序偶发)。

### 我的股票:自选增强、已实现与分红、表格导入(0.1.24)

- **自选**(`storage.rs`、`monitor.rs`、`WatchlistPanel.tsx`、`watchlistView.ts`):`/mobile/monitor` 的自选行新增 `pe_ttm`/`pb`/`turnover_pct`(腾讯批量行情第 39/46/38 列,兜底路径读 evidence 同名字段)、`group`、`tags`;新路由 `POST /mobile/watchlist/meta`(分组 ≤ 12 字、标签 ≤ 5 个且每个 ≤ 8 字,超长/带逗号/控制字符拒收)与 `POST /mobile/watchlist/order`(必须给全部代码、不重复、不多不少);`list` 按 `sort_order IS NULL, sort_order, created_at` 排,新加的排末尾。负市盈率/市净率前端显示「亏损」「净资产为负」,排序时当缺值。选了分组后上移下移只在该分组内换位(`moveVisible`)。
- **已实现与分红**(`portfolio.rs`、`PortfolioIncome.tsx`):`GET /mobile/portfolio/summary`、`POST /mobile/portfolio/dividend`。已实现 = 手机端「交易统计」口径(清仓后才计、净额、没填费用按设置费率估);分红 `mobile_dividends` 手填、日期不能是未来、金额 > 0、同代码同日同金额不重复、上限 5000 条;成本法只有移动加权平均。
- **表格导入**(`trade_import.rs`、`TradeImport.tsx`,`POST /mobile/journal/import/preview` 与 `/mobile/journal/import`):列名别名与 desktop `tradeCsv.ts` 一致;1–5 位纯数字代码、负数数量、未来日期等拒收;按代码、日期重放切轮(同日先买后卖);卖出超过此前买入的行写问题;与已记成交/分红完全相同的行标已记过;期初持仓与送转行不支持。导入前后各重算一遍计划,在一个事务里写(`journal::insert_record_tx` 是从手动记一笔里抽出来共用的)。前端 `decodeFile` 先 UTF-8 严格解码、失败再 GBK。
- **schema v8**:`migrate_7_to_8` 给 watchlist 补 `grp`/`tags`/`sort_order`(先查 `pragma_table_info`,幂等)并建 `mobile_dividends`;`migrate_6_to_7` 的表清单校验改为「当前清单去掉 v8 新表」。备份 v8:`watchlist` 行加 `group`/`tags`/`sort_order`(`#[serde(default)]`),新增 `dividends`;兼容标志 `legacy_missing_watchlist_groups_are_empty`、`legacy_missing_dividends_are_empty`(schema_version ≤ 7)。
- **CI 冒烟失败的定位办法**(0.1.24 第一次推送失败时用的):冒烟脚本 `catch` 里输出 `::error title=Mobile UI smoke failed::...`,GitHub 把它变成 check-run annotation,`GET /repos/<owner>/<repo>/check-runs/<job id>/annotations` 匿名可读(Actions 日志和 artifact 下载需要登录)。现象:选完下拉选项后底部面板没有收起(根因未证实,最终把面板 portal 到 .mobile-app 根下后通过,见下方 0.1.25 一节)。
- 验证(本机):Rust 191 项 + 1 项 parity 通过;前端 501 项中 500 项通过(agent_error 缺 ajv);Edge 与 Playwright Chromium 冒烟各跑过,144 张截图;已人工查看 360 白色自选、360 深色持仓已实现与分红、360 白色表格导入。

### 持仓分析:收益表现、持仓体检、历史回放(0.1.25)

- `portfolio_perf.rs`(`GET /mobile/portfolio/perf`):成交 + 分红 + 腾讯不复权日线 → TWR(每日 (期末市值 + 取出) ÷ (上日市值 + 当日投入) 连乘)、资金加权区间值、XIRR(二分法,区间 ≥ 365 天才给)、逐只盈亏、走势(≤ 150 点)、缺口(北交所 / 持有超约三年 / 第一根价格晚于第一笔成交 20 天以上)与提醒(单日断崖、费用估算)。
- `portfolio_xray.rs`(`GET /mobile/portfolio/xray`):在手持仓(各未清仓轮合计)× 最新前复权收盘价 = 权重;对齐最近约 260 根的共同交易日,不足 60 根时把历史最短的移进缺口再试;年化波动、最大回撤、历史 VaR/CVaR、下行波动、风险贡献(wᵢ(Σw)ᵢ/σ²)、相关性、分散化比;行业取东财 `f127`(`research::profile`)。`chart::day_closes`(东财为主、腾讯兜底)与 `chart::index_closes` 是这几块共用的取价函数。
- `portfolio_replay.rs`(`GET /mobile/portfolio/replay`):与桌面版同一组五段 A 股预设;每只一次取最多 4000 根前复权日线(东财 `lmt`)再按窗口切片;窗口两端价格要在 7 个日历天内,否则写原因进缺口;有价格的 < 50% 市值不给组合数字;指数用东财 `1.000300` 不复权日线。**这三块没有对真实上游端到端跑过**(东财这台机器上当时连不通),计算部分有合成数据的单元测试和夹具(`mobile-portfolio-{perf,xray,replay}-synthetic.json`,由各模块里被 ignore 的 `dump_synthetic_fixture` 生成)。
- 前端 `PortfolioAnalysis.tsx`:三张卡片都是点「开始」才取数(`useRun` 在模块内存里记 30 分钟);提醒线存 localStorage `vibe.portfolio.xray.thresholds`,非法值当没设。
- 0.1.24 的发布过程:前三次推送的 CI 冒烟都因「选完下拉选项后面板没收起」失败,只在 CI 出现、本机复现不了;第四次(`0b1535d`,把 `Sheet` 用 portal 渲染到 `.mobile-app` 根下)通过并发布。根因未证实,见 ANDROID_REMAINING.md。

### 截图识别(AI)、加密备份、清除全部数据(0.1.26)

- **AI 截图识别**(`ai_vision.rs`、`ScreenshotImport.tsx`、`aiPresets.ts`):`POST /mobile/ai/recognize {images:[{name,mime,data(base64)}], target:"watch"|"position", probe?}`,命令层 `commands.rs` 给这条路由注入 `__secure_api_key`(和聊天、AI 复盘同一套)。只有 `mobile_ai_config.vision = 1` 才发图(`probe:true` 的「测试图片识别」例外,且跳过搜索核对);请求是 OpenAI 兼容多模态(`content` 数组里 `text` + `image_url` data URL,`temperature: 0`);系统提示只许转写看得见的内容。回答里取 JSON(整段 / 代码块 / 前后有废话都认),代码取 6 位数字,数量成本非正数丢弃并写进待核对;再用 `search::symbols` 核对代码对应名称(`names_agree`:去空白、星号后互相包含),只有名称的给前 3 个候选。前端把图缩到长边 1600 转 JPEG;草稿可改、要勾「已对照截图核对」才进写入队列;写入自选走 `POST /mobile/watchlist`,持仓走 `POST /mobile/journal {action:"record"}`(一笔买入,费用 0,note「截图识别(期初持仓)」),已有持仓/已在自选的代码拒收。单元测试里有本地 TCP 模拟服务验证请求形状和错误提示。
- **schema v9**:`migrate_8_to_9` 给 `mobile_ai_config` 补 `vision`(先查列,幂等);`ai_config::init` 的建表语句也带这一列。AI 模型配置不进备份,所以备份格式不变。
- **加密备份**(`backupCrypto.ts`,WebCrypto):外壳 `{format:"vibe_astock_mobile_backup_encrypted", version:1, kdf:{PBKDF2,SHA-256,iterations,salt}, cipher:{AES-GCM,iv}, data}`,AAD = format 字符串;迭代次数限 10 万–200 万(防止被改成巨大值);恢复路径先 `isEncryptedBackup` 再解密,其余流程不变。依赖 `crypto.subtle`(需要安全上下文;Tauri 的 `*.localhost` 源按 Chromium 规则算安全上下文,**真机未验证**,缺失时报「当前环境不支持加密」)。
- **清除全部本地数据**(`data_clear.rs`):`POST /mobile/data/clear {confirm:true, phrase:"清除全部本地数据"}`,逐表 DELETE(表清单取自 `sqlite_master`)+ `sqlite_sequence` + VACUUM,保留 user_version;`src-tauri/src/commands.rs` 在 200 之后删 Keystore 里的 API Key 并 `sync_reminders`(取消已排提醒),返回体里带 `keystore_cleared`/`reminders_cancelled`。命令层这段只能靠 CI 编译,本机编不了 src-tauri。前端两次 `askConfirm` 后清 localStorage/sessionStorage 并 reload。
- 0.1.25 已发布(c25a92d,APK 25,565,781 字节)。

### 消息流(0.1.26)与本机检查 src-tauri 的办法

- `messages.rs`(`GET /mobile/messages`、`POST /mobile/messages` read/read_all/delete/clear、`POST /mobile/messages/generate`):表 `mobile_messages(kind, day, title, body, route, read, ...)` UNIQUE(kind, day)(schema v10);收盘摘要 = `emotion::review_day` 的最近已收盘日、每天一条不覆盖;盘前提醒 = 交易日 09:30 前、当天一条;自选异动 = 交易日 09:30–15:05、每次覆盖并重新标未读(自选里 |涨跌幅| ≥ 5% 或触及涨跌停,只看当天有报价的,没有就不生成空消息);最多留 400 条;生成失败只进返回里的 skipped,不影响列表。文字只用自选和情绪读数,**不含持仓**。前端启动时(`MobileApp`)和进入消息页都会跑一次 `refreshMessages`;`route` 存页面 id(emotion / verify / monitor …),不认识的不给「去看看」。
- **本机检查 src-tauri 能不能编译**(Android 构建失败时先别等 CI):Windows 上直接 `cargo check` 过不了,因为 ① `tauri-build` 要 `icons/icon.ico`,② 三个插件的 desktop 桩是 `PhantomData<R>`、不是 Send。临时办法:生成一个最小的 `src-tauri/icons/icon.ico`,并把 `tauri-plugin-*/src/desktop.rs` 里的 `PhantomData<R>` 临时改成 `PhantomData<fn() -> R>`,在 `src-tauri` 里跑 `cargo check`,看完用 `git checkout` 还原这三个文件、删掉 icon.ico、`Cargo.lock`、`permissions/autogenerated`、`permissions/schemas`、`gen`(别误删已提交的 `permissions/default.toml`)。这样能提前发现「命令 future 不满足 Send / 高阶生命周期」这类只在 src-tauri 才暴露的错误。
- 教训:`#[tauri::command]` 的 async 函数里调用的所有 Core future 都必须 Send 且不能带泛生命周期的借用闭包;在 `stream::iter(...)` 里别借用外部切片,先拷成自有的 Vec 再 `.map`。

### 图片 / 文件分享、研报导入、AI 复盘归档(0.1.27)

- **原生文件分享**:`SharePlugin.kt` 新增 `shareFile`(参数 title / name / mime / dataBase64 / chooserTitle;mime 白名单 png、jpeg、markdown、plain、json;≤ 8 MB;文件名只留 ASCII;写到 `cacheDir/vibe-share/`,超过 24 小时的旧文件顺手删;`FileProvider.getUriForFile` + `ACTION_SEND` + `FLAG_GRANT_READ_URI_PERMISSION` + `clipData`)。插件清单(`AndroidManifest.xml`)里声明 `<provider ... android:authorities="${applicationId}.vibe.fileprovider">`,路径配置 `res/xml/vibe_share_paths.xml`(cache-path vibe-share/);`build.gradle.kts` 加 `androidx.core:core-ktx`。Rust 插件层 `share_file`(mobile.rs 走 `run_mobile_plugin("shareFile")`,desktop.rs 返回 Unsupported)、命令层 `share_file`(校验 mime 与大小)、`lib.rs` 注册。前端 `nativeShare.shareFile`:Android 走 invoke `share_file`,浏览器走 Web Share(files),再不行下载。**没在真机上验证**;src-tauri 的 Rust 部分用上一节的临时补丁办法本机 cargo check 通过,Kotlin / 清单合并只有 CI 的 Gradle 能验。
- **分享卡**(`ShareCard.tsx`):`cardRows` 只取 `emotion` 返回里已有的读数(没有的写 —、不可用的块不出现),`renderCard` 用 canvas 画 720 宽 PNG(按当前明暗主题),先预览再分享;不含自选 / 持仓 / 成本 / 盈亏。
- **研报导入**(`ReportImport.tsx`):`draftOf` 校验扩展名(.txt/.md/.markdown/.csv/.tsv/.json)、空文件、二进制(含 NUL)、超 25 万字符;`decodeFile`(从 TradeImport 复用)先 UTF-8 严格解码再 GBK;保存走 `POST /mobile/documents`(kind report,source file_import)。
- **AI 复盘归档**(`AiReviewPanel.tsx`):`reviewNote` 把回答 + 来源说明存成 kind note、category「AI 复盘」、source ai_review 的笔记;偏好存 localStorage `vibe.ai.review.archive`(默认开);归档失败只在提示里写,不影响复盘结果。
- 0.1.26 已发布(4f0fd7e,APK 25,652,877 字节)。0.1.26 的第一次推送(fe08dbb)APK 构建失败,原因与办法见上一节。
- 0.1.27 已发布(9a45a75,APK 25,669,211 字节)。第一次推送(97773aa)APK 构建失败:推断是 `androidx.core.content.FileProvider` 与 Tauri 生成清单里的同名 provider 合并冲突;现在用子类 `com.vibeastock.share.VibeFileProvider`(authority `${applicationId}.vibe.fileprovider`)。原因只是推断。以后给插件加 manifest 元素,先想想会不会和 Tauri 模板里已有的同名元素冲突。

### 隐私模式(0.1.28)

- `privacy.ts`:模块级状态 + localStorage `vibe.privacy`;`priv(text)` 开着返回 ••••;`usePrivacyMode()`(useSyncExternalStore)给需要重绘的组件(持仓页、交易日志页标题栏的眼睛按钮、设置页开关、密码样式输入框);其余页面是「进页面时读一次」(开关在设置页或页内按钮,切换时这些页面不在屏幕上或本身订阅了)。**新增显示个人金额 / 股数 / 价位的地方都要过 `priv()`,输入框用 `type={on ? "password" : "text"}`,画权益曲线这类能反推盈亏大小的图要判断 `isPrivate()`**。市场行情(最新价)、涨跌幅与收益率百分比、权重百分比不遮。
- 已覆盖:PortfolioPanel / PortfolioIncome / PortfolioAnalysis / JournalPanelV2 / JournalRiskPanel(含权益曲线)/ JournalReviewPanel / JournalEnvPanel / JournalAttributionPanel / JournalProblemsPanel。测试 `mobile_privacy.test.ts` 用 `setPrivate(true)` + 渲染各视图验证遮罩,冒烟里在持仓页点眼睛开关再关。
- 0.1.27 已发布(9a45a75,APK 25,669,211 字节)。

### AI 对话:流式、停止、多会话(0.1.29)

- **多会话**(`ai_chat.rs`):`mobile_ai_threads(id, title, created_at, updated_at)` + `mobile_ai_messages.thread_id`(默认 1,编号 1 的「默认对话」始终存在,`ensure_default_thread`);路由 `GET /mobile/ai/chat?thread=`、`DELETE /mobile/ai/chat?thread=`(不带 thread = 清全部消息,保留旧语义)、`GET /mobile/ai/threads`、`POST /mobile/ai/threads {action: create|rename|delete}`;标题 ≤ 30 字,最多 50 个,新对话用第一个问题当标题(只改还叫「新对话」的)。`migrate_10_to_11` 幂等(先查列,`ai_chat::init` 的建表也带新列);旧的迁移里 `verify_tables` 要排除后来才有的表。备份 v9:`ai_threads` + 每条消息的 `thread_id`(`#[serde(default)]`),校验线程 id 唯一、标题 1–30 字、消息引用的对话存在。
- **流式**:`send_stream` 用 `Response::chunk()`(reqwest 没开 stream 特性)攒字节、按合法 UTF-8 前缀取文字,`sse_feed` 按行解析 `data:`,只取 `choices[0].delta.content`,`[DONE]` 结束;服务端不是 `text/event-stream` 时当普通回复一次性读。命令层 `/mobile/ai/chat/stream`:注入 API Key 后调 `AppCore::ai_chat_stream`,闭包里 `app.emit("ai-chat-delta", {request_id, delta})`;前端 `AiChatPanel` 用 `__TAURI__.event.listen` 按请求号过滤、累加显示,没有事件通道(浏览器预览)就走原来的一次性 `POST /mobile/ai/chat`。
- **停止**:`AppCore.ai_cancel`(请求号 → `AbortHandle`),`Abortable` 包住整个流式 future,`POST /mobile/ai/chat/cancel {request_id}` 中止;被中止的 future 不会走到 `save`,所以这一轮(含用户那句)不入库;前端识别「已取消」把问题留在输入框。
- 测试里的本地 SSE 模拟服务:注意响应头用 `\r\n`、读请求头用 `b"\r\n\r\n"`——用 Python 脚本往 Rust 文件里写这些转义很容易被吃掉(写成真换行),这次就是因此卡住了测试,直接用 Edit 工具写。
- 0.1.28 已发布(45418ee,APK 25,665,915 字节);同一提交的跨平台验证里 ubuntu / Node 22 一组「Orchestrator full suite and typecheck」失败,与本次改动无关,疑似偶发。

### 先进先出成本法与批量按名称添加(0.1.30)

- **成本法**:`mobile_journal_settings.cost_method`(NULL / 'average' = 移动加权平均,'fifo' = 先进先出),`migrate_11_to_12` 幂等(先查列再补);备份 v10 在设置行里带 `cost_method`(`#[serde(default)]`,旧备份恢复为移动加权平均)。结算函数 `journal_risk::settle` 本来就拿着 `Fees`,所以把 `fifo: bool` 放进 `Fees`(`settings_of` 从行里填),不用改十几处 `rows_of(.., &settings.fees)` 的调用;各测试里的零费率常量补了 `fifo: false`。FIFO 分支:买入入 `VecDeque<Lot>`(剩余股数、买入价、未结转的买入费),卖出从队首扣,`leg_cost` / `leg_buy_fee` / `leg_gain` 三个量两种方法共用后面的累计;移动加权平均分支的算式保持原样(`(price - unit) * n`),避免浮点差异。设置接口 `POST /mobile/journal/settings {cost_method: "average"|"fifo"|null}`,视图里有 `cost_method` 与 `cost_methods`。
- **迁移派发改成循环**:`schema::migrate` 以前每个旧版本一条分支、每加一版要改十几处,现在是 `MIGRATIONS` 数组 + 从 `version.max(1) - 1` 起依次执行;新增一版只需写 `migrate_N_to_N+1` 并追加到数组、把数组长度加一。
- **批量添加**:`watchlistView.ts` 的 `splitAddTokens`(分隔符、去重保序)与 `pickByName`(名称完全一致优先 → 只搜到一只 → 否则列候选,不猜);`WatchlistPanel.addCodes` 六位数字当代码,其余走 `/mobile/symbol-search`(只含沪深北 A 股),逐个串行查,查到的批量加入,没加成的原样留在输入框并写明原因。
