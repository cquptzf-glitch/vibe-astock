# Vibe AStock Android 单 APK 重构方案

> 目标：将当前以 React/Vite + Node.js Orchestrator + Python 数据/回测为核心的桌面项目，重构为 **单个 Android App（APK/AAB）**。App 不依赖自建服务端，不要求 PC 常驻，不在手机内运行 Node/Python 子进程；行情、资讯、AI 模型等均由 App 直接访问外部数据源/API，用户数据保存在手机本地。
>
> 推荐技术路线：**Tauri 2 + React/TypeScript + Rust Core + SQLite + Android Native Plugin（Kotlin）**。
>
> 文档状态：架构设计基线，可作为后续 Android 分支/里程碑拆解依据。

---

## 1. 重构目标

### 1.1 最终产品形态

最终安装包应满足：

- 一个 APK/AAB 安装后即可运行；
- 不依赖用户自己部署服务器；
- 不依赖电脑端 Node/Python 服务；
- 不启动 localhost 业务服务作为核心运行方式；
- 行情/资讯/公告等直接由 App 通过 HTTPS/WebSocket 获取；
- AI 通过用户配置的模型 API 直接调用；
- 自选、持仓、交易日志、笔记、缓存、报告等保存到手机本地；
- 支持 Android 后台任务、通知、分享、文件导入导出；
- 尽量复用现有 React/TypeScript 页面和业务模型；
- PC 版仍可保留，不要求为了移动端而破坏现有桌面能力。

目标结构：

```text
┌──────────────────────────────────────────────┐
│              Vibe AStock Android             │
│                                              │
│  React + TypeScript + Vite + ECharts         │
│                     │                        │
│                 App Bridge                   │
│                     │                        │
│                 Rust Core                    │
│   ┌────────┬────────┬────────┬──────────┐    │
│   │Market  │Research│Backtest│AI Client │    │
│   │Data    │Engine  │Engine  │          │    │
│   └────────┴────────┴────────┴──────────┘    │
│        │          │        │         │       │
│      SQLite     Cache    HTTPS    Stronghold │
│                                              │
│            Android Native Layer              │
│     Notification / WorkManager / Share       │
└──────────────────────────────────────────────┘
             │          │            │
             ▼          ▼            ▼
        行情/资讯 API   AI API       文件/系统能力
```

---

## 2. 当前项目架构判断

当前仓库大体分成四层：

```text
desktop/                 React/Vite 前端
orchestrator/            Node.js/TypeScript 编排器
.agents/.../scripts/     Python 数据抓取
backtest/                Python 回测
calc/                    Python 计算
```

前端技术栈已经非常适合复用：

- React 19
- TypeScript
- Vite
- React Router
- ECharts
- Zustand
- Tailwind

真正阻碍 Android 原生打包的不是 UI，而是后端运行模型：

```text
Node.js
  ├── node:fs
  ├── node:path
  ├── node:child_process
  ├── spawn/spawnSync
  └── 本地 HTTP Service
          │
          └── Python 子进程
                ├── requests
                ├── pandas
                ├── lxml
                ├── akshare
                ├── baostock
                └── mootdx
```

Android 正式版不应继续复制这种“桌面本地服务 + 子进程”的设计。

因此重构原则不是“把 Node/Python 塞进 APK”，而是：

> **保留 UI 与业务契约，替换运行时。**

---

## 3. 技术选型

## 3.1 推荐：Tauri 2

推荐采用：

```text
React/Vite
    │
Tauri IPC
    │
Rust Core
    │
Android Native Plugin
```

选择原因：

1. 可继续复用现有 React/Vite 前端；
2. Android/iOS 为 Tauri 2 正式支持平台；
3. Rust 适合实现数据抓取、计算、缓存、回测等本地核心；
4. 不需要内置完整 Node runtime；
5. 能直接生成 APK/AAB；
6. Rust Core 后续也能反向给 Windows/macOS 使用，最终实现多平台共核；
7. 官方已有 HTTP、SQL、Notification、Stronghold 等移动端可用插件。

建议以当前 Tauri 2.x 稳定版为基线，不锁死文档编写时的具体小版本号。

官方参考：

- https://v2.tauri.app/
- https://v2.tauri.app/plugin/
- https://v2.tauri.app/plugin/sql/
- https://v2.tauri.app/plugin/stronghold/

---

## 3.2 不推荐方案

### 方案 A：APK 内完整打包 Node + Python

不推荐作为正式架构。

问题：

- APK 体积显著增加；
- pandas/numpy/lxml Android ABI 维护复杂；
- child_process 体系与 Android 生命周期冲突；
- 后台执行受 Android 限制；
- 更新依赖复杂；
- 崩溃、日志、权限定位困难；
- Google Play 分发风险与兼容成本更高。

可以作为短期 PoC，但不应成为最终产品底座。

### 方案 B：React Native 全量重写

不推荐第一阶段采用。

原因：现有 React Web 页面、ECharts、样式体系都已经成熟，全量 React Native 重写会浪费现有资产。

