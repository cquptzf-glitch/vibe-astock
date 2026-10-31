# Android 当前状态与未完成清单

## 当前入口：0.1.48 并行预览、签名恢复与月营收图表（2026-10-31）

Actions 触发已恢复：仓库页面的 fork 工作流禁用开关是本次 422 的原因，重新启用后 GitHub 创建 [0.1.47 CI #41184117989](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/41184117989)。Rust、前端与完整手机界面检查通过；后续因原预览签名缓存为空，按原有身份保护规则停在签名检查。维护者确认没有原私钥备份，并明确选择新包名并行安装。

0.1.48 预览使用独立配置 `com.vibeastock.preview` / Vibe AStock Preview，新证书固定在 `docs/android/preview-signing-parallel.json`，私钥保存于 GitHub Actions 加密 Secret 和本机 ignored 的签名目录，不进入 Git、日志、截图或公开交付包。旧 `com.vibeastock.mobile` 及其公开签名记录保留；新包不覆盖旧数据，需从旧版导出业务备份后在新版导入。正式分发的默认包名不变。

补齐月营收 SVG 走势图与 PNG 分享，支持 12/24/全部区间，缺月断开、不补零；完整表格保留，图片附带来源、期间、滞后和资料边界。旧预览会随区间或元数据改变失效。本地前端 585 通过，四尺寸明暗主题 8 组图表与分享检查、47 路由 166 组页面状态检查通过；发行辅助脚本 12 通过，真实新密钥恢复/证书核对和新包 Android 工程生成通过。Core 严格 Clippy、Core/Tauri fmt 与 388 个计算对照散列检查通过。本机 Gradle 遇到 loopback 连接错误，Android 构建与原生验收以本版 CI 回执为准。

版本 0.1.48，配置预期 versionCode 1048，SQLite v13 / 业务备份 v11 不变。交易数据与模型处理保持原状。发布结果见 [0.1.48](docs/android/releases/0.1.48.md)，Actions 经过见 [故障记录](docs/android/actions-troubleshooting.md)。

## 仍需外部条件或补充覆盖，不能宣称全部验收

1. **设备与正式发行身份**：ADB 没有设备。真机后台/Doze、电量/流量停止、通知、权限、Keystore、分享、导入/备份与覆盖升级仍需设备。正式签名/AAB 流程已实现，但尚未提供正式渠道维护者升级密钥，也未提交商店；新并行预览已独立固定签名并使用加密 Secret 恢复，不覆盖原应用；App Links 配置已实现，尚无实际自有域名关联部署和系统验证。
2. **历史数据覆盖**：自动日状态源覆盖沪深，不覆盖北交所；重新上市、退市整理、2014 年前 IPO 等特殊制度仍需明确的历史区间/可靠外部数据，不能从今天名称推断。日线开盘成交仍是近似，不是交易所逐笔 feed；缺长历史/末端停牌退市数据时须披露。
3. **产业与研究覆盖**：资料库和受控研究流程已实现，自动硬数据源仍主要是 GPU 与四家公司月营收；其他行业事实需要可靠来源整理导入。用户数据不是程序独立核验的事实。真实模型长报告的业务质量未验收。旧 CLI 文件输入和 Python ID 不与移动 JSON 架构逐字/字节等价，不能据此删除 legacy oracle。
4. **核算边界**：已知送转可换算；未知送转、现金分红对不复权 MFE/MAE 的影响及日内极值先后仍不能精确恢复。分批买卖的换算路径不是逐日账户权益。
5. **连接与工程验收**：Kalshi/Polymarket 本机 HTTPS 仍有失败；概率扫描有预算。日历为深交所来源，其他交易所特殊安排缺独立源。Core 严格 Clippy 与 Core/Tauri fmt 本地通过；依赖审计、更多冻结样本和仪器测试仍未全部完成。0.1.39 已接入 K 线、分时和回测曲线真实 PNG；其他公开页仍为文字摘要卡。图表系统分享仍需真机验收。不能把测试签名、配置文件或单次取数成功当作所有功能最终验收。


<details>
<summary>历史候选、旧版阻塞与发布记录（以当前入口和 CI 回执为准）</summary>


### 0.1.47 发布尝试（2026-10-31）

功能提交 `d01da745652debe4a3241ce8bb85512e2d7a5cd3` 已推送并经 GitHub 分支接口核对。Android 工作流以 publish_preview=true 触发后再次返回 HTTP 422 `Actions has been disabled for this repository.`，没有本版 APK，也不能将新增 CI 检查写成已经在云端运行通过。本地交付目录 `.local/delivery/0.1.47/` 保存严格 lint、全量 Rust 测试、五组实际输出对照及版本回执；界面预览沿用 0.1.46。


## 当前入口：0.1.47 Rust 工程收尾（2026-10-31，本地候选）

Rust Core 严格 Clippy 的 69 项独立诊断已清理，未通过新增 lint 屏蔽放行。回测共享具名配置、报价证据点与研究阶段输入整理为明确结构，复杂类型增加别名，大枚举使用间接存储；交易复查拒绝非有限成本。Core、Tauri 和本地插件完成 rustfmt 统一。Android 预览与正式分发 CI 新增格式/严格 lint 检查并安装对应组件，YAML 本地解析通过；平台执行尚未通过。

本地 Core/Tauri 格式检查、Core 严格 Clippy（含测试）通过。Rust 单元测试 322 通过/30 ignored，证券代码契约 1 通过；9 个合成夹具生成测试通过。五组收益/体检/回放/风险/复查输出与 0.1.46 的实际测试程序输出一致（仅排除风险设置生成时间）；388 个冻结计算对照散列匹配。本轮未改前端，沿用 0.1.46 的 584 项前端与四尺寸明暗主题布局基线。详情及发布回执见 [0.1.47](docs/android/releases/0.1.47.md)。

这解决了此前的 Core lint 和项目 Rust 格式缺口，不代表真机、全依赖审计、全部历史数据或旧运行时的完整端到端等价已经验收。SQLite v13 / 业务备份 v11 不变。


### 0.1.46 发布尝试（2026-10-31）

功能提交 `ecc4e21f6f43ae8b53990f2cd92469b127d88878` 已推送并经 GitHub 分支接口确认。Android 工作流以 publish_preview=true 触发后，平台再次返回 HTTP 422 `Actions has been disabled for this repository.`，没有生成本版 APK。本地交付为 `.local/delivery/0.1.46/`，含四尺寸明暗主题的 24 张合成持仓截图、166 组全页检查和 8 组最终持仓布局检查的回执，ZIP SHA256 为 `2d94356ab0660e11cedc3a7a253655ec8ff0b56878523ab669de7105cee38e74`。配置预期 versionCode 1046 不代表已构建或安装。

本轮重新执行严格 Clippy（含测试）仍未通过，得到 69 项独立 lint 诊断；不能沿用旧记录声称工程检查已全部通过。功能回归与构建通过不等于全部迁移或真机验收完成。


## 当前入口：0.1.46 收益取价与复查完整性（2026-10-31，本地候选）

北交所持仓已接入东财不复权日线，参与时间加权、资金加权、XIRR 与逐只盈亏。首笔买入日缺价或行情含异常/重复行时整只列入缺口，避免零初始市值；持仓旧价结转显示日期和原因边界。已登记送转说明与现有核算一致，XIRR 无解说明已修正。沪深交易复查不再静默删除坏行；旧缓存若缺买卖日日线、顺序或价格异常则拒绝复用，主动取价时可重新获取。中间缺日和未知公司行动仍不能精确恢复。

本地 Rust 321 项通过/30 ignored、前端 584 项通过，桌面/移动构建和 388 个冻结计算对照散列检查通过；收益合成夹具与实际 Rust 输出一致；公开北交所日线单次探测通过。手机浏览器及本版发布结果见 [release 记录](docs/android/releases/0.1.46.md)。此前发布触发被 GitHub 拒绝；不能将本地候选当作已发布 APK。

下方“仍需外部条件或补充覆盖”为当前缺口，折叠的历史条目不应重复计为新功能未实现。本轮没有真机回执，未改变交易数据与模型处理。


### 0.1.45 发布尝试（2026-10-29）

代码提交 `0691a8ad05a42bf77bdf8cc53b230e020d22a7d5` 已推送。核对远端 android-mvp 指向该提交后，手动触发 Android 工作流并选择发布预览；GitHub 返回 HTTP 422 `Actions has been disabled for this repository.`，未生成本版 APK。仓库权限接口此前返回 enabled=true、工作流为 active；平台拒绝原因仍待解决。本机没有固定预览 keystore，不能生成替代签名覆盖发布。此前确认的最近 Android Release 仍为 0.1.38。

## 当前入口：0.1.45 北交所交易复查（2026-10-29，本地候选）

北交所已清仓股票可以通过东财不复权日线计算最大浮盈/浮亏，并沿用本机定稿缓存。严格核对代码、市场、买入/卖出日，遇到坏行或缺少成交日日线则逐笔提示失败，不把残缺结果当完整；持仓中间缺日、现金分红除息及日内高低点先后仍不能自动确认。北交所普通 30% 日跌幅不再直接当作疑似送转；未登记的大比例送转和历史特殊制度仍可能需要人工核对。沪深仍用腾讯不复权日线。

真实公开行情探测：东财 920575 的 2026-10-08 至 2026-10-16 不复权日线取得并通过边界验证（单次探测不代表长期稳定）；合成样本覆盖代码不符、坏行、缺失买卖日、北交所 30% 下跌、疑似送转和缓存。Rust 库 317 通过/29 ignored，前端 584 通过，桌面/移动构建通过；320 像素明暗主题 166 个页面/状态和原有交互回归通过。0.1.45 是本地候选，未构建 APK 或真机验收。

按维护者要求尝试发布 0.1.44：确认远端 `android-mvp` 为 0d127cf680f628fae218beba2dcac47328f36c5b 后，Android `workflow_dispatch` (publish_preview=true) 返回 HTTP 422，GitHub 提示 `Actions has been disabled for this repository.`；权限接口同时显示 enabled=true、workflow active，原因未确定。该提交没有 Actions run，本机也没有固定预览 keystore，不能以新密钥替代发行。2026-10-29 查询最新 Android Release 仍是 0.1.38。其他产业事实、历史制度、交易核算与真机验收缺口仍见下文。


## 当前入口：0.1.44 滚动回测分段平仓边界（2026-10-29，本地候选）

滚动样本外检验原先在每个分段末尾假设持仓可按收盘价平仓。现遇到末日停牌、当日买入受 T+1 限制，或启用涨跌停规则时收盘触及跌停，返回不可用原因及已完成分段，不生成全程累计收益；实际日线无法证明跌停时的成交能力，因此采用保守处理。普通可交易分段继续按原有费用和滑点平仓。其他历史制度、数据源覆盖及真机验收缺口仍见下文。

验证：Rust 全量库测试 316 通过、28 ignored；回测相关 35 通过、1 ignored；新增停牌、T+1、跌停与滚动分段端到端合成样本。0.1.43 日历相关 Rust 9 项、前端 584 项和桌面/移动构建通过，模拟浏览器检查四组通过。0.1.44 只改 Rust 回测与版本元数据，未重新运行未修改的前端全套。版本号 0.1.44（预期 Android versionCode 1044），本机无 APK 和真机回执。


## 当前入口：0.1.43 日历同步与日期修正（2026-10-29，本地候选）

日历公告与预约披露按每批最多 20 只自动连续处理全部自选；显示进度并保留各批来源缺口。自选名单在同步中改变时拒绝后续批次并提示重新同步，先前保存的事件保留；单股票同步仍可用。同源事件继续幂等更新，不能把源站限流当作完整数据。日历查询窗口、当天及逾期判断统一北京时间，跨 UTC 午夜、跨年不会晚一天。

验证：前端全量 584 项、日历相关 Rust 9 项、桌面和移动构建通过；320/390 像素明暗主题共 4 组模拟浏览器检查，43 只自选分三批、来源缺口与跨午夜日期通过。使用合成数据，未证明真实源站全量可用，也未进行 Android 真机验收。源码版本 0.1.43（预期 versionCode 1043），目前无本版 APK。完整研究、历史制度、核算、其他数据源及设备验收仍待后续完成；见下方最新剩余清单。


## 当前入口：0.1.42 月营收历史展示（2026-10-29，本地候选）

移动端产业信号新增四家公司历史月营收表：按月倒序，金额按亿新台币展示，缺月明确显示 —；环比与同比仅在前月、上年同月均有值时计算。来源、陈旧与错误提示保留，不推断 A 股公司业绩。数据接口和四家公司覆盖范围未扩充。源码版本 0.1.42，预期 versionCode 1042；尚无新版 APK。

本地验证：前端 582 项、定向 Rust 月营收 1 项、桌面和移动构建通过；Edge 320/390 × 明暗主题使用合成数据验证跨年、缺月和窄屏，4 组通过。当前仅完成此项迁移增量，不代表其他迁移或真机验收完成。完整剩余项仍见下文“仍需外部条件或补充覆盖”。


## 当前入口：0.1.41 全部移动页面内部布局（2026-10-26，本地候选）

本轮版本 0.1.41 / Android 配置 versionCode 1041。按维护者选择交付本地预览，继续等待 GitHub Actions 平台恢复，不重复请求设备或修改仓库权限。尚无 0.1.41 APK；最后有发布回执的 APK 仍为 0.1.38。

### 已完成与验证

- 全部 47 个路由统一手机边距、可换行筛选/操作、至少44px高的按钮和表单控件、局部滚动表格；默认页面及重点展开/编辑状态均进入预览。目录42项与路由47项口径不同，未删除兼容入口。
- 个股分为行情图表、公司资料、量价资金；首次访问才加载对应内容，已访问页面保留图表和筛选状态，支持键盘方向键切换。其他股票搜索改为展开入口。
- 候选池筛选折叠、指标完整换行；资料库优先检索与列表，全文使用页面滚动，编辑/导入/归档分层；日历与提醒折叠新增，提醒编辑自动展开；设置增加外观/模型/通知/备份跳转，清除数据移到末尾。
- 情绪、市场、风险和分红金额指标为手机放宽列宽；资讯/研究二级页签明确选中态；下拉/日期面板新增可见关闭按钮；AI可选工具折叠，回车换行，点击发送或Ctrl/Meta+Enter提交。
- 前端581项通过，桌面与独立移动构建通过。Edge四尺寸320/360/390/768 × 明暗主题，47路由/83状态共664组页面检查，另保留原有行情/日志/研究/回测/图表分享交互回归。自动断言无页面根部横向溢出、无脚本异常、无遗漏测试API；人工检查窄屏全部默认路由及代表性明暗/展开内容。逐页记录见 [布局验收表](docs/android/mobile-layout-audit-0.1.41.md)。

本地交付目录为 `.local/delivery/0.1.41/`，含可按页面/尺寸/主题筛选的全页画廊、ZIP、逐页验证JSON与哈希回执；源截图为公开冻结或合成样本。截图等待真实页面加载完成、停止滚动动画，不能以加载占位或错误响应充当功能验收。

SQLite v13 / 业务备份 v11 / 固定预览签名、交易核算及交易数据与模型处理不变。Rust、编排器和Python业务未改，本轮不把既有Rust/Python/Windows契约环境缺口计为通过。完整旧研究流程等价、历史特殊制度、核算边界、真实源站及Android键盘/返回/安装/原生分享真机验收仍见 [剩余清单](ANDROID_REMAINING.md)。


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


当前已发布预览版：**0.1.38**（2026-10-19），CI 构建、验证与发布全部成功。

使用者本轮明确：暂时没有 Android 设备，先完成 CI 和预览交付。真机验收保留为后续项；交易数据与模型的处理保持现状。

## 0.1.39 本轮候选

## 0.1.39 图表分享迁移（2026-10-25）

本轮候选版本 0.1.39 / versionCode 1039，本地完成、已推送；GitHub 平台拒绝启动 Actions，未发布。最后已发布版本仍为 0.1.38。

- 个股多周期 K 线、分时/五日分时、回测策略权益曲线支持生成真实 PNG：保留当前可见区间、指标、十字线、主题及来源，先预览再调用系统文件分享。回测曲线明确为历史假设，不是真实账户权益。
- 图片完整保留源站回退、异常数据、缓存失败提示及历史规则覆盖说明；说明超出图片容量时拒绝导出，不静默裁掉缺口。只取显式图表 SVG，不抓整个页面，不加载外部图像。
- K 线平移后无十字线时，信息和导出读数改用最后一根可见 K 线；数据、区间、指标或缓存提示变化后旧预览失效。
- 本地前端 577 项通过，类型与桌面/移动构建通过；Edge 三尺寸 × 明暗主题 216 张截图及 18 张实际 PNG 导出通过。SQLite v13 / 业务备份 v11 不变，交易数据与模型处理保持现状。

图表分享补齐以上三类；其他公开页仍用文字摘要卡。特殊历史制度、行业自动事实源/完整旧研究工作流等价、现金分红核算边界、源站连通/工程整理以及真机/正式身份仍未全部验收。用户已明确暂时没有设备，继续完成 CI 和预览交付。

### 本地验证（0.1.39）

前端 577 项通过，TypeScript 检查与桌面/独立移动构建通过；Edge 三尺寸 × 明暗主题 216 张截图、18 张实际 PNG 导出通过。核对 PNG 尺寸/签名、原生分享载荷与预览一致、K 线平移读数、指标切换失效、私有/嵌入内容拒绝；人工查看明暗图片。发行脚本 9 项通过。SQLite v13 / 业务备份 v11 保持不变。

改动前本机 Windows 全量编排器基线 1291 通过 / 128 失败 / 14 跳过，类型检查通过；系统 Python 3.14 缺 numpy/mypy，Python 基线未通过。它们不计为本轮验证通过；未改编排器和 Python 业务代码，Linux/macOS 全套及 Windows 契约以新 CI 回执为准。既有全量 Rust fmt / 严格 Clippy 未通过仍保留。

本机补充：Rust Core 312 项通过 / 28 ignored，符号 parity 另 1 项通过。Windows CI 同款契约子集 113 通过 / 5 失败 / 6 跳过，失败为 WorkBuddy 检测及目录联接/清理；未将其计为通过，也未为前端增量改动旧桌面环境路径。

### CI 发布阻塞（2026-10-25）

功能提交 `d35597ff50fc93bd6677294f484a63fc390835de` 已推送到 `android-mvp`。GitHub 未为 push 创建工作流；显式请求 Android workflow_dispatch 返回 HTTP 422：`Actions has been disabled for this repository.`。随后只读核实仓库 Actions 权限为 `enabled=true`、`allowed_actions=all`，工作流为 active；平台拒绝原因尚未确认，未擅自切换仓库权限。

因此 0.1.39 没有 CI 运行编号、APK 或公开 Release；最后已发布版本仍为 0.1.38，不能将本地 577 项/截图检查当作 APK 验收。已请求维护者在 GitHub Actions 页面处理平台限制，解除后对本提交启动 Android（publish_preview=true）与跨平台工作流，核对固定签名和内部版本 0.1.39 / 1039，再补发布回执。恢复构建前确认缓存签名密钥存在，缺失时拒绝换身份。

## 上一版本已实现

- **预览升级与包版本**：Gradle 显式使用受守卫保护的密钥，最终 APK 核对实际证书、包名、版本、ABI 和 debug 属性；缺失密钥或不匹配即拒绝发布。Tauri 与 Cargo 版本构建前一致，包内版本 0.1.38 / code 1038，修复旧包默认为 1.0 / 1 的问题。
- **历史制度取数与回测**：原生 Rust 接入 BaoStock 匿名 TCP，无 Python 常驻；查询逐日 ST/停牌和上市/退市日期，校验分页、代码、日期及字段。沪深主板 ST 按 2026-07-06 区分 5%/10%，创业板按改革时点区分，新制 IPO 前五日无限幅，旧制 2014 年后 IPO 首日 +44%/-36% 分开处理。参数 `historical_rules=auto|required|off`，覆盖、失败和日期缺口写进报告与界面；明确历史区间仍优先。
- **产业资料库**：20 赛道可记录规模、产能、产量、良率、份额、交付周期和价格；日期、单位、币种、公开出处和摘录必填。JSON 导入先预览，整批事务校验、去重；冲突值并列保留，不跨单位混算。支持筛选、分页显示、完整导出、移除和备份恢复。产业任务默认不附带资料库，用户明确开启才进入该赛道研究快照；来源标 user_curated，未独立核验。算力相关产业研究接入已有 GPU 温度计与供应链月营收。
- **送转 MFE/MAE**：已登记送转按事件日期把日线和买入股数换算到退出日股数口径，再与整轮平均买入成本比较。覆盖部分卖出、再买入、二次送转与反向拆股；不再一律排除。未知/无法可靠换算的疑似送转继续排除，现金分红除息与日线时序边界保留；这不是逐日账户收益。
- **后台官方月历**：启动前取得完整月历；源站缺失时不启动。休市日不采样，跨月或覆盖失效时停止并提示刷新；空/损坏订阅配置会停止，不进入无数据常驻状态。
- **发行工程**：正式签名 release APK/AAB 的手动构建、证书/包名/ABI/签名/校验和检查、维护者 artifact 已实现；HTTPS App Links 支持自有 host、原生路由白名单、assetlinks 生成和网站检查。未配置 host 时关闭。普通 CI 增加一次性测试密钥的 release APK/AAB 冒烟，不把测试身份当正式密钥。预览签名缓存与公开指纹固定；缺失密钥或最终包证书不符即停止发布。

## 验证与交付回执

0.1.38 已于 2026-10-19 发布：[Android 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.38)。实际功能构建提交：`b629dc5c18b4f7271122d990043bb8bf8300415b`。

- [Android CI #39150050100](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39150050100)：build-apk / publish-preview 均成功。ARM64 debug APK、原生 JUnit、一次性测试密钥 release APK/AAB 签名与版本检查通过；最终预览 APK 证书与缓存守卫/固定公开指纹一致。
- [跨平台 CI #39150049996](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39150049996)：六组全部成功。Linux / Node 24 日志核对：编排器 1429 项通过 / 4 跳过；前端 574 项；mypy 115 个源文件；发行脚本 9 项；388 例 calc oracle 散列匹配；Python 311 + 583 + 783 项通过。
- Rust：312 项通过、28 项 ignored，符号 parity 另 1 项通过；移动端截图 198 张通过。原生本机路由/时区 20 项断言的既有回执保留。
- APK：`vibe-astock-android-mvp-v0.1.38-arm64.apk`，30,700,727 bytes（29.28 MiB）；SHA256：`187757bc658a0b033abc11edf70c52aee646345742518015705e49d7e3899ef3`。
- Android 内部包名 `com.vibeastock.mobile`，versionName `0.1.38`，versionCode `1038`。发布后又从公开 APK 中独立提取二进制 manifest，用 aapt 核对相同字段及 debug 属性；不只检查文件名或 Cargo 版本。
- 最终 APK / 守卫证书 SHA256：`7C:2B:B8:8E:E4:E1:C6:20:29:93:5B:92:6F:F0:0F:2C:43:2C:5C:B7:76:AE:1E:8E:95:29:12:78:13:B2:35:8F`；公开固定值见 [preview-signing.json](docs/android/preview-signing.json)。缓存密钥缺失或证书不匹配即停止发布，不静默换身份。

0.1.37 及以前未建立同一可保证的预览签名身份，且 0.1.37 包内版本仍为默认 1.0 / 1；迁移前先导出业务备份。0.1.38 的证书与版本检查通过不代表真机覆盖升级验收。正式维护者密钥、商店、自有域名 App Links、真实设备仍未验收。用户本轮已明确暂时没有设备，先完成 CI 和预览交付；不再以设备连接阻塞本轮。

Core 严格 Clippy 与 Core/Tauri 格式检查已在 0.1.47 本地通过；GitHub CI 尚未执行成功。交易数据与模型处理保持原状；SQLite v13 / 业务备份 v11 不变。

## 仍需外部条件或补充覆盖，不能宣称全部验收

1. **设备与实际发行身份**：ADB 没有设备。真机后台/Doze、电量/流量停止、通知、权限、Keystore、分享、导入/备份与覆盖升级仍需设备。正式签名/AAB 流程已实现，但尚未提供维护者升级密钥，也未提交商店；App Links 配置已实现，尚无实际自有域名关联部署和系统验证。
2. **历史数据覆盖**：自动日状态源覆盖沪深，不覆盖北交所；重新上市、退市整理、2014 年前 IPO 等特殊制度仍需明确的历史区间/可靠外部数据，不能从今天名称推断。日线开盘成交仍是近似，不是交易所逐笔 feed；缺长历史/末端停牌退市数据时须披露。
3. **产业与研究覆盖**：资料库和受控研究流程已实现，自动硬数据源仍主要是 GPU 与四家公司月营收；其他行业事实需要可靠来源整理导入。用户数据不是程序独立核验的事实。真实模型长报告的业务质量未验收。旧 CLI 文件输入和 Python ID 不与移动 JSON 架构逐字/字节等价，不能据此删除 legacy oracle。
4. **核算边界**：已知送转可换算；未知送转、现金分红对不复权 MFE/MAE 的影响及日内极值先后仍不能精确恢复。分批买卖的换算路径不是逐日账户权益。
5. **连接与工程验收**：Kalshi/Polymarket 本机 HTTPS 仍有失败；概率扫描有预算。日历为深交所来源，其他交易所特殊安排缺独立源。Core 严格 Clippy 与 Core/Tauri fmt 本地通过；依赖审计、更多冻结样本和仪器测试仍未全部完成。0.1.39 已接入 K 线、分时和回测曲线真实 PNG；其他公开页仍为文字摘要卡。图表系统分享仍需真机验收。不能把测试签名、配置文件或单次取数成功当作所有功能最终验收。

<details>
<summary>历史记录与候选构建（当前缺口以上文为准）</summary>

# Android 当前状态与未完成清单

当前已发布预览版：**0.1.36**（2026-10-18），Android CI 构建与发布成功，回执见下文。0.1.35 及以前历史记录保留在文末。

本轮继续补齐功能，保留既有交易数据与模型处理。SQLite 仍为 v13、业务备份仍为 v11；计算记录复用内部文档存储，无破坏性迁移。

## 0.1.36 已实现

- **24 个确定性函数**：移植 calc 0.5.0 的估值、汇率、增长、PEG、消化年限、分位/一致预期分散度、单季/TTM/YTD、技术指标/筹码、财报反应/超预期。单位、缺失值、非有限数与不适用结果明确输出，不用零补齐。
- **证据绑定计算依赖链**：研究模型先提出有界 JSON 计划，业务输入只能引用本轮证据或已有计算；本机按依赖顺序运行，再把实际结果交回模型撰写。循环依赖、伪造字面值、错误引用和与实际结果不符的数字拒绝归档为成功结论，失败审计仍保留。
- **可追溯计算记录**：保存函数版本、绑定、实际输入、来源路径与完整输出；计算 ID 使用 mobile-calc-v1 内容散列。恢复后读取会重新计算并核对，损坏记录拒绝使用；随现有业务备份恢复，普通资料检索/清理不混入计算记录。
- **研究界面**：显示每阶段计算结果、不可计算原因与记录入口，可展开完整输入、来源和输出。任务取消后迟到的模型结果及计算不会入库；过去时点的新研究不能拿当前快照冒充。
- **受控聊天与取数**：新增计算目录与函数工具；研究增加近期 180 根日线及明确单位的财务序列。只有已有取数契约能确认单位的字段才补充单位，不能推断的字段说明缺口。

## 仍未完成，不能宣称全功能验收

1. **真实设备验收**：ADB 无设备，后台/Doze、通知、权限、Keystore、导入/分享和备份恢复仍缺真机验收。
2. **交易制度与历史数据**：历史 ST、上市初期、重新上市的自动时点数据仍未接入；已有明确日期区间规则覆盖。全市场为报价分页/分钟采样，未接交易所逐笔 feed；停牌退市、分红除权和送转轮次 MFE/MAE 边界保留。
3. **完整桌面研究等价**：24 个纯计算函数已有 Python 冻结对照；移动 JSON 依赖链已落地。旧 CLI 文件输入、全部桌面 DAG/取数适配及行业市场规模/产能/良率资料库仍非逐项等价；mobile-calc-v1 ID 不与 Python CLI ID 字节等价。财务口径缺失与资料不足仍标 incomplete，真实模型报告质量尚未验收。
4. **源站覆盖**：Kalshi/Polymarket 本机 HTTPS 连接仍有失败；事件概率扫描有预算边界；四家公司台系营收已跑通，单次连通不代表长期可用。完整开休市月历为深交所来源，上海/北京特殊安排缺独立源。
5. **正式发行**：正式签名/长期密钥、release/AAB、商店、升级兼容和 HTTPS Verified App Links 仍需相应密钥、域名与验收；当前 debug-signed ARM64 预览版。
6. **工程强化**：全项目 fmt/clippy/依赖检查、更多跨市场冻样本和仪器测试未完成。公开分享图仍是文字摘要；当前计算输入/节点数有显式预算，超限会拒绝，不裁切已完成报告充数。

## 本地验证

- Rust Core：300 项通过、27 项 ignored，另有符号 parity 1 项通过；包含备份恢复、审计损坏拒绝、取消竞态和真实计算结果数字核对。
- calc 0.5.0：388 个冻结 Python 对照用例覆盖全部 24 函数；CI 校验对应 Python 源文件散列，防止旧 fixture 掩盖源变化。计算值与输出结构对照通过，不宣称错误文案逐字一致。
- 前端：569 项通过，桌面与独立移动构建通过；Edge 三尺寸 × 明暗主题共 180 张截图通过，包括查看计算明细和来源。
- 真实公开数据：新浪财务序列与东财 180 根日线在本机跑通，单季/TTM/同比与技术指标实际执行成功；真实用户模型长报告和真机生命周期未验收。
- Android ARM64 Rust 命令层最终检查通过；CI APK、原生单元测试与签名校验通过。本机 APK 链接仍受 SafeNet 环境影响，APK 以 CI 为准。

## 发布回执

0.1.36 已发布：[Android 0.1.36 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.36)，标签固定在功能提交 `46fbc52e176fd48a857600984a302325e5928d6d`。[Android CI #39123017859](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39123017859) 的 APK 构建、原生 Gradle 单元测试、签名校验和发布全部成功。ARM64 APK 30,577,839 bytes（29.16 MiB），SHA256 `69385bf3372d0876573885f48e8b8fa067ed70e78bd5f6f82f7703dea499586b`。仍为 debug-signed 预览版，未做真机验收。

云端 Android 日志核对：388 例 oracle 散列校验、Rust 300 项 + 符号 parity 1 项、前端 569 项、Chromium 180 张截图均通过。本机 Edge 同样 180 张截图通过。首轮跨平台六组有五组成功，Linux / Node 24 的既有并发 fixture 测试失败；测试夹具修复提交 `19d15f3`（不改变 APK 功能）对应[跨平台 CI #39123695067](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39123695067) 的 Windows/macOS/Linux × Node 22/24 六组全部成功。Linux / Node 24 日志核对：原并发槽位测试通过、编排器 1429 项通过 / 4 跳过、前端 569 项通过、mypy 115 个源文件通过、Python 311 + 583 + 783 项通过。本节最终回执优先于前文候选/首轮失败记录；回执文档提交不改变已发布标签。


0.1.36 首轮跨平台 CI #39123017948 的 Linux / Node 24 并发槽位测试返回 `agent_probe_failed`，Linux / Node 22 已通过。检查发现测试专用 fixture-flags.json 会被重复原地写入，而多个 CLI 探针子进程同时读它，存在读到空/不完整 JSON 的竞态；具体子进程 stderr 未由该错误保留，不能声称日志已证明唯一原因。后续测试修复改为相同内容不重写、变更原子替换；不改产品登录探针与容量逻辑。本机定向 9 项通过、1 项平台跳过，修复后的跨平台 CI #39123695067 六组全部成功，Linux 并发分支已通过。Android APK 仍从功能提交 46fbc52 构建。

<details>
<summary>0.1.35 及以前的历史记录（当前缺口以上文为准）</summary>

# Android 当前状态与未完成清单

当前已发布预览版：**0.1.35**（2026-10-18）。Android CI 构建与发布成功，回执见下文。以下是当前代码状态；文末折叠内容是 0.1.34 及以前的历史快照。

本轮按维护者要求优先实现功能；保留既有交易数据与模型处理，不以调整其规则作为本轮前置事项。旧数据不清空，SQLite 升到 v13，业务备份升到 v11。

## 本轮已落地的功能

- **研究**：公司六阶段、多空辩论六阶段、情绪六阶段、市场周报、每日市场复盘、20 赛道产业六阶段。任务保存在 SQLite，可分阶段或连续执行、暂停后续阶段、继续、中止；完成及部分结果存成独立报告版本。阶段引用按本轮证据 ID 核对，来源数值/确定性算式/无来源三级核对，失败审计完整归档；取消后丢弃迟到结果。每阶段注入仓库研究宪法与对应 SOP 的打包快照。不能把当前财务/产业快照伪装为历史时点研究。
- **公司数据**：新浪完整利润表、资产负债表、现金流量表与同报告期同比；同花顺汇总的一致预期 EPS、机构数与范围；百度 PE TTM/PB 历史及仅正值样本的分位。不把逐篇研报 EPS 拼成一致预期。
- **回测**：自然语言工具目录、参数校验、真实运行与报告归档；参数敏感度热图；252 日训练/63 日检验的滚动样本外；确定种子的区块重采样夏普区间；成本重跑对照；沪深300价格基准；组合共享现金再平衡和加减仓；零量/日历缺失日不成交、结转估值；日期区间、明确的历史涨跌幅区间覆盖。单股/多股真实日线回测已在本机跑通，东财失败时使用真实腾讯 OHLC 备源。
- **盘中**：全市场 A 股分页快照扫描、自选批量扫描、涨跌幅/成交额/换手/分钟采样变化条件，字段缺失另计；分钟变化只使用工作日连续交易时段内的新鲜报价。Android 显式开启 Foreground Service，常驻通知、停止入口、采样间隔、流量上限、低电量停止线、异动分组与去重。
- **日历**：深交所官方完整开休市月历，财报预约与公告事件采集/去重、保留已完成状态，事件日期提醒与 Android 规则同步；提醒设置进业务备份。
- **聊天/资料库**：有预算与取消能力的受控公开数据/计算工具；可选资料库检索只返回命中片段，漏引或引用本轮未召回文档会拒绝保存。PDF（文本层）、Word、Excel 本机解析后预览导入；新导入 PDF 保留原始页序（包含空白页），检索片段不跨页，引用强制核对本轮实际召回页码，分页可随业务备份恢复；资料的日期/状态/代码检索与自动归档规则。
- **交易**：表格及手动期初持仓、送转拆股（新股数/原股数倍率），统一成本结算接入持仓/风险/收益/问题检查；不把送转当成现金流；本周我的交易在本机计算。含送转的轮次暂不算不复权 MFE/MAE，并单独计数。
- **产业与资讯**：GPU B200 Kalshi 月均阶梯报价/现货对照，排除过期合约月并区分报价类型/未取到；台系四家公司月营收、同比、环比、完整基数累计同比及同月环比差；公告雷达；Polymarket 分页及现查宏观标签、Kalshi 现查经济/金融系列的有界事件概率扫描。
- **全局/构建**：公开行情/研究页面悬浮问 AI、文字摘要分享图；统一公开 GET 缓存、失败历史快照标记、显式离线模式；独立移动入口与构建目录，移动包不再携带桌面 router/ECharts 包；`vibeastock://stock/...`、`calendar`、`research` 外部深链。

## 仍未完成，不能宣称全功能验收

1. **真实设备验收**：当前 ADB 没有设备。后台持续运行/Doze/低电量/流量停止、通知冷/热启动、拒绝权限、Activity 重建、Keystore、文件分享、备份恢复、WebView 导入与字体等尚未逐项真机验收。
2. **数据与核算边界**：全市场是报价分页/分钟采样，未接入交易所全市场逐笔 feed；历史 ST、上市初期、重新上市的自动时点识别仍缺完整源（已提供明确日期区间的规则覆盖，不能用今天 ST 名称套整个历史）；源站缺失长历史、末端停牌/退市与分红除权仍影响精度；送转轮次 MFE/MAE 暂排除。
3. **桌面完整研究等价**：移动研究使用受控数据快照、固定阶段和有限确定性算式，尚非桌面 Python 全量 calc/DAG/全部行业取数工作流的逐项等价移植。产业资讯是登记的 RSS 行业组，不是完整市场规模/产能/良率库；资料不够会标 incomplete。未用真实用户模型验收长报告业务质量。
4. **源站验证与覆盖**：Kalshi Rust HTTPS 在本机仍被对端断开；GPU 现货正常，远期失败会独立标出。事件概率是有预算的分页/定向扫描，不是全量；台系四家公司月营收已对真实 FinMind 接口跑通。单次源站可达不等于长期可用。上海/北京交易所独有的特殊交易安排尚无独立日历源；当前完整月历来源为深交所。
5. **发行工程**：正式签名、长期升级密钥、release/AAB、商店渠道与升级兼容验收仍需持有发布密钥的维护者；HTTPS Verified App Links 仍需域名与 assetlinks 配置。当前仅自定义 scheme，不称为已验证 HTTPS App Links。
6. **工程强化**：原生 Gradle 单元测试与 APK 构建已在 CI 通过，仪器/真机测试未完成；全项目 fmt/clippy/依赖检查及更多跨市场冻样本仍需补齐。公开页面分享图是最多 600 字的文字摘要，不是每页专门排版的完整图表卡。

## 本地验证记录

- Rust Core：289 项通过，26 项 ignored；符号 parity 1 项通过（0.1.35）。
- 前端：566 项通过；桌面与独立移动构建通过。移动主入口约 426 kB（未压缩），未包含桌面 router/ECharts。
- 浏览器：Edge 三尺寸 × 明暗主题，174 张截图通过，包含新研究创建、分阶段执行、继续并归档；这是合成数据界面验收，不是真实模型质量验收。
- Android Rust 命令层：ARM64 目标检查通过，最终 APK 也已在 CI 成功编译。原生服务源码用 Android 36 SDK 编译通过，外部深链与中国时区日期解析 15 项直接断言通过；移除 API 26 才支持的 java.time，以兼容 minSDK 24/25。
- 真实取数：财务三表、THS 一致预期、百度历史估值、深交所完整月历、GPU 现货、台系四家公司月营收、单/多股票回测跑通；Kalshi 与 Polymarket Rust HTTPS 在本机被远端重置，HTTP/1.1 复核相同，事件概率真实取数未通过。
- 本地 APK：链接器响应文件被本机 SafeNet 加密导致链接失败，不能写作 APK 构建成功。Gradle 原生测试又受 Google Maven HTTPS 下载失败影响，未完成。
- 旧桌面额外验证：orchestrator 类型检查通过；本地 Python 3.12 环境下 orchestrator 1360 通过 / 59 失败 / 14 跳过，Python 1639 通过 / 35 失败 / 3 错误（存在子进程控制台等环境问题，不能记作通过）。首轮 Linux CI 全套仅 1 个架构断言失败：新增 mobileMain.tsx 组装根尚未登记；已补齐双入口边界，定向 13 项通过；最终候选 69eb810 的跨平台六组全部通过；Linux 编排器 1429 项通过 / 4 跳过、前端 566 项通过、Python 311 + 583 + 783 项通过，mypy 115 个源文件通过。本机环境失败不代表这些云端检查失败。

## 发布回执

0.1.35 已发布：[Android 0.1.35 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.35)（标签固定在 `69eb81066a3e639b34f1431e934cc19bc6df131b`；[Android CI #39098061563](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061563) 的 APK 构建、原生 Gradle 单元测试、APK 签名校验与发布全部成功；[跨平台 #39098061530](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061530) 六组全部成功）。ARM64 APK 29,104,663 bytes（27.76 MiB），SHA256 `262c6b14f16b7b6b5bf58ebce27c344639cd9f7c393de93be993d2f6557cc1ff`。仍为 debug-signed 预览版，未做真机验收。

云端日志已核对：Rust 289 项 + 1 项 parity、前端 566 项、Chromium 174 张截图；Linux 编排器 1429 项通过 / 4 跳过、Python 311 + 583 + 783 项通过、mypy 115 个源文件通过。本机 Edge 同样 174 张截图通过。本节最终回执优先于前面的候选阶段“待 CI”记录；文档回执提交不改变已发布 APK 标签。

0.1.34 的既有发布回执保留在下面历史记录中。

<details>
<summary>0.1.34 及以前的历史记录（不是当前缺口判定依据）</summary>

# Android 未完成功能清单

当前版本：Android 0.1.34 预览版（本轮提交后由 Android CI 发布，CI 与 APK 结果以本段稍后补记为准，没补记就是还没核对）。0.1.34：**回测分析加深**（`position_pct` 仓位比例、`monthly_table` 月度收益对照买入持有、`drawdown_episodes` 最深 3 段回撤、`optimize_value` 前 70%/后 30% 样本内外对照、回测结果分享图）与**盘面事实、涨停全表两页「问 AI」**。本地通过：Rust 248 项 + 1 项 parity、前端 545 项（agent_error 缺 ajv）、tsc 与构建、Edge 冒烟 162 张截图（回测页不在冒烟里，只有 SSR 测试）。**没验证**：对真实行情跑回测（东财日线本机连不上）、真机上的分享图。

0.1.34 已发布：[Android 0.1.34 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.34)（标签固定在 98c2629dd77fbf7386b2caaeae0a8cc64f4b9e5f；Android CI [#39050516018](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39050516018) 的 build-apk 与 publish-preview 成功，跨平台验证 [#39050515968](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39050515968) 六组全部成功；ARM64 APK 25,889,675 bytes，SHA256 76704f759b5b40f60836322aca0be2e17e29361e75979365211f7d7dbf94ef1c，仍为 debug-signed 预览版；未做真机验收）。

上一个版本 0.1.33：**候选池深化**（`candidateView.ts` 纯计算：满足全部/任一、涨跌幅条件、排序、命名组合最多 10 组存浏览器本地；自选分组过滤；条件改动不重新取行情）、**更多页面「问 AI」**（连板天梯、行业题材、近 5 天、情绪历史；近 5 天的按钮是「生成市场周报」，上下文不含个人交易）、**连板天梯分享图**（`drawRowsCard` 通用画卡 + `ladderRows`）。本地通过：前端 542 项（agent_error 缺 ajv）、tsc 与构建、Edge 冒烟 162 张截图；Rust 没改动。**没验证**：真机上的分享图与真实模型的周报质量。

0.1.33 已发布：[Android 0.1.33 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.33)（标签固定在 24f3f37d26c0cdfbcf96ab6d4cd03fe7bc25f005；Android CI [#39048657907](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39048657907) 的 build-apk 与 publish-preview 成功，跨平台验证 [#39048657905](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39048657905) 六组全部成功；ARM64 APK 25,867,867 bytes，SHA256 bdc3fd1060d004a02ecef1a25799cc6743e9a44abe4fb3a3a622d0dc0ea51cc5，仍为 debug-signed 预览版；未做真机验收）。

上一个版本 0.1.32：**问 AI / 附带当前页面**（`aiPageContext.ts` 登记表 + 今日情绪/首板名单两页登记与「问 AI」按钮；个人页面 `PERSONAL_PAGES` 进入即清登记，源码测试守着个人页面文件不引用登记模块；后端 `page_context` 只作为本次请求的 system 消息、不入历史、`local_data_attached` 如实为 true）、**回测涨跌停模拟**（按板块幅度近似，ST/新股未识别）、**统一取数重试与并发上限**（`discovery::get_bytes`：2 次退避重试、8 并发，本地模拟服务测过）、**路由分包**（MobileApp 主包 461 → 183 kB）。本地通过：Rust 242 项 + 1 项 parity、前端 533 项（agent_error 缺 ajv）、tsc 与构建、Edge 冒烟 162 张截图（含新增的「问 AI」步骤）。**没验证**：真机上真实模型对附带页面的回答质量；统一重试对真实上游限流的效果。

0.1.32 已发布：[Android 0.1.32 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.32)（标签固定在 1246bffd305853afba36d37dec681ed4724256f2；Android CI [#39035500304](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39035500304) 的 build-apk 与 publish-preview 成功；**跨平台验证 [#39035500284](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39035500284) 里 ubuntu-latest / Node 22 一组在「Orchestrator full suite and typecheck」失败**，其余五组成功。这一步以前在 b172ed1、45418ee 也各失败过一次，每次都只有 ubuntu / Node 22 或 24 其中一组，本次改动没碰 orchestrator，看着像偶发，但**没有读到日志、没有证明**；ARM64 APK 25,857,403 bytes，SHA256 e9dc44a4d7204d0afb9b5984d714d998b3e71652bfe72aacbcb0b04e3852e91b，仍为 debug-signed 预览版；未做真机验收）。

上一个版本 0.1.31：**回测升级**——手续费/印花税/过户费/滑点、100 股整手、三种策略（均线交叉 / 通道突破 / RSI）、止损、最多 5 只的等权组合、固定小格参数实验、年化/夏普/成交明细；Rust 层拆成纯计算 `simulate` + `build_result`（有手算对账的测试，不碰网络）。**没验证**：对真实行情跑一遍（`live_backtest_probe` 是 ignored 联网测试，本机连东财日线被对端关闭 TLS，没跑通，K 线取数代码本次没动，原因没查）；没有涨跌停/停牌模拟。本地通过：Rust 236 项 + 1 项 parity、前端 529 项（agent_error 缺 ajv）、tsc 与构建。

0.1.31 已发布：[Android 0.1.31 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.31)（标签固定在 917b3f1b7626f804fb2c7002ea84d0e336f00f97；Android CI [#39031391262](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39031391262) 的 build-apk 与 publish-preview 成功，跨平台验证 [#39031391264](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39031391264) 六组全部成功；ARM64 APK 25,760,819 bytes，SHA256 adce88e62bff969f0b875d9d5cb4179c94d2b32d1aa8997558da5b87c0ca5d54，仍为 debug-signed 预览版；未做真机验收）。

上一个版本 0.1.30：**先进先出成本法**（设置里 `cost_method`，`journal_risk::settle` 按批次结转，买入费用跟着批次走；schema v12、备份 v10）与**自选批量按名称添加**（`splitAddTokens` / `pickByName`，名称不唯一不替用户猜）。本地通过：Rust 228 项 + 1 项 parity、前端 526 项（agent_error 缺 ajv）、前端构建通过。

0.1.30 已发布：[Android 0.1.30 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.30)（标签固定在 e8774c09da1afece7dec88e8a8ab22ac9738ee13；Android CI [#39029266108](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39029266108) 的 build-apk 与 publish-preview 成功，跨平台验证 [#39029266063](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39029266063) 六组全部成功；ARM64 APK 25,723,859 bytes，SHA256 a5d063a1790abdcb40f5ba5ba4fee286eb29b51e26f92ee42e0dd180172b7dbd，仍为 debug-signed 预览版；未做真机验收）。

上一发布版本：已发布 [Android 0.1.29 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.29)（标签固定在 4c089e09d9a5791dae31731a413f489d830ef502；Android CI [#39028351378](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39028351378) 的 build-apk 与 publish-preview 成功，跨平台验证 [#39028351359](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39028351359) 六组全部成功；ARM64 APK 25,709,051 bytes，SHA256 363cf6a6ce03e3ec17c4d489fc63e823cefa610e33379cc9114d718878e07ba8，仍为 debug-signed 预览版；未做真机验收）。0.1.29：**AI 对话**——流式输出（`ai_chat::send_stream` 读 SSE，`AppCore::ai_chat_stream` 登记请求号、命令层用 Tauri 事件 `ai-chat-delta` 推增量）、停止按钮（`POST /mobile/ai/chat/cancel` 中止 future，取消的这一轮不保存）、多会话（`mobile_ai_threads` + 消息表 `thread_id`，新建 / 重命名 / 删除 / 清空，默认对话不可删，最多 50 个）；schema v11、备份 v9。**没验证**：真实模型的 SSE 兼容性（各厂商的流式格式细节不同，只按 OpenAI 兼容的 `data: {choices:[{delta:{content}}]}` 解析，推理内容字段 `reasoning_content` 跳过）、真机上 Tauri 事件通道。本地通过：Rust 225 项 + 1 项 parity、前端 524 项（agent_error 缺 ajv）、Edge 冒烟 162 张截图、src-tauri 本机 cargo check 通过。

上一发布版本：已发布 [Android 0.1.28 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.28)（标签固定在 45418eeed7894e908f4d9378c01550e29c3cd380；Android CI [#39026918456](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39026918456) 的 build-apk 与 publish-preview 成功；**同一次提交的跨平台验证 [#39026918347](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39026918347) 里 ubuntu-latest / Node 22 一组在「Orchestrator full suite and typecheck」失败**（日志读不到，这一步与本次改动无关，之前 b172ed1 的 ubuntu / Node 24 也出现过一次，疑似 orchestrator 测试偶发失败，没有查）；ARM64 APK 25,665,915 bytes（24.48 MiB），SHA256 997f8268343d988cd876797e79cd79228b543cdd66afe2258c33051e225acb54，仍为 debug-signed 预览版；未做真机验收）。0.1.28：**隐私模式**（`privacy.ts`；金额与股数遮成 ••••、价位等输入框密码样式、权益曲线不画、持仓快照不能分享；覆盖持仓页、已实现与分红、收益表现、持仓体检、交易日志记录、风险与纪律、交易统计、入场环境、判断×执行、问题记录；市场行情与百分比不遮；设置页与持仓 / 交易日志页标题栏都能切）。**没覆盖**：日复盘页里的个人数字、盯盘页持仓行的浮盈亏金额（只显示百分比）、模式卡 / 复查页（只有百分比）、AI 页（不含个人数据）；系统截屏 / 录屏不受影响。本地通过：Rust 219 项 + 1 项 parity、前端 522 项（agent_error 缺 ajv）、Edge 冒烟 162 张截图。

上一发布版本：已发布 [Android 0.1.27 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.27)（标签固定在 9a45a755aa20d9856fdf4f99628112aafe3d05bc；Android CI [#39024591518](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39024591518) 的 build-apk 与 publish-preview、跨平台 [#39024591509](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39024591509) 六组全部成功；ARM64 APK 25,669,211 bytes（24.48 MiB），GitHub 记录的 SHA256 为 501d69b3458ecc377580a58ad9bf0183e6abd227a8a083b0788e981fb2b8cc5b，仍为 debug-signed 预览版；未做真机验收）。**0.1.27 第一次推送（97773aa）APK 构建失败、没有发布**：日志读不到，推断是分享用的 `androidx.core.content.FileProvider` 与 Tauri 生成的清单里同名 provider 在清单合并时冲突；改成自己的子类 `VibeFileProvider` 后 CI 通过（原因是推断，没有日志证实）。0.1.27：① **图片 / 文件分享**（Android 原生分享面板 + FileProvider：「今日情绪」页底部的图片复盘分享卡——只含市场读数，不含自选与持仓；研报 / 研究记录 / 收件箱每条的「分享」按钮按 .md 文件分享；`share_file` 命令、`SharePlugin.shareFile`、`ShareCard.tsx`、`nativeShare.shareFile`）；② **研报文件导入（先预览）**（文本类文件，UTF-8 或 GBK，≤ 2 MB，`ReportImport.tsx`；PDF/Word/Excel 不解析正文）；③ **AI 复盘自动归档**到研究记录（默认开，可关）。本地通过：Rust 219 项 + 1 项 parity、前端 519 项（agent_error 缺 ajv）、Edge 冒烟 162 张截图、src-tauri 用「临时补丁 + cargo check」的办法在本机编译通过（见 ANDROID_HANDOFF.md）。**没验证**：Kotlin 的 `shareFile`、FileProvider 的清单合并与分享面板在真机上能不能用——只有 CI 的 Gradle 编译能证明它能编过；图片分享卡的画布在真实 WebView 里的字体效果。

上一发布版本：已发布 [Android 0.1.26 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.26)（标签固定在 4f0fd7e7a08dd6944f6afe9ef971c88d74c67666；Android CI [#39022586152](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39022586152) 的 build-apk 与 publish-preview、跨平台 [#39022586239](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39022586239) 六组全部成功；ARM64 APK 25,652,877 bytes（24.46 MiB），GitHub 记录的 SHA256 为 e72a454b90f3e54c789389f6a755fefbf4b66c9097d432b47de417710506f4a6，仍为 debug-signed 预览版；未做真机验收）。0.1.26：① **自选 / 持仓「从截图识别」(AI 多模态)**——AI 模型设置里新增「常用厂商」选型（地址与默认模型来自仓库 `providers/*.json`）、「支持图片识别」开关与「测试图片识别」（用本机画的小图问模型，读出 600519 才算通过并自动打开开关）；截图交给当前模型读出代码/名称(持仓还有数量、成本)，再用腾讯股票搜索核对代码与名称（对不上的标出来，只有名称的给候选、不替你选），草稿逐条对照截图核对（可改）、勾选后才写入自选或记成一笔期初买入（`ai_vision.rs`、`ScreenshotImport.tsx`，路由 `POST /mobile/ai/recognize`；SQLite schema 升到 v9，`mobile_ai_config` 新增 `vision` 列）；② **可选加密备份**（口令 + PBKDF2-SHA256 31 万次派生 AES-256-GCM 密钥加密整份备份，`backupCrypto.ts`；恢复时识别加密备份并要口令，口令错或文件被改说同一句话）；③ **统一清除全部本地数据**（`data_clear.rs`：SQLite 全部表清空、表结构与版本号保留；命令层同时删 Android Keystore 里的 API Key 并取消已排的提醒；两次确认后清浏览器本机偏好并重新加载）。④ **消息**（「我的」→「消息」：盘前提醒、收盘摘要、自选异动，打开 App 或点刷新时按当天行情生成，**没有后台推送**，不含持仓与盈亏，`messages.rs`/`MessagesPanel.tsx`，schema v10 `mobile_messages`，导航角标）。**第一次推送 0.1.26（fe08dbb）时 Android APK 构建失败、没有发布**：原因是 `ai_vision::cross_check` 里带借用的闭包让 `mobile_api` 命令的 future 报「implementation of FnOnce is not general enough」（Core 单独编译和测试都通过，只有 src-tauri 的命令层要求 Send 才暴露）；已改成先把要查的内容拷成自有数据再并发，并在本机用「临时把三个插件的 desktop 桩改成 `PhantomData<fn() -> R>` + 临时 icon.ico 跑 `cargo check`」复现和验证通过（临时改动已还原，见 ANDROID_HANDOFF.md）。本地通过：Rust 219 项 + 1 项 parity、前端 513 项（agent_error 缺 ajv）、Edge 冒烟 162 张截图。**没做/没验证**：识别结果依赖用户自己配的多模态模型，本机没有真实模型可测（请求形状用本地模拟服务测过：OpenAI 兼容 `image_url` data URL、`Authorization` 头、400 时提示「可能不是多模态模型」）；Keystore 删除、提醒取消在命令层，Windows 上无法编译 src-tauri，只能靠 CI 编译，没在真机上跑过。

上一发布版本：已发布 [Android 0.1.25 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.25)（标签固定在 c25a92df725d275c09b3e0b2504663dbba309da3；Android CI [#39020578421](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39020578421) 的 build-apk 与 publish-preview、跨平台 [#39020578401](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39020578401) 六组全部成功；ARM64 APK 25,565,781 bytes（24.38 MiB），GitHub 记录的 SHA256 为 100b13da0d7f37c545d228205f75fdc3fa3ab83a59ee73cc3c90b1522136c86d，仍为 debug-signed 预览版；未做真机验收）。0.1.25 在「我的持仓」页补上三块（都只在点按钮时才联网取日线，结果记 30 分钟）：**收益表现**（时间加权、资金加权区间值、年化 XIRR、逐只盈亏，`portfolio_perf.rs`）、**持仓体检**（集中度、行业、最近一年波动/最大回撤/历史 VaR/下行波动、每只风险贡献、两两相关，用户自设提醒线，`portfolio_xray.rs`）、**历史回放**（按现在的比例回放 2015/2016/2018/2020/2021–2024 五段大跌期，并排沪深 300，`portfolio_replay.rs`）；前端 `PortfolioAnalysis.tsx`。口径差异见下方「我的股票」一条。

上一发布版本：已发布 [Android 0.1.24 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.24)（标签固定在 0b1535d06e5119f0c27a4403b33ad4a380e42158；Android CI [#39019542020](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39019542020) 的 build-apk 与 publish-preview、跨平台 [#39019542038](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39019542038) 六组全部成功；ARM64 APK 25,344,077 bytes（24.17 MiB），GitHub 记录的 SHA256 为 a4b98494ed2dd4fe2de559a085dbe3cd56eff4ea317fec8d37dbe54f0fa6d081，仍为 debug-signed 预览版；APK artifact #12096700991、截图 artifact #12095713400 保留 30 天；CI 内各步骤测试数未从日志核对；未做真机验收）。0.1.24 是界面改版：去掉顶部软件信息栏；底部菜单新增「设置」（六项），主题模式移到设置页；紧凑密度下页面边距、卡片、读数格、子页签、按钮、标题块整体收紧（复盘「今日情绪」页在 360px 宽下长度由约 2110px 降到约 1600px，其他页面同类收紧但未逐页量化）；系统控件换成网页自绘——下拉（底部选项列表）、日期（自绘月历）、复选框、确认/提示对话框（`controls.tsx`/`controls.css`，`WebSelect`/`DateField`/`askConfirm`/`showAlert`）。系统文件选择器（导入备份、导入表格）无法替代，仍是系统的。同一版还带上「我的股票」的一批功能：自选加市盈率/市净率/换手率、按任意列排序、分组/标签/自定义顺序（`storage.rs` 的 `set_meta`/`set_order`，`WatchlistPanel.tsx`）；持仓页「已实现与分红」（`portfolio.rs`，已实现只计清仓，分红手填、单独列）；交易日志「从表格导入成交与分红」（`trade_import.rs`，预览 → 一个事务导入 → 可撤销）。SQLite schema v8 / 备份 v8（新增自选分组/标签/顺序三列与 `mobile_dividends`，v1–v7 备份仍可恢复）。**0.1.24 前三次推送（6a5fe1e、840e4bf 等）的 Android CI 浏览器冒烟都失败、没有发布**：定位办法是让冒烟脚本失败时输出 GitHub 注解（匿名可读 check-run annotations）——看到的现象是「选完下拉选项后底部面板没有收起」，只在 CI（Linux Chromium）出现，本机 Edge 与 Playwright Chromium（含 8 倍降速）都复现不了，**根因没有证实**。最后的改法是把底部面板（`Sheet`）用 `createPortal` 渲染到 `.mobile-app` 根下、不再留在触发器所在的 `<label>` 里（怀疑是面板选项在事件冒泡中被卸载后 `<label>` 的默认动作把点击转发回触发器；用一个最小页面验证过「label + 同步卸载」在 Edge 里不会转发，所以这只是怀疑），之后 CI 通过。真机上若还有「选了又弹开」，先看这里。本地通过：前端类型检查与构建、501 项前端测试中 500 项通过（agent_error 缺 ajv）、Rust 191 项 + 1 项 parity、本机 Edge 与 Playwright Chromium 冒烟 6 组视口/主题 144 张截图。未做真机验证：自绘控件在真实 Android WebView 的触摸手感、安全区（状态栏）留白需要真机确认。

上一发布版本：已发布 [Android 0.1.23 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.23)（标签固定在 bc8590623dec28ff0f0dee6ce5d67592902316ae；Android CI [#39010312193](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39010312193) 的 build-apk 与 publish-preview、跨平台 [#39010312145](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39010312145) 六组全部成功；ARM64 APK 25,193,061 bytes（24.03 MiB），GitHub 记录的 SHA256 为 5fc6ecc7b99fca2a3036116cfe60073ff2f4e321822df22cb30838b1c2b04146，仍为 debug-signed 预览版；APK artifact #12092902282、截图 artifact #12092761246 保留 30 天；CI 内各步骤测试数未从日志核对；未做真机验收）。0.1.23 包含：交易日志分析页补全（模式卡、复查与 MFE/MAE、入场环境、判断×执行四象限、问题记录，0.1.22 之后合并）、复盘「题材事件树」（同花顺涨停原因，首板名单带原因）、市场面板「涨停分档」、「结构变化」页（涨停结构比较、上游字段清单、人工登记制度事件）、市场面板沪深 300 市盈率/股息率与股债利差（中证 xls 解析）、盘面事实「昨日炸板股修复」与反馈矩阵「昨日炸板」一行（前一交易日炸板池 + 腾讯实时行情，行情不属于复盘日时整块标不可用）。SQLite schema v7 / 备份 v7（模式卡与制度事件进备份；行情缓存表不进）。本地通过：Rust 179 项 + 1 项 parity（20 项 ignored 实时探测/夹具生成）、前端类型检查与构建、488 项前端测试中 487 项通过（agent_error 缺 ajv，CI 会装）、本机 Edge 浏览器冒烟 6 组视口/主题 126 张截图。按仓库主人 2026-10-17 的长期指示，本版及以后的 Android 预览版不再做发布前隐私扫描。

上一发布版本：已发布 [Android 0.1.22 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.22)（标签固定在 2d92a32190391a1901e28d8a54140ff5628c4d8e；Android CI [#39002403555](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39002403555) 的 build-apk 与 publish-preview、跨平台 [#39002403552](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39002403552) 六组全部成功；ARM64 APK 24,387,637 bytes（23.26 MiB），GitHub 记录的 SHA256 为 85eb0358ad3f276b1ffe1de6f2d367c710375d9f5887f669824300f4be03ab85，仍为 debug-signed 预览版；APK artifact #12089996843、截图 artifact #12090001310 保留 30 天；CI 内各步骤测试数未从日志核对）。发布前隐私扫描（工作树 / HEAD / 完整历史 / 文件名 / GitHub Issues·PR·Release 正文）相对基线 0727bb4 新增 0 命中，口径见 ANDROID_HANDOFF.md；**Secret Scanning 与 Push Protection 的开关匿名读不到，需仓库主人在设置里确认已开启（待办）**。真机验收仍未做。本轮补齐盯盘与复盘的几块大功能：盯盘「实时动态」（盘中异动流、昨日梯队实时表、持仓/自选、大票涨幅前十/跌幅前五、昨日成交额前十）、「竞价与盘中」（09:25 竞价强弱 + 6 个时点情绪路径 + 昨晚验证条件盘中读数）、复盘「涨停样本」、复盘「市场面板」（每日复盘的市场状态八格、主要指数中长线、成交额与两融、全球市场、行业资金、成交额前 20、龙虎榜）、交易日志「风险与纪律」（权益曲线与回撤、近 10/20/50 笔、纪律归因、规矩检查、在险资金、分组统计、费率/规矩/账户规模设置）。SQLite schema v6（新增 `mobile_watch_alerts`、`mobile_watch_turnover`、`mobile_intraday_snapshots`、`mobile_intraday_status`、`mobile_samples_days`、`mobile_journal_settings`；只有 `mobile_journal_settings` 进备份，备份格式升到 v6，v1–v5 仍可恢复，旧备份恢复后设置回到默认）。本地通过：Rust 125 项 + 1 项 parity（8 项 ignored 实时探测中本轮 5 个已对真实上游跑通）、前端类型检查与构建、457 项前端测试中 456 项通过（agent_error 缺 ajv，CI 会装）、本机 Edge 浏览器冒烟 6 组视口/主题 78 张截图。

0.1.22 之后的改动（交易日志分析页补全，**已合并到 `android-mvp`，随 0.1.23 发布**）：在「风险与纪律」之后补齐桌面版交易日志剩下的五块分析页，全部在复盘栏目的子页签里——**模式卡**（版本化）、**复查**（复查清单 + 最大浮盈浮亏 MFE/MAE）、**入场环境**（按入场日情绪强中弱与当时板位分组）、**看错还是没执行好**（判断 × 执行四象限）、**问题记录**（算不进统计的记录逐笔改正）。口径照 `orchestrator/src/finance/journal/{modes,excursion,inbox,environment,attribution}.ts`，差异见下方「交易日志」一条。**SQLite schema 升到 v7、备份格式升到 v7**（**已发布的 0.1.22 是 schema v6 / 备份 v6**，v7 是在其上新增的升级迁移 `migrate_6_to_7`，已装 0.1.22 的设备打开新版会自动升级；v1–v6 备份仍可恢复，旧备份恢复后没有模式卡）：v7 新增 `mobile_journal_modes`（模式卡，**进备份**）、`mobile_journal_bars`（逐笔日线缓存，公开行情，不进备份）、`mobile_journal_env_days`（逐日涨停/炸板池成员缓存，公开行情，不进备份）。本地通过：Rust 159 项 + 1 项 parity（15 项 ignored 实时探测/夹具生成；`journal_excursion::tests::live_tencent_probe` 与 `journal_env::tests::live_environment_probe` 已对真实上游跑通）、前端类型检查与构建、477 项前端测试中 476 项通过（agent_error 缺 ajv）、本机 Edge 浏览器冒烟 6 组视口/主题 108 张截图。前端夹具全部是合成数据；新页面都不登记 AI 页面上下文（测试守着）。合并后 Android CI 与 APK 构建见下方（以实际结果为准）。**这次合并 + push 是仓库主人明确指示「不要扫描」的，所以没有为这批提交重做发布前隐私扫描**；发布这批改动之前仍应补一次（扫描方法见 ANDROID_HANDOFF.md 0.1.22 一节）。真机验收未做。

上一发布版本：已发布 [Android 0.1.21 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.21)（个股 K 线与盘口按常用证券 App 重做：分时/五日/日周月 K/60·30·15·5 分钟、前复权/不复权、MA/BOLL、VOL/MACD/KDJ/RSI、拖动/缩放/十字线、20 项盘口与五档）。本地通过：Rust 78 项 + 1 项 parity（3 项 ignored 实时探测已对真实上游跑通）、前端类型检查与构建、435/436 项前端测试（agent_error 缺 ajv）、本机 Edge 浏览器冒烟 6 组视口/主题 48 张截图（含 K 线点按十字线、拖动平移、副图切换、分时切换）。代码 66a8496 的 Android CI #38989460626 与跨平台 #38989460648 全部成功；ARM64 APK 23,655,181 bytes（22.56 MiB），SHA256 ea473744eb1a6cce842bf0710138db5f1646f774ec8acaada531d0728c555dd1，仍为 debug-signed 预览版；真机验收未完成。发布前隐私扫描候选 65 / HEAD 65 / 历史 71 既有命中，新增 0。

上一发布版本：已发布 [Android 0.1.20 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.20)（复盘新增近 5 天、情绪历史、明日验证；SQLite schema v5 / 备份 v5）。本地通过：Rust 72 项单元/接口 + 1 项 parity（连续 3 次全量通过；2 项 ignored 实时探测对真实上游跑通）、前端类型检查与构建、431/432 项前端测试（未通过的是本机缺 orchestrator 依赖 ajv 的 agent_error）、本机 Edge 浏览器冒烟 6 组视口/主题 42 张截图。代码 2d49f00 的 Android CI #38983459066 与跨平台 #38983459069 全部成功；ARM64 APK 23,597,797 bytes（22.50 MiB），SHA256 c06e1f4676e1ad04bea137dc2fc734a6bd04019335adcb68abfa2d7eaceab460，仍为 debug-signed 预览版；真机验收未完成。发布前隐私扫描候选 65 / HEAD 65 / 历史 71 既有命中，新增 0。

上一发布版本：已发布 [Android 0.1.19 预览版](https://github.com/cquptzf-glitch/vibe-astock/releases/tag/android-mvp-v0.1.19)（五大栏目导航 + 情绪复盘「今日情绪」整套迁移）。代码 47bbc61 的 Android CI #38975425413（build-apk、publish-preview）与跨平台 #38975425430 全部成功；ARM64 APK 23,334,749 bytes（22.25 MiB），SHA256 6282f1b84eb693fc3bbf02abfa81d305b9f26a619969cbcda5247f71991050ea，仍为 debug-signed 预览版；真机验收未完成。发布前隐私扫描：候选 65 / HEAD 65 / 完整历史 71 个既有命中（测试假 key、公开查询参数），新增 0；历史里的 `.env.example` 只有占位值。本地通过：Rust 64 项单元/接口 + 1 项 parity（另有 1 项需手动运行的实时探测，已对 2026-10-16 真实上游跑通）、前端类型检查与构建、427/428 项前端测试（未通过的 1 项是本机未装 orchestrator 依赖 ajv，CI 会安装）、用本机 Edge 跑的 6 组视口/主题浏览器冒烟与 30 张冻结样本截图；CI 同一冒烟用 chromium 通过（截图 artifact #12078317923）。CI 逐项测试数未从日志核对（公开 API 读不到日志）。

每轮开发结束必须同步本文、ANDROID_HANDOFF.md 与 PARITY_MATRIX.md，并在回复中确认未完成清单、实际构建结果及发布版本；没有通过的检查不得写成通过。本文以当前代码为准，交接文档早期章节是历史快照。

## 尚未实现或需要较大移植的产品功能

- **产业信号**：Kalshi B200 月均远期概率、现货/远期对照、更多产业温度计。当前只有 GPU 现货与历史。
- **盘中异动与信号筛选**：全市场逐笔/分钟急拉急跌、批量条件扫描、自选信号聚合。0.1.22 的异动流只盯「持仓 + 自选 + 昨日梯队 + 昨日成交额前十 + 500 亿以上大票」，且只在 App 前台开着盯盘页/竞价页时才取行情与判断（切后台即停）；不等同全市场异动。量价观察仅覆盖单个股票日线。
- **估值与研究**：历史 PE/PB、历史分位、完整财务三表/同比、真正一致预期、证据链研究工作流。公司资料、利润表摘要、公告、逐篇研报 EPS 已有基础实现。
- **AI**：（流式输出、停止、多会话已在 0.1.29 完成；「附带当前页面」与「问 AI」已在 0.1.32–0.1.33 覆盖今日情绪、首板名单、连板天梯、行业题材、近 5 天、情绪历史、盘面事实、涨停全表八页，每日复盘、板块轮动、个股详情等其它公开页面还没登记）确定性工具调用、Deep Research Lite、多角度辩论及报告归档。当前基础聊天与复盘分析已存在；不得把个人成交/持仓/金额发送给模型。
- **完整回测**：（手续费/印花税/滑点、整手、三种策略、止损、等权多标的、固定小格参数实验已在 0.1.31 完成）仍缺：（涨跌停已在 0.1.32 按板块近似模拟，ST/新股未识别；仓位比例、月度收益表、回撤区间、一段样本内/外对照已在 0.1.34 完成）停牌模拟、多标的再平衡与加减仓、自动寻优与严格的滚动样本外检验、桌面研究回测工作流里剩下的部分（Python 引擎的基准指数对照、夏普置信区间、参数敏感度热力图等）、自然语言回测入口。
- **候选池与日历深化**：（候选条件组合、自选分组、排序、命名组合已在 0.1.33 完成）完整交易所日历、财报/公告事件自动采集及事件提醒联动。当前基础候选和本地事件 CRUD 已存在。
- **交易与复盘深化**：（先进先出成本法已在 0.1.30 完成：设置里二选一，影响已实现盈亏、逐日盈亏与剩余成本）AI 周报（0.1.33 先做成近 5 天页的「问 AI:生成市场周报」，只含公开读数，不含「本周我的交易」；带个人交易的周报没做）、图片复盘分享卡（今日情绪与连板天梯已有）。（成交导入预览已在 0.1.24 完成：表格导入，期初持仓/送转行不支持；情绪周期与明日验证战绩已在 0.1.19/0.1.20 完成；费率/规矩/账户规模设置在 0.1.22 完成。）
- **自选与内容库深化**：（自选分组/标签/自定义排序已在 0.1.24 完成）笔记/报告/收件箱的自动归档、丰富检索与桌面工作流对齐。当前本地文档 CRUD/备份已存在。
- **分享与备份**：（图片复盘分享卡、报告文件分享已在 0.1.27 完成，仅覆盖「今日情绪」读数图与文档的 .md 文件；连板天梯（0.1.33）和回测结果（0.1.34）也有分享图，其它页面的分享图没做；原生分享未在真机验证。可选加密备份、统一清除全部本地数据已在 0.1.26 完成。）
- **持续后台盯盘**：显式开启的 Foreground Service、常驻通知、流量/电量控制。现有 WorkManager 提醒不等同持续后台行情服务。

## 对照桌面版仍缺的页面与交互（0.1.19 逐页清点，0.1.22 更新）

- **盯盘 · 实时动态（0.1.22 已迁移，差异）**：异动、梯队、大票、成交额前十都有。差异：① 只在前台开着页面时取行情和判断，没有常驻后台服务；② 「昨日成交额前十」桌面版用通达信盘后包（任意交易日），手机端改为收盘后从新浪成交额榜记下一份存本机（不含北交所），**没在收盘后开过盯盘的那天就没有这份名单**，页面会写明原因，不拿别的日子顶替；③ 设置里选「仅手动刷新」时异动基本判不出（需要连续采样至少 1 分钟）。
- **盯盘 · 竞价与盘中路径（0.1.22 已迁移，差异）**：时点快照只在 App 前台开着盯盘页/竞价页时每分钟检查一次并存，过了窗口补不回来；没有桌面版的后台定时任务，所以没开 App 的时点会是「没存到」。
- **复盘 · 情绪复盘其余页签**：AI 复盘（六阶段+数字三级核对+版本，汇总里的条件可一键加进明日验证）、AI 周报（含本机计算的「本周我的交易」）。结构变化已在 0.1.23 完成（差异：东财池子只留约一个月，市场结构比较要本机缓存够窗口天数才可用，不够时写明天数与原因；制度事件只收人工登记）。涨停样本已在 0.1.22 完成；近 5 天、情绪历史、明日验证已在 0.1.20 完成；题材轮动按东财行业而非同花顺涨停原因。情绪历史与涨停样本只能从东财保留期（约 28 天）起积累，前期样本偏少。
- **复盘 · 今日情绪口径差异**：题材事件树（同花顺涨停原因，状态徽标）与首板名单的涨停原因已在 0.1.23 完成（差异：题材串靠同花顺涨停揭秘，覆盖不到的票不显示原因；题材归因来自源站编辑；「题材轮动」页仍按东财行业）；「昨日炸板股修复」与反馈矩阵「昨日炸板」一行在 0.1.23 做成盘面事实页里的独立卡片（前一交易日炸板池 + 腾讯实时行情，只在行情快照就是复盘日时可算，回看更早日子写明不可用；桌面版的计算源码没在仓库里找到，口径按桌面版界面字段「当天均 / 翻红」定义，晋级涨停/跌停按涨跌停价判断，**没有和桌面版逐项对账**，所以卡片没有并进上面的反馈矩阵，而是单独一行）；首板名单的「问 AI」未做（依赖页面上下文登记，归入「AI」一项）。
- **复盘 · 每日复盘（0.1.22 市场面板，差异）**：市场状态八格、主要指数中长线、成交额与两融走势、国债收益率、全球市场、行业资金、成交额前 20、龙虎榜已有。0.1.23 补上沪深 300 市盈率/股息率（中证官网 xls，用 calamine 解析，取不到写原因）与股债利差，涨停分档表也已在市场面板里。仍缺：② 北证 50 以外的指数走腾讯日线，北证 50 只走东财；③ 行业资金用新浪口径（东财列表接口成片取不到），和桌面版东财「主力净额」不是一个口径；④ 涨跌分布与三所涨跌家数只给「当天此刻」，周末/盘前取不到时显示没取到；⑤ Agent 复盘未做。
- **交易日志（0.1.22 发布了「风险与纪律」；其余分析页随 0.1.23 发布）**：已有权益曲线与回撤、近 10/20/50 笔状态漂移、纪律归因、自定规矩与违规检查、在险资金、按打法/是否按计划/持有周期分组、费率/规矩/账户规模设置。之后（0.1.22 发布后合并进 `android-mvp`，尚未发布）又补上：模式卡、复查清单与 MFE/MAE、按入场环境分组、判断×执行四象限、问题记录，**桌面版交易日志的分析页至此都有对应页面**。下面是这些页面与桌面版的差异：① 手机端的一笔交易是一条带成交明细的记录，不像桌面版从流水里切；**只有清仓后才算已实现盈亏**，所以模式卡版本统计、四象限、MFE/MAE 只算卖光了的（和补记），没卖完的交易只出现在在险资金与复查清单（「还在手上没写止损」）。② **交易日志只算 A 股股票**：新页面和「风险与纪律」「模式卡」一律排除 ETF/基金/债券/可转债/回购（`journal_risk::is_journal_symbol`，同桌面版 `isJournalSymbol`），它们仍计入持仓；「问题记录」页把它们列出来、不算错误。（0.1.22 之前「风险与纪律」不过滤，本轮顺带统一。）③ **MFE/MAE**：只在用户点「取行情并计算」时联网（腾讯不复权日线，北交所取不到不算），取到的按「代码+买入日+卖出日」永久存本机，卖出当天收盘前取的那份不算定稿、收盘后再点会重取一次；复查清单只读缓存、一次网络都不发。手机端**没有送转流水**，没法像桌面版那样排除「期间有送转」的整轮，改成看日线：持仓期间任意一天收盘比前一天低 21% 以上整轮标「疑似送转」不算（也可能是新股上市初期）——**只能识别大比例送转**，小比例送转与现金分红除息仍会让低点偏低，页面说明里写明。④ **入场环境**：每日情绪读数复用情绪历史缓存（`mobile_shortline_days`），涨停/炸板池成员另存 `mobile_journal_env_days`；东财池只留约一个月，更早又没缓存过的日子列在「没取到的部分」，不补、不当成「非涨停」；入场日北京时间 15:30 前不分组。⑤ **四象限**的「判对没判对」来自「明日验证」的对账（事后写的、暂定的不计），只覆盖最近约 60 个交易日，需要先写条件并对完账；最近交易日用交易日历（上证指数日线）取，不再单独请求 K 线。⑥ **模式卡**ID 是数字、文字上限按「字」数（800/60/300 字），超长拒收不截断；卡片文字只在本机、进备份、不进 AI。⑦ **问题记录**与桌面版的问题清单不是一一对应（手机端没有「不参与计算的流水 / 挂不上的标注」）：改成找卖超或卖在买之前、成交日期无效、没有成交的空记录、没填盈亏的补记，并提供逐笔改成交 / 删成交 / 删记录 / 补盈亏（改完不能更糟、至少留一笔成交、入场日跟着最早成交走）。仍缺：标注（打法/计划/止损）的单独冲突检测（手机端标注就在记录上，不会挂不上）、AI 周报里的「本周我的交易」。口径差异：桌面版按卖出就算已实现，手机端与「交易统计」一致，**清仓后**才计入。隐私模式未做（见「全局」），个人交易数据不得进 AI，本页不登记页面上下文。
- **我的股票**：自选 PE/PB/换手列与排序（0.1.24 已完成）、持仓已实现/分红（0.1.24 已完成，差异：已实现只计清仓，没卖完的交易已卖出的部分不算，分红手填、不自动抓取、不改成本；成本法可选移动加权平均或先进先出（0.1.30）；没有送转/期初持仓流水）、CSV 导入（0.1.24 已完成）。0.1.25 补上收益表现、持仓体检、历史回放（口径对照 `backtest/portfolio_perf.py` / `portfolio_xray.py` / `scenario_replay.py`，差异：只做人民币 A 股与 ETF，没有港美股；价格用腾讯不复权日线（收益）与东财 / 腾讯前复权日线（体检、回放），**手机端没有送转拆股流水**，所以收益表现遇到价格单日断崖只能提醒、不能修正；收益表现一只持有超过约三年、北交所的取不到日线会进缺口；持仓体检行业取东财所属行业而不是申万一级，体检窗口最近约一年；历史回放没有港美股时期，东财取不到时腾讯只给最近约三年，更早的时期写取不到；这三块没有对真实上游跑过端到端，只用合成价格序列测过计算，需要用户在真机上点一次确认能取到数）。截图识别（AI 多模态）已在 0.1.26 完成：自选读代码和名称，持仓读代码、名称、数量、成本，按「建仓日期 + 截图成本 + 数量」记成一笔买入（费用记 0，已有持仓的代码不让叠加），需要先在 AI 设置里选一个多模态模型并打开「支持图片识别」；不支持表格文件（表格走交易日志的「从表格导入」）。自选批量按名称添加已在 0.1.30 完成（代码和名称混输；名称不唯一时列候选、不替用户猜）。
- **研究**：产业研究 20 个赛道详情页、资讯雷达的 A 股公告/事件概率页签、多空辩论、深度研究六阶段、自然语言回测。
- **我的**：提醒页的盘前提醒/收盘摘要/自选变化消息流、研报上传预览、研究记录自动归档 AI 复盘。
- **全局**：（隐私模式已在 0.1.28 完成，覆盖范围见上）各页悬浮 AI 助手（页面上下文登记 0.1.32 先做了今日情绪与首板名单两页，没有悬浮入口，入口是页面上的「问 AI」按钮）。

## 已有功能仍未完成的工程验收

- 统一供应商缓存、限流、幂等 GET 重试与离线读取；不能把公共接口单次可达视为持续可用。
- 腾讯/东财实时报价的冻结样本和完整 fallback 覆盖，SH/SZ/BJ、停牌/盘前/空值等跨市场回归；本轮新增的东财 K 线样本只覆盖一个沪市股票。
- 模拟器/真机自动化与生命周期验证：Android Back/手势、Activity 重建、通知冷/热启动、权限拒绝、Doze、Keystore、备份/恢复/分享、弱网和外链返回。
- 正式签名与长期升级密钥、release profile / AAB、渠道发布与升级兼容性验证。当前 GitHub 预览版为 CI debug-signed ARM64 APK；不同 CI 的默认调试签名可能不同，不能承诺覆盖安装。更换安装前应先导出业务备份。
- 独立移动构建产物与更细的路由分包；当前入口已懒加载，APK 中仍含共享构建生成的桌面资源。
- CI 的全项目 fmt/clippy、原生插件更完整的边界/仪器测试、依赖检查。当前 Rust/前端/提醒路由单元测试和 APK 签名校验已纳入 CI。
- 外部网页 App Links/通用外部深链；当前是应用内部 hash 路由与通知定位。

## 已完成，不再重复计作全新缺口

- Rust 股票代码归一化、自选/交易日志/持仓推导、日复盘、基础 AI 配置/聊天、Keystore。
- SQLite 版本迁移、版本号来源收口、前端入口按运行环境懒加载（共享构建仍生成桌面资源）、本地文档库、JSON 备份/恢复、文本分享。
- WorkManager 业务提醒、提醒点击定位、应用内路由/返回、盯盘快照、Lite 回测、基础研究/候选/本地日历。
- 12 行业资讯、GPU 租金现货/历史、板块详情/成分股/历史走势。
- 0.1.16：日线及分钟累计资金流、异常波动与重点监控、个股所属板块反查、单股量价观察。
- 0.1.17：全页明暗主题/跟随系统、显式设置与模型配置入口、紧凑/舒适密度、前台刷新偏好、首页快捷功能与自选行情表、6 指数及成交额。真机验收独立保留。
- 0.1.22（已发布预览版）：盯盘实时动态与竞价/盘中路径、涨停样本、每日复盘市场面板、交易日志风险与纪律及费率/规矩/账户规模设置；schema/备份 v6；修复长文本撑宽整页的样式隐患（`.emo-page` 列宽、`.watch-row` 最小宽度）。
- 0.1.23（随本轮发布）：题材事件树与首板涨停原因、涨停分档、结构变化页、沪深 300 估值与股债利差、昨日炸板股修复；以及 0.1.22 之后合并的交易日志分析页——交易日志模式卡（版本化）、复查清单与 MFE/MAE、按入场环境分组、判断×执行四象限、问题记录与逐笔修复；schema/备份升到 v7（`mobile_journal_modes` 进备份，两张行情缓存表不进）。
- 0.1.21：个股盘口 20 项 + 五档 + 外内盘；K 线分时/五日/日周月/四档分钟、复权切换、MA/BOLL 主图、VOL/MACD/KDJ/RSI 副图、拖动平移/捏合与按钮缩放/十字线信息条；东财⇄腾讯互为备用。
- 0.1.20：近 5 天（热度、行业轮动矩阵、龙头谱系）、情绪历史（六项分位、偏热偏冷、噪声门槛以上变化、逐日读数）、明日验证（8 个读数、基准发生率、09:15 锁定、事后/暂定不计、累计战绩）；每日读数 SQLite 缓存；schema/备份 v5。
- 0.1.19：五大一级栏目 + 栏目内子页签 + 可搜索全部功能；今日情绪（赚钱效应/晋级率/连板溢价/梯队断层/情绪周期/封板质量）、连板天梯（含没接上的票）、涨停/炸板/跌停全表、首板名单、盘面事实（反馈矩阵/亏钱效应/分制度）、行业题材与关键事件账本；首页情绪速览；资料库分页与检索。
- 0.1.18：窄屏研究/量价标签保持单行、三列指标收紧、个股完整 60 根涨跌窗口和缺失成交额处理；CI 增加单行标签及三列指标检查，实际截图已人工查看。

## 发布约定

**2026-10-17 起的长期指示（仓库主人在对话里明确）**：① 要把本清单里的功能都实现；② 之后每轮开发结束**升版本号、写 `docs/android/releases/<版本>.md`、提交、push 并由 CI 发布预览版**，不再逐版本询问；③ **不再做发布前隐私扫描**（密钥/个人成交不入库、不进 AI、夹具只用公开行情或合成数据的约束继续有效）。AGENTS.md 6.4/6.5 已同步。我（Agent）在这个环境里做不了的两项会一直如实留在清单里：**真机验收**（需要实体设备）、**正式签名 / AAB**（需要仓库主人持有的发布密钥，不能由 Agent 生成后放进 CI 密钥）。

维护者已要求每轮开发后构建并发布预览版。Android CI 在构建、测试、APK 验证成功后，发布不可覆盖的版本标签与 APK/SHA256；正式生产版签名与商店发布仍需单独安排。

维护者已明确授权后续 Android 预览版在现有检查通过后持续发布，并要求删除 AGENTS.md 中强制使用 Opus 5 审计的约束；该约束已移除，不再需要逐版本申请模型豁免。测试、发布前隐私检查与如实记录验证结果的要求继续适用。

</details>


### 0.1.35 PDF 原始页码补齐（2026-10-18）

PDF 采用逐页提取，含空白页的原始页序随正文和来源标记一起进入现有业务备份，不改备份格式。检索只返回单页内匹配片段和真实 citation，模型漏引、错页、引用未召回页均拒绝；旧导入文本不猜页码，编辑后按普通文本处理。双页 PDF（第一页空白）生成、导入、备份恢复、检索及错页拒绝端到端测试通过。Rust 288 项 + 1 项 parity、前端 565 项与移动构建通过。273f479 的跨平台六组 CI 已全部通过；本次 PDF 增量仍需后续 CI。台系四家公司真实月营收已跑通，事件概率接口连接重置仍未通过。


### 0.1.35 缓存刷新复核（2026-10-18）

手动刷新明确传 refresh=1，保留原证券、日期与筛选参数，不再被正常 TTL 缓存拦住。HTTP 200 中的 failed/error 结果也触发带陈旧标记的旧公开快照回退，不覆盖好缓存；参数 400 错误不回退掩盖，部分成功仍保留可用数据。新增刷新参数与缓存回退回归；Rust 289 项 + 1 项 parity、前端 566 项、移动构建通过，PDF 增量的 ARM64 目标检查通过。最终 APK 与发布回执待最新 CI。


最终功能提交 `69eb810`：跨平台 [#39098061530](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061530) 六组已全部成功；本机最新候选 174 张截图通过。Android [#39098061563](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39098061563) 的 APK / 原生测试 / 发布结果继续等待最终回执。

</details>



0.1.37 收尾修复：指数日历不可用时，也用已取得的逐日日状态补齐停牌日期；缺价日不成交、按最近已知收盘结转。新增对应回归测试，最终测试项数以 CI 回执为准。

产业快照预算修复：研究仅附带至多二十条、二十八 KiB 的观测；数值/单位/日期/ID/出处不裁改，摘录至多四百字并标记，省略条数、原有冲突数和无效记录数明确返回。完整观测仍在资料库及备份，避免长摘录使整份来源被排除；不是截断已完成报告。


工程检查回执（2026-10-19）：全量 Rust `cargo fmt --check` 未通过（已有格式差异）；`cargo clippy --all-targets --no-deps -- -D warnings` 未通过（库 60 项、含测试 70 项 lint）。这两项不计为通过。Build Tools 37 本机一次性合成签名包验证通过，新证书解析器读取并核对了真实工具输出；不代表设备安装升级验收。


提交 `10d3a87` 的[跨平台 CI #39146642264](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/39146642264) 六组成功。Linux / Node 24：编排器 1429 项通过 / 4 跳过、前端 574 项、mypy 115 个文件、发行脚本 6 项、388 例 oracle 散列及 Python 311 + 583 + 783 项均通过。Android CI #39146642305 的 ARM64 debug APK 和原生单元测试已通过，等待 release APK/AAB 签名验证和发布。


使用者本轮已明确：暂时没有 Android 设备，先完成 CI 和预览交付。真机验收保留为后续项，本轮不再请求连接设备；交易数据与模型处理继续保持现状。


0.1.38 候选 `e448383` 的跨平台 #39148052991 六组成功；Android #39148053065 的 debug/release APK、AAB、原生 JUnit 和测试签名验证通过，最终包证书与守卫一致（7C:2B:B8:8E:E4:E1:C6:20:29:93:5B:92:6F:F0:0F:2C:43:2C:5C:B7:76:AE:1E:8E:95:29:12:78:13:B2:35:8F），但新增版本检查阻止发布。直接检查已发布 0.1.37 的二进制 manifest，确认内部仍是默认 versionName=1.0 / versionCode=1。修复为在 tauri.conf.json 显式填写 0.1.38；构建前与 Cargo 对照、最终 debug/release APK 核对 0.1.38 / 1038；帮助脚本 9 项通过，待新 CI。以后升级必须同步 tauri.conf.json 与两处 Cargo 版本。

</details>

</details>
