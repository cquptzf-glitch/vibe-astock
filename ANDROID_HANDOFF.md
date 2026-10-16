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

验证（本机）：Rust 72 项 + 1 项 parity 连续 3 次全量通过；`emotion_history::tests::live_history_weekly_verify_probe` 对真实上游：30 天窗口首次 0.96 秒、14 天取到、16 天超出保留期；第二次 46 毫秒全部读缓存；2026-10-16 最高板 11 偏热、赚钱效应 −3.3% 与炸板率 44.5% 偏冷。前端 431/432（agent_error 缺 ajv），新增 `mobile_history.test.ts` 用真实返回裁剪的 `desktop/test/fixtures/mobile-history-2026-10-16.json`；本机 Edge 浏览器冒烟 42 张截图通过，已人工查看 360 白色情绪历史与 390 深色明日验证。CI 与发布结果在本节末尾补充。

已知限制：情绪历史从东财保留期起积累，前期 60 天窗口大部分为空；节假日工作日写条件时用上证快照时间判断今天开没开市，取不到时允许新增/修改但不许删除。

下一步建议：盯盘异动流与昨日梯队实时 → 竞价与盘中路径（可复用明日验证条件做盘中读数）→ 涨停样本 → 交易日志分组/风险与复查页签 → 每日复盘市场状态八格。