### 方案 C：Kotlin Compose 全量重写

性能与 Android 原生体验最好，但迁移成本最高；除非未来完全放弃桌面共享 UI，否则当前不划算。

---

# 4. 新架构总览

建议把项目逐步整理为：

```text
vibe-astock/
├── apps/
│   ├── web/                     # 现 desktop/ 前端迁移后的共享 UI
│   ├── android/                 # Android/Tauri 外壳配置（可后续增加 ios）
│   └── desktop/                 # 可选：PC 壳
│
├── core/
│   ├── rust/                    # 新 Rust 核心
│   │   ├── src/
│   │   │   ├── api/
│   │   │   ├── market/
│   │   │   ├── datasource/
│   │   │   ├── storage/
│   │   │   ├── portfolio/
│   │   │   ├── journal/
│   │   │   ├── shortline/
│   │   │   ├── research/
│   │   │   ├── ai/
│   │   │   ├── calc/
│   │   │   ├── backtest/
│   │   │   ├── notification/
│   │   │   └── scheduler/
│   │   └── Cargo.toml
│   │
│   └── contracts/               # TS/Rust 共用接口定义/Schema
│
├── src-tauri/                   # Tauri 宿主
│   ├── src/
│   │   ├── lib.rs
│   │   └── commands.rs
│   ├── capabilities/
│   ├── gen/android/
│   └── tauri.conf.json
│
├── legacy/
│   ├── orchestrator/            # 原 Node 逻辑，迁移期间保留
│   ├── python-data/             # 原 Python 数据层
│   └── python-backtest/
│
├── desktop/                     # 第一阶段暂时保持，避免大规模移动目录
├── orchestrator/
├── backtest/
├── calc/
└── ...
```

## 4.1 实际实施时不要第一天就移动所有目录

第一阶段建议只新增：

```text
src-tauri/
mobile/
core-rust/
```

待 Android MVP 稳定以后再重命名/重组旧目录。

这样可以减少大规模 Git diff，便于逐模块迁移和回滚。

---

# 5. 最重要的设计原则：保持前端 Backend Contract 稳定

现有前端已经有：

```text
desktop/src/verticals/finance/lib/backend.ts
desktop/src/verticals/finance/lib/api.ts
```

这两层应该成为移动端迁移的边界。

当前：

```text
Page / Component
      │
      ▼
 backend.ts / api.ts
      │
      ▼
HTTP / localhost
      │
      ▼
Node Orchestrator
```

重构后：

```text
Page / Component
      │
      ▼
 backend.ts / api.ts
      │
      ▼
BackendAdapter
   ├── HttpBackendAdapter       # PC 旧模式
   └── TauriBackendAdapter      # Android
            │
            ▼
         invoke()
            │
            ▼
         Rust Core
```

建议新增：

```text
desktop/src/core/backend/
├── BackendAdapter.ts
├── httpBackend.ts
├── tauriBackend.ts
└── index.ts
```

接口示例：

```ts
export interface BackendAdapter {
  fetchEndpoint<T>(request: FetchRequest): Promise<T>;
  getView<T>(name: string, args?: unknown): Promise<T>;
  runResearch(request: ResearchRequest): Promise<ResearchRun>;
  runBacktest(request: BacktestRequest): Promise<BacktestResult>;
  chat(request: ChatRequest): Promise<ChatResponse>;
  saveSettings(settings: AppSettings): Promise<void>;
}
```

运行时选择：

```ts
export const backend = isTauri()
  ? new TauriBackendAdapter()
  : new HttpBackendAdapter();
```

页面不直接出现：

```ts
invoke(...)
fetch('http://127.0.0.1...')
```

所有平台差异必须收口到 Adapter 层。

---

# 6. Rust Core 模块设计

## 6.1 datasource

职责：

- HTTP 请求；
- Header/Cookie/User-Agent；
- 超时；
- 重试；
- 数据源降级；
- 限速；
- 原始数据标准化；
- source metadata；
- evidence envelope。

目录建议：

```text
core-rust/src/datasource/
├── mod.rs
├── client.rs
├── registry.rs
├── retry.rs
├── rate_limit.rs
├── normalize.rs
├── eastmoney.rs
├── tencent.rs
├── sina.rs
├── cninfo.rs
├── cls.rs
├── exchange.rs
└── yahoo.rs
```

统一接口：

```rust
#[async_trait]
pub trait DataSource {
    async fn fetch(&self, req: DataRequest) -> Result<DataEnvelope>;
}
```

统一返回：

```rust
pub struct DataEnvelope {
    pub status: FetchStatus,
    pub source: String,
    pub fetched_at: DateTime<Utc>,
    pub evidence: Vec<Evidence>,
    pub errors: Vec<SourceError>,
}
```

保持和现有前端 Envelope 语义兼容。

---

## 6.2 market

负责：

- 实时行情；
- K 线；
- 指数；
- 涨跌幅；
- 成交额；
- 快照；
- 自选股行情聚合。

