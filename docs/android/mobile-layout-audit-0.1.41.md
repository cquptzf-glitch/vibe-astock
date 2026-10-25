# 0.1.41 移动页面布局验收

范围为 `MOBILE_PATHS` 的全部 47 个路由，包含资料库兼容入口。功能目录本身包含 42 项，两个数量口径不同。所有测试数据均为公开冻结数据或合成样本。

本轮覆盖 320 / 360 / 390 / 768 像素及明暗主题。默认页面之外，还检查资料库全文/编辑、候选筛选、日历新增/月历、提醒新增/编辑、设置模型配置、AI多行输入及个股财务/估值/量价资金二级内容。共 83 个页面状态 × 8 组合 = 664 组检查；最终回执以本地 `verification.json` 为准。

自动断言页面根部无横向溢出、可见按钮/输入/summary至少44px、没有页面脚本异常、没有遗漏的测试API；截图等待真实页面完成加载并终止滚动动画。表格保留容器内滚动，长报告全文不截断。SSR使用统一渲染助手。

| 页面 | 路由 | 本轮布局与入口处理 |
| --- | --- | --- |
| 功能目录 | `/mobile/functions` | 保留分组目录、搜索及44px筛选入口 |
| 我的 | `/mobile/my` | 保留应用、日程、研究资料分区 |
| 消息中心 | `/mobile/my/messages` | 消息标题及批量/逐条操作换行排布 |
| 个股深挖 | `/mobile/research` | 搜索和自选行情优先，保留个股入口 |
| 今日情绪 | `/mobile/review/emotion` | 指标卡与赚钱效应单列，晋级率图表防溢出 |
| 连板天梯 | `/mobile/review/ladder` | 日期、名单页签与股票卡片触控区统一 |
| 首板名单 | `/mobile/review/first-board` | 日期和筛选前置，股票卡片自适应 |
| 盘面事实 | `/mobile/review/seal` | 反馈矩阵在容器内横向滚动 |
| 行业题材 | `/mobile/review/themes` | 题材卡片、标签及详情入口适配窄屏 |
| 涨停样本 | `/mobile/review/samples` | 历史范围页签换行，说明完整显示 |
| 市场复盘 | `/mobile/review/market` | 市场状态金额指标改两列，主题/结构详情保留 |
| 结构复盘 | `/mobile/review/structure` | 结构说明完整显示，编辑区按屏宽分列 |
| 周复盘 | `/mobile/review/weekly` | 周期页签与周统计卡片统一边距 |
| 历史对照 | `/mobile/review/history` | 时间筛选及历史矩阵保留局部横向滚动 |
| 明日验证 | `/mobile/review/verify` | 新增条件表单两列/窄屏单列，保留写入限制 |
| 研究记录 | `/mobile/library/notes` | 先检索后新建，支持全文及编辑 |
| 我的研报 | `/mobile/library/reports` | 检索前置，导入及归档明确分层 |
| 收件箱 | `/mobile/library/inbox` | 未读筛选、全文、编辑和逐条操作换行 |
| 首页 | `/mobile/market` | 保留搜索、常用入口、行情详情的顺序 |
| 个股详情 | `/mobile/stock`（带股票代码） | 行情图表/公司资料/量价资金分区，首次访问加载并保留已访问状态 |
| 实时动态 | `/mobile/shortline` | 名单页签与筛选换行，动态卡片统一触控区 |
| 盘中快照 | `/mobile/shortline/intraday` | 时段卡片与日期控件适配，正文完整显示 |
| 涨停全表 | `/mobile/shortline/limit-up` | 筛选及排序按窄屏分列，名单行可点 |
| 异常监控 | `/mobile/shortline/risk` | 补页面边距，名单/关键词/刷新分层 |
| 板块轮动 | `/mobile/discovery/rotation` | 板块类型、资金周期与详情卡片适配 |
| 资讯雷达 | `/mobile/discovery/news` | 资讯分类选中态明确，筛选按屏宽分列 |
| 产业信号 | `/mobile/discovery/signals` | 历史范围前置，租金/概率说明完整显示 |
| 板块详情 | `/mobile/board`（带板块代码） | 排序与分页易触控，成分股及日线保留 |
| 产业研究 | `/mobile/discovery/sector` | 产业目录两列，分步方法与操作分行 |
| 候选池 | `/mobile/discovery/candidate` | 筛选默认折叠，范围表单窄屏单列，全部指标换行显示 |
| 事件日历 | `/mobile/discovery/calendar` | 事件列表优先，新增表单折叠，当天提醒/取消/删除分行 |
| 历史回测 | `/mobile/discovery/backtest` | 保留Agent唯一入口，输入及结果表格适配 |
| 我的自选 | `/mobile/my/watch` | 分组、排序、编辑及识别入口保留并统一触控区 |
| 我的持仓 | `/mobile/my/hold` | 消除双重边距，分红/收益指标两列，完整分析保留 |
| 交易日志 | `/mobile/journal` | 保留记一笔主入口，摘要两列与成交编辑分列 |
| 每日复盘 | `/mobile/journal/daily` | 日期、市场概况、观察与明日验证分区 |
| 交易复盘 | `/mobile/journal/review` | 表现、纪律及风险卡片统一手机边距 |
| 风险与纪律 | `/mobile/journal/risk` | 金额指标两列，规矩编辑表单分列 |
| 模式卡 | `/mobile/journal/modes` | 模式版本表保留局部滚动，编辑操作可触控 |
| 复查 | `/mobile/journal/excursion` | 复查列表及逐笔矩阵保留完整内容 |
| 入场环境 | `/mobile/journal/environment` | 环境说明与逐笔表格局部滚动 |
| 归因分析 | `/mobile/journal/attribution` | 归因四格与明细在手机内自适应 |
| 问题记录 | `/mobile/journal/problems` | 逐条修改/删除操作换行，编辑表单分列 |
| AI 助手 | `/mobile/ai` | 可选工具折叠，回车换行，按钮或Ctrl/Meta+Enter发送 |
| 资料库兼容入口 | `/mobile/library` | 沿用研究记录布局，保留兼容路由 |
| 提醒规则 | `/mobile/reminders` | 新增折叠，点规则自动展开编辑，操作区分行 |
| 应用设置 | `/mobile/settings` | 外观/模型/通知/备份跳转，清除数据移到末尾 |

验证范围是本地浏览器和冻结/合成响应，不能代替源站连通、模型真实业务、Android WebView/键盘/系统返回与文件分享真机验收。交易核算与模型数据处理未改，SQLite v13 / 业务备份 v11 / 固定预览签名未改。CI/APK按维护者选择继续等待平台恢复，不能把本轮截图标为已发布安装包。