建议避免页面直接调用各家数据源：

```text
UI → MarketService → DataSourceRegistry → EastMoney/Tencent/...
```

这样后续某一个公开接口失效时，无需修改 UI。

---

## 6.3 shortline

迁移现有短线能力：

- 竞价；
- 连板梯队；
- 涨停池；
- 炸板；
- 情绪；
- 板块轮动；
- 市场结构；
- 盘中异动；
- 日复盘；
- 周复盘。

建议拆分：

```text
shortline/
├── auction.rs
├── ladder.rs
├── limit_pool.rs
├── emotion.rs
├── rotation.rs
├── intraday.rs
├── review.rs
└── weekly.rs
```

原则：

> 数据获取和指标计算分离。

禁止：

```text
emotion.rs 内部直接请求某个 URL
```

应该：

```text
DataSource → MarketSnapshot → EmotionCalculator
```

方便离线测试。

---

# 7. Python 数据源迁移策略

不建议一次性重写全部数据源。

按价值分三层。

## P0：Android MVP 必须迁移

优先实现：

1. 腾讯行情；
2. 东方财富行情/K 线；
3. 东方财富板块/涨停/资金等核心 A 股数据；
4. 新浪作为备份源；
5. 巨潮公告；
6. 交易所公开接口；
7. RSS/公开新闻源。

这些覆盖：

- 首页；
- 自选；
- 持仓；
- K 线；
- 短线；
- 日复盘。

## P1：研究能力

迁移：

- 财务数据；
- 估值；
- 行业；
- 宏观；
- 招聘；
- 政策；
- 全球市场；
- SEC/HKEX。

## P2：低频/特殊源

包括：

- mootdx 特殊通道；
- AkShare 中无直接 HTTP 替代的接口；
- Baostock 特殊能力；
- 低频辅助源。

对于这类数据源，策略是：

1. 优先找原始 HTTP/API；
2. 找不到再评估 Rust crate/协议实现；
3. 仍不值得迁移则在 Android 版标记“不支持”；
4. 不为了 100% endpoint 数量把 Python runtime 塞回 APK。

---

# 8. Node Orchestrator 迁移策略

不是逐行翻译 Node 代码，而是按职责拆。

| 当前模块 | Android 去向 |
|---|---|
| service.ts | Tauri Commands + Rust Service |
| fsutil.ts | Rust std::fs / tokio::fs + SQLite |
| providers.ts | Rust AI Provider Registry |
| runtime_provider.ts | Rust AI Runtime |
| chat.ts | ai/chat.rs |
| finance/* | 对应 Rust finance/shortline/portfolio 模块 |
| notify.ts | Android Notification + scheduler |
| schedule.ts | WorkManager / Rust schedule config |
| report_library.ts | SQLite + app private files |
| ledger.ts | SQLite Repository |
| local_agent_runtime.ts | Android 移除/降级 |
| assistant_tools_mcp.ts | Android 默认移除 |
| run_tools_mcp.ts | Android 默认移除 |
| research_process.ts | Rust async task |
| child_process 相关 | 全部消除 |

核心目标：

```text
Node Orchestrator
       ↓
Rust Application Services
```

而不是：

```text
Node Orchestrator
       ↓
Android Node Runtime
```

---

# 9. AI 重构方案

## 9.1 Android 版保留

保留纯 API Provider：

- OpenAI-compatible；
- DeepSeek；
- Qwen；
- Kimi；
- GLM；
- MiMo；
- 自定义 Base URL；
- Self-hosted OpenAI-compatible endpoint。

统一模型配置：

```rust
pub struct LlmProfile {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_ref: String,
}
```

API Key 不进入普通配置文件。

---

## 9.2 Android 版默认移除

移动端不保留：

- Claude Code CLI；
- Codex CLI；
- WorkBuddy CLI；
- 任意依赖本机 shell/CLI 登录态的 Agent；
- PC 本地 MCP 子进程。

这些能力仍可存在桌面版。

前端设置页应根据平台隐藏：

```ts
if (platform === 'android') {
  hideLocalCliProviders();
}
```

---

## 9.3 密钥安全

禁止继续使用 localStorage 明文保存 API Key。

推荐：

```text
UI
 ↓
Tauri Command
 ↓
Stronghold / Android Keystore
```

普通设置（模型名、Base URL）可以 SQLite/Store 保存。

敏感项：

- API Key；
- Token；
- 私有数据源凭据；

必须进入安全存储。

---

# 10. 本地数据库设计

推荐 SQLite。

Tauri 官方 SQL 插件支持 Android SQLite；也可以由 Rust Core 直接使用 sqlx/rusqlite。

建议业务 Core 直接封装 Repository，UI 不直接写 SQL。

```text
StorageService
├── WatchlistRepository
├── PortfolioRepository
├── TradeRepository
├── JournalRepository
├── NoteRepository
├── ReportRepository
├── QuoteCacheRepository
├── SettingsRepository
└── TaskRepository
```

## 10.1 核心表

建议至少：

```text
settings
watchlists
watchlist_items
portfolios
positions
trades
journals
notes
reports
research_runs
ai_threads
ai_messages
quote_cache
kline_cache
shortline_snapshots
notifications
scheduled_jobs
schema_migrations
```

---

## 10.2 缓存分层

```text
Memory Cache
   ↓ miss
SQLite Cache
   ↓ stale/miss
Network
```

行情示例：

- 实时报价：5~15 秒 TTL；
- K 线：按周期决定 TTL；
- 财报：小时/天级 TTL；
- 公司 Profile：天级 TTL；
- 静态行业映射：版本级缓存。

所有结果保存：

```text
fetched_at
source
expires_at
schema_version
```

避免“旧数据看起来像实时数据”。

---

# 11. 回测重构

当前 Python Backtest 不建议直接打包 Android。

分两步。

## 第一阶段：Android Lite Backtest

只支持：

- 单标的/组合日线；
- MA；
- RSRS；
- 动量；
- 简单再平衡；
- 手续费/滑点；
- 基础指标。

Rust：

```text
backtest/
├── engine.rs
├── strategy.rs
├── broker.rs
├── portfolio.rs
├── cost.rs
├── metrics.rs
└── report.rs
```

输出：

- 收益曲线；
- 年化收益；
- 最大回撤；
- Sharpe；
- Sortino；
- 胜率；
- 盈亏比；
- 交易次数。

## 第二阶段

逐步迁移：

- Event Study；
- Scenario Replay；
- Portfolio XRay；
- Risk XRay；
- Benchmark；
- Sensitivity。

不要为了迁移完整 Python 功能阻塞 Android MVP。

---

# 12. Android 后台任务与通知

这是移动版应该强化的能力。

## 12.1 通知场景

支持用户定义：

```text
价格 > X
价格 < X
涨幅 > X%
跌幅 < -X%
成交额异常
自选股涨停/跌停
板块快速拉升
持仓大幅波动
竞价异常
重大公告
AI 复盘完成
```

## 12.2 后台实现

不要假设 Android App 能像 PC daemon 一样永久运行。

推荐：

- WorkManager：延迟/周期任务；
- AlarmManager：少量需要更精确时间的任务；
- Foreground Service：仅在用户主动开启“盘中实时监控”时使用；
- Notification Channel：行情、风险、研究任务分渠道。

需要接受 Android 的现实：

> “持续 1 秒级后台盯盘”不适合作为普通 Android 后台能力。

默认产品策略应是：

- 前台：高频刷新；
- 后台：低频/条件任务；
- 用户显式开启实时监控：Foreground Service。

---

# 13. 移动端 UI 重构

不要把 PC 侧边栏简单缩窄。

建议一级导航：

```text
首页 | 盯盘 | 复盘 | 研究 | 我的
```

## 首页

- 指数；
- 市场情绪；
- 自选摘要；
- 持仓摘要；
- 热点板块；
- 今日风险；
- 最近 AI 结论。

## 盯盘

Tab：

```text
自选
持仓
异动
短线
```

## 复盘

```text
今日
情绪
涨停
板块
周复盘
交易日志
```

## 研究

```text
个股研究
AI 对话
研报
回测
候选池
```

## 我的

```text
模型设置
数据源
通知
数据导入导出
隐私
关于
```

---

# 14. 前端响应式改造规范

建议建立 breakpoint：

```text
mobile: < 768
compact: 768~1024
desktop: > 1024
```

移动端规范：

- 主页面禁止固定宽度；
- 表格优先转 Card/List；
- 复杂表格支持横向滚动；
- Hover 行为必须有 Touch 替代；
- Tooltip 不能是唯一信息入口；
- 右键菜单必须替换；
- 小按钮最小可点击尺寸约 44dp；
- Modal 在手机优先 Bottom Sheet/Full Screen Dialog；
- AI Dock 改为独立全屏页面或底部抽屉。

ECharts 保留，但需要：

- `resize()`；
- touch zoom；
- tooltip touch；
- 减少同屏 series；
- 大量 K 线开启 sampling/dataZoom。

---

# 15. 文件导入导出

现有 Position/Trade CSV Import 保留。

Android 使用 Storage Access Framework：

```text
系统文件选择器
     ↓
Android/Tauri File API
     ↓
Rust Parser
```

支持：

- CSV；
- JSON；
- Markdown；
- 备份 ZIP。

导出：

```text
交易记录
持仓
研究报告
AI 报告
App 全量备份
```

不要让前端依赖真实文件系统绝对路径。

统一抽象：

```ts
export interface FileService {
  pickFile(types: string[]): Promise<FileRef | null>;
  exportFile(name: string, bytes: Uint8Array): Promise<void>;
  shareFile(file: FileRef): Promise<void>;
}
```

---

# 16. 平台能力抽象

新增：

```text
desktop/src/core/platform/
├── Platform.ts
├── BrowserPlatform.ts
├── TauriPlatform.ts
└── index.ts
```

统一：

```ts
interface PlatformService {
  kind: 'web' | 'desktop' | 'android';
  notify(...): Promise<void>;
  secureGet(key: string): Promise<string | null>;
  secureSet(key: string, value: string): Promise<void>;
  pickFile(...): Promise<FileRef | null>;
  share(...): Promise<void>;
  openUrl(...): Promise<void>;
}
```

禁止页面直接调用 Android/Kotlin API。

---

# 17. Tauri Command 设计

不要设计上百个极细 IPC command。

按领域暴露：

```text
market_quote
market_kline
market_snapshot
shortline_view
portfolio_get
portfolio_update
journal_query
journal_save
research_start
research_status
research_cancel
backtest_run
ai_chat
settings_get
settings_save
notification_update
```

Rust DTO 与 TypeScript DTO 应由 Schema 约束。

推荐：

```text
JSON Schema / TypeScript type / Rust serde
```

保持字段稳定。

---

# 18. 网络层

Rust 统一使用异步 HTTP Client。

建议：

- reqwest；
- rustls；
- serde；
- serde_json；
- tokio；
- url；
- chrono/time。

统一实现：

```text
超时
重试
限流
并发上限
Header
Cookie
代理策略
日志脱敏
TLS
响应体大小限制
```

禁止每个 DataSource 自己 `Client::new()`。

应共享连接池。

---

# 19. 数据源失败与降级

例：报价：

```text
Tencent
   ↓ fail
EastMoney
   ↓ fail
Sina
```

每次结果应包含：

```json
{
  "status": "ok|partial|failed",
  "primarySource": "tencent",
  "usedSources": ["tencent"],
  "fetchedAt": "...",
  "errors": []
}
```

这是当前项目非常值得保留的思想。

UI 显示：

```text
行情 14:31:05
来源：腾讯
```

而不是默默回退到旧缓存。

---

# 20. 安全设计

## 20.1 API Key

- 禁止 log；
- 禁止 analytics；
- 禁止出现在 URL query；
- 禁止 localStorage 明文保存；
- 禁止导出备份时默认包含。

## 20.2 WebView

- CSP；
- 关闭不需要的 remote navigation；
- 仅白名单外链交给系统浏览器；
- IPC command 使用 Tauri capabilities；
- 文件权限最小化。

## 20.3 网络

数据源和 AI Base URL 应分别控制。

自定义模型 Base URL 需要：

- 默认要求 HTTPS；
- 明确提示 HTTP 风险；
- 禁止静默访问 localhost/LAN（除非用户主动开启高级模式）。

---

# 21. 日志设计

统一 Rust tracing。

日志级别：

```text
ERROR
WARN
INFO
DEBUG
TRACE（开发）
```

严禁记录：

- API Key；
- Authorization Header；
- Cookie；
- 完整个人持仓（默认）；
- AI 私密 prompt（默认）。

提供：

```text
设置 → 诊断 → 导出诊断包
```

诊断包只包含：

- App Version；
- Android Version；
- Device ABI；
- 数据源健康状态；
- 脱敏日志；
- Schema Version。

---

# 22. 测试体系

迁移不能只靠真机点击。

## 22.1 Rust 单元测试

每个指标计算：

```text
input fixture → expected JSON
```

尽量复用现在 Python/Node tests 中的 fixture。

这是降低重写风险最重要的措施之一。

## 22.2 Contract Test

针对同一 fixture：

```text
旧 Node/Python 输出
          ==
新 Rust 输出
```

允许：

- 字段顺序不同；
- 可解释的浮点误差。

业务语义必须一致。

## 22.3 前端

保留当前 Node test，并新增移动布局测试。

## 22.4 Android

至少覆盖：

- 冷启动；
- 旋转/尺寸变化；
- 后台恢复；
- 网络切换；
- 飞行模式；
- API 失败；
- 数据源超时；
- 大量 K 线；
- 文件导入；
- 通知权限；
- Android 13+ Notification Permission。

---

# 23. 性能目标

建议 Android MVP 目标：

```text
冷启动：首页可操作 < 2.5s（中端设备）
热启动：< 1s
首页缓存渲染：< 300ms
普通 API：不阻塞 UI
K 线 1000~3000 bars：流畅缩放
内存：普通使用 < 250MB
APK：尽量 < 80MB（单 ABI 可更小）
```

不应把 Python runtime/numpy 打包后再讨论 APK 体积优化。

---

# 24. Android MVP 范围

第一版不要追求和 PC 功能 100% 对齐。

建议 MVP：

## 必须有

- 首页；
- 指数；
- 股票搜索；
- 个股详情；
- K 线；
- 自选；
- 持仓；
- 交易记录；
- 短线今日；
- 情绪；
- 涨停/连板；
- 板块轮动；
- 日复盘；
- AI 对话；
- AI 日复盘；
- 本地数据；
- 设置；
- API Key 安全保存；
- 通知。

## 第二批

- 深度研究；
- 候选池；
- 研报库；
- Lite Backtest；
- 周复盘；
- 分享卡片；
- 完整备份。

## 后续

- Event Study；
- Scenario Replay；
- Portfolio XRay；
- 复杂回测；
- 全量数据源；
- 高级 Agent workflow。

---

# 25. 详细开发阶段

## Phase 0：建立边界

目标：不改变业务功能，先让前端不依赖具体 HTTP 后端。

任务：

- 建立 `BackendAdapter`；
- 当前 HTTP 访问搬入 `HttpBackendAdapter`；
- 页面全部只调 adapter；
- 建立 `PlatformService`；
- 保证 PC 功能测试通过。

验收：

```text
PC 功能无回归
UI 不知道 localhost 地址
```

---

## Phase 1：Android Shell

新增 Tauri 2 Android 工程。

目标：

```text
现有 React 首页能够打包成 APK
```

任务：

- `tauri init`；
- Android init；
- applicationId；
- icon/splash；
- signing debug；
- WebView/CSP；
- deep link 预留；
- mobile CSS。

验收：

```text
APK 可安装
能启动
路由正常
ECharts 正常
横竖屏不崩溃
```

---

## Phase 2：Rust Data Core

迁移：

```text
quote
kline
symbol search
market index
```

并建立：

- HttpClient；
- DataSource trait；
- Registry；
- Cache；
- DTO；
- Error Model。

验收：

首页/股票详情不需要 Node/Python。

---

## Phase 3：本地数据

实现 SQLite：

- 自选；
- 持仓；
- 交易；
- Notes；
- Settings；
- AI Threads。

迁移 UI。

验收：

杀掉 App → 再启动数据仍存在。

---

## Phase 4：Shortline

迁移：

- 今日市场；
- 涨停池；
- 连板；
- Emotion；
- Rotation；
- Review。

尽量从 Python tests 导入 fixture 做 parity test。

---

## Phase 5：AI

实现：

- Secure Credential Store；
- OpenAI-compatible Client；
- Provider profiles；
- Streaming；
- Cancel；
- Retry；
- Timeout；
- Markdown rendering。

Android 隐藏 CLI Agent。

---

## Phase 6：Android 系统能力

实现：

- Notification；
- File Picker；
- Share；
- WorkManager；
- Foreground monitor；
- Import/export。

---

## Phase 7：Research

将研究工作流改造成 Rust async task：

```text
Created
Queued
Running
Completed
Failed
Cancelled
```

保存进 SQLite。

支持 App 被切后台后恢复任务状态。

---

## Phase 8：Backtest Lite

迁移基础回测。

---

## Phase 9：产品化

- Release signing；
- APK/AAB；
- ABI；
- ProGuard/R8（Android/Kotlin）；
- Crash logs；
- Privacy；
- Upgrade/migration；
- GitHub Actions。

---

# 26. CI/CD

新增：

```text
.github/workflows/android.yml
```

建议 jobs：

```text
frontend-test
rust-test
contract-test
android-debug-build
android-release-build
```

Release：

```text
Git Tag
   ↓
GitHub Actions
   ↓
Frontend Build
   ↓
Cargo/Tauri Android Build
   ↓
Sign
   ↓
APK + AAB Artifact
```

Release Key 放 GitHub Actions Secrets，不进入仓库。

---

# 27. 版本与数据迁移

每个 App 版本包含：

```text
app_version
schema_version
cache_schema_version
```

SQLite 使用 Migration。

禁止升级 App 后发现表结构不兼容再直接清数据库。

缓存可以丢弃，用户数据不能。

用户数据包括：

```text
自选
持仓
交易日志
笔记
研究报告
AI 对话（视隐私设置）
```

---

# 28. PC 与 Android 共存策略

建议最终发展成：

```text
                 Shared React UI
                      │
            Shared Backend Contract
                      │
               Shared Rust Core
                 /           \
        Desktop Shell       Android Shell
```

长期可以逐步让 PC 也从 Node/Python 迁移到 Rust Core。

这样 Android 重构不是“维护第二套系统”，而是在建立新的跨平台 Core。

Node/Python 在迁移期仍可以作为：

```text
Reference Implementation
```

用于 parity test。

待 Rust 覆盖率足够后再删除。

---

# 29. 现有代码迁移映射

## 直接保留为主

```text
desktop/src/core/components/
desktop/src/verticals/finance/components/
desktop/src/verticals/finance/pages/
desktop/src/verticals/finance/shortline/
desktop/src/verticals/finance/lib/*View.ts
desktop/src/verticals/finance/data/
```

需要做 mobile CSS 与交互改造。

## 重点改造

```text
desktop/src/verticals/finance/lib/backend.ts
desktop/src/verticals/finance/lib/api.ts
desktop/src/verticals/finance/lib/storage.ts
desktop/src/verticals/finance/lib/llm.ts
desktop/src/verticals/finance/lib/llmStore.ts
desktop/src/verticals/finance/lib/download.ts
desktop/src/verticals/finance/lib/notify.ts
```

## Rust 重写

主要来自：

```text
orchestrator/src/service.ts
orchestrator/src/finance/*
orchestrator/src/providers.ts
orchestrator/src/chat.ts
orchestrator/src/report_library.ts
orchestrator/src/ledger.ts
orchestrator/src/notify.ts
orchestrator/src/schedule.ts
.agents/skills/data-access/scripts/*
calc/*
backtest/*
```

## Android 不迁移

```text
Claude Code CLI
Codex CLI
WorkBuddy CLI
PC 本地 MCP 子进程
PC remote-access 服务
Node child_process runtime
Python runtime
```

---

# 30. API/错误模型

不要让 Rust Error 字符串直接显示在 UI。

统一：

```ts
interface AppError {
  code: string;
  message: string;
  retryable: boolean;
  source?: string;
}
```

例如：

```text
network_timeout
data_source_unavailable
bad_symbol
ai_key_rejected
ai_rate_limited
cache_corrupted
storage_failed
permission_denied
task_cancelled
```

Rust 内部详细 error chain 只进脱敏 log。

---

# 31. Research Task 架构

复杂研究不要通过一个超长 IPC 等待返回。

使用任务：

```text
UI
 ↓ research_start
Rust
 ↓
Task DB
 ↓
Async Worker
 ↓
Progress Event
 ↓
UI
```

事件：

```text
research://started
research://progress
research://artifact
research://completed
research://failed
```

切后台后 UI 可以通过：

```text
research_status(runId)
```

恢复。

---

# 32. 数据刷新策略

不要每个页面自己 `setInterval`。

建立统一：

```text
RefreshCoordinator
```

按数据 key 去重：

```text
quote:600519
index:000001
shortline:today
sector:AI
```

同一时刻多个组件需要同一个 quote 时，只发一次请求。

前台状态：

```text
active → normal refresh
background → stop/highly reduce
```

恢复 App 时：

```text
check stale → refresh
```

能显著降低手机耗电和数据源封禁风险。

---

# 33. 数据源限流

必须实现 per-host limiter。

示例：

```text
push2.eastmoney.com   4 req/s
qt.gtimg.cn           5 req/s
cninfo                1 req/s
```

真实数值在测试后调整，不在业务代码写死。

配置：

```rust
SourcePolicy {
    concurrency,
    qps,
    timeout,
    retries,
    backoff,
}
```

---

# 34. 推荐 Rust 依赖方向

参考，不要求立即全部使用：

```text
tauri
serde
serde_json
tokio
reqwest
rustls
thiserror
anyhow
tracing
chrono / time
url
sqlx 或 rusqlite
async-trait
futures
sha2
uuid
```

原则：

> Core 尽量不依赖 Tauri API。

应该：

```text
core-rust  ← 纯 Rust Domain/Application
src-tauri  ← Adapter
```

避免未来无法单元测试或无法用于桌面端。

---

# 35. 推荐首个目录骨架

第一轮代码可以直接新增：

```text
core-rust/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs
    ├── datasource/
    │   ├── mod.rs
    │   ├── client.rs
    │   ├── registry.rs
    │   └── tencent.rs
    ├── market/
    │   ├── mod.rs
    │   ├── quote.rs
    │   └── kline.rs
    ├── storage/
    │   ├── mod.rs
    │   └── sqlite.rs
    └── ai/
        ├── mod.rs
        └── openai_compatible.rs

src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── capabilities/
└── src/
    ├── lib.rs
    └── commands/
        ├── mod.rs
        ├── market.rs
        ├── storage.rs
        └── ai.rs
```

第一批 IPC 只做：

```text
app_info
market_quote
market_kline
symbol_search
watchlist_list
watchlist_add
watchlist_remove
```

先证明完整链路：

```text
React → Tauri → Rust → 数据源 → Rust → React
```

跑通后再扩展。

---

# 36. 第一个可运行里程碑

定义为：

> 手机安装 APK 后，可以搜索“贵州茅台/600519”，查看实时价格和 K 线，加入自选；退出 App 再进入，自选仍存在。

这个里程碑同时验证：

- Android build；
- React UI；
- Tauri IPC；
- Rust HTTP；
- DataSource；
- SQLite；
- ECharts；
- 生命周期。

如果这个闭环稳定，再开始迁移 Shortline/AI。

---

# 37. 第二个可运行里程碑

> Android 可以作为一个真正可日常使用的 A 股看盘 App。

范围：

- 首页；
- 自选；
- 个股；
- K 线；
- 持仓；
- 涨停池；
- 情绪；
- 板块；
- 日复盘；
- 通知。

此阶段暂时没有复杂 Research/Backtest 也可以发布内部测试。

---

# 38. 第三个里程碑

> AI 投研 App。

新增：

- AI Provider；
- 个股 AI 分析；
- 日复盘 AI；
- Report Library；
- Research Tasks；
- Backtest Lite。

---

# 39. 风险清单

## 高风险

### 1. 数据源反爬/接口变化

应对：多 Provider + Registry + Cache + Source Health。

### 2. Python → Rust 结果不一致

应对：Fixture + Contract/Parity Test。

### 3. 手机后台受限

应对：接受 Android 生命周期模型，不复制 PC daemon 思维。

### 4. UI 桌面化过重

应对：重做信息架构，而不是只加 CSS media query。

### 5. 迁移范围过大

应对：MVP 不要求 182 endpoint 全迁移。

## 中风险

- WebView/ECharts 性能；
- Android 文件权限；
- 网络证书兼容；
- 部分公开 API Referer/Cookie；
- ROM 后台限制；
- AI 长连接被系统打断。

---

# 40. 不应做的事情

重构过程中明确禁止：

1. 为了“快速兼容”把 Node runtime 当最终方案；
2. 为了 AkShare 强行塞整套 Python；
3. 页面直接写 Tauri invoke；
4. 页面直接依赖 Android API；
5. 每个 datasource 自己设计返回格式；
6. API Key 放 localStorage；
7. 一次性重写全部 182 endpoints；
8. 第一版追求 PC 所有功能 100% 对齐；
9. Android 后台无限循环高频请求；
10. 移动端继续照搬 PC 左侧菜单布局。

---

# 41. 推荐实施顺序（最终版）

```text
1. BackendAdapter
      ↓
2. Tauri Android Shell
      ↓
3. Rust HTTP/DataSource
      ↓
4. Quote + Kline + Search
      ↓
5. SQLite + Watchlist
      ↓
6. Portfolio + Trade
      ↓
7. Mobile UI
      ↓
8. Shortline
      ↓
9. Notifications
      ↓
10. AI API
      ↓
11. Research Task
      ↓
12. Backtest Lite
      ↓
13. Remaining Datasources
```

不能反过来先迁移全部 Python。

---

# 42. 最终推荐架构

```text
                         Vibe AStock
                              │
                  ┌───────────┴───────────┐
                  │                       │
             React Shared UI        Shared Contracts
                  │                       │
                  └───────────┬───────────┘
                              │
                       BackendAdapter
                    /                       \
            PC Legacy HTTP              Tauri IPC
                │                           │
        Node/Python Legacy               Rust Core
                                             │
                        ┌────────────────────┼────────────────────┐
                        │                    │                    │
                    Datasource            SQLite                AI
                        │                    │                    │
                 Public Market API      Local Data          Model APIs
                                             │
                                  Android Native Services
                                  Notification/WorkManager
```

迁移完成后的理想终态：

```text
                         Shared UI
                            │
                         Rust Core
                     /                  \
                Desktop                 Android
```

届时 Node/Python 可以逐步退出正式运行链路，只作为历史参考或离线开发工具保留。

---

# 43. 验收标准

Android V1 认为重构成功，需要同时满足：

### 安装

- APK 可直接安装；
- 首次启动无需安装 Node/Python；
- 无需用户启动服务端。

### 数据

- 主要行情直接从手机获取；
- 数据源故障有降级；
- 缓存有时间戳；
- 无“假实时”。

### 本地能力

- 自选/持仓/交易/日志持久化；
- 升级 App 不丢用户数据。

### AI

- API Provider 正常；
- Key 安全保存；
- 支持流式输出；
- CLI Agent 在 Android 不暴露。

### Android

- 前后台切换正常；
- 网络变化正常；
- 通知正常；
- 文件导入导出正常。

### 工程

- Rust 单测；
- 前端单测；
- Contract test；
- CI 自动生成 Debug APK；
- Release 可生成签名 APK/AAB。

---

# 44. 建议下一步立即执行的开发任务

建议第一个开发 PR 不做“大重构”，只完成下面 6 件事：

```text
1. 新增 BackendAdapter
2. 新增 Tauri 2 Android 壳
3. 新增 core-rust
4. Rust 实现腾讯 quote
5. Rust 实现东方财富 kline
6. SQLite 实现 watchlist
```

PR 验收 Demo：

```text
Android APK
   ↓
搜索股票
   ↓
查看价格
   ↓
查看 K 线
   ↓
加入自选
   ↓
重启 App
   ↓
自选仍存在
```

这是整个 Android 重构项目最关键的技术闭环。

完成这个闭环后，再开始大规模迁移短线、AI、研究和回测，风险最低。

---

## 结论

Vibe AStock 不应该通过“在 Android 里模拟一台 PC”来移植。

正确方向是：

> **保留 React/Vite UI 和现有业务 Contract，将 Node/Python 的本地服务/子进程架构逐步替换为 Rust Core，并用 Tauri 2 将它封装成真正的 Android 单 App。**

这条路线兼顾：

- 现有代码复用率；
- Android 安装体验；
- 性能；
- APK 体积；
- 安全；
- 后续维护；
- 桌面/移动共用核心的长期架构。

Android 不是旁支版本，而可以成为推动 Vibe AStock 从“桌面脚本式本地服务架构”升级为“跨平台应用核心架构”的契机。
