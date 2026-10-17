import type { MobilePage } from "./mobileRoutes";

// 五个一级栏目对应桌面版左侧栏:首页 / 盯盘 / 复盘(含交易日志) / 研究(产业资讯、产业研究、研究工具) / 我的(我的股票、资料库、设置)。
export type WorkspaceGroup = "overview" | "watch" | "review" | "research" | "my";
export interface WorkspaceEntry { page: MobilePage; title: string; short: string; detail: string; group: WorkspaceGroup; keywords?: string }
export const WORKSPACE_GROUPS: { id: WorkspaceGroup; title: string; detail: string; home: MobilePage }[] = [
  { id: "overview", title: "首页", detail: "市场全景、情绪速览与常用入口", home: "market" },
  { id: "watch", title: "盯盘", detail: "自选、持仓实时动态与风险名单", home: "monitor" },
  { id: "review", title: "复盘", detail: "情绪复盘、每日复盘与交易日志", home: "emotion" },
  { id: "research", title: "研究", detail: "个股深挖、板块产业、资讯与研究工具", home: "research" },
  { id: "my", title: "我的", detail: "事件日历、提醒、资料库与设置", home: "my" },
];
export const WORKSPACE_ENTRIES: WorkspaceEntry[] = [
  { page: "market", title: "市场总览", short: "总览", detail: "六大指数、成交额、涨跌分布与自选报价", group: "overview" },
  { page: "functions", title: "全部功能", short: "全部", detail: "按工作流程浏览或搜索所有栏目", group: "overview" },
  { page: "monitor", title: "实时动态", short: "实时", detail: "盘中异动流、持仓自选、昨日梯队、大票与昨日成交额前十", group: "watch", keywords: "盯盘 异动 急拉 急跌 封板 开板 梯队" },
  { page: "intraday", title: "竞价与盘中", short: "竞价", detail: "09:25 竞价强弱、全天 6 个时点情绪路径、昨晚验证条件的盘中读数", group: "watch", keywords: "竞价 分时 快照 时点 梯队" },
  { page: "watch", title: "我的自选", short: "自选", detail: "价格、涨跌、成交额与自选管理", group: "watch" },
  { page: "hold", title: "我的持仓", short: "持仓", detail: "成交推导的仓位、成本、浮盈亏与止损覆盖", group: "watch" },
  { page: "risk", title: "异常监控", short: "异常", detail: "异常偏离和重点监控名单、窗口与分页", group: "watch", keywords: "风险 偏离" },
  { page: "emotion", title: "今日情绪", short: "情绪", detail: "赚钱效应、晋级率、连板溢价、梯队断层与情绪周期", group: "review", keywords: "情绪复盘 周期" },
  { page: "ladder", title: "连板天梯", short: "天梯", detail: "每层晋级率、没接上的票与反包标记", group: "review", keywords: "连板 高标 晋级" },
  { page: "limitUp", title: "涨停全表", short: "涨停", detail: "涨停/炸板/跌停三池，封单、换手、首封等完整字段", group: "review", keywords: "涨停池 炸板池 跌停池" },
  { page: "firstBoard", title: "首板名单", short: "首板", detail: "首封时间、反包标记、成交与流通市值", group: "review" },
  { page: "seal", title: "盘面事实", short: "事实", detail: "昨日强势股反馈矩阵、亏钱效应、封板质量与分制度", group: "review", keywords: "封板 炸板率 亏钱效应" },
  { page: "themes", title: "行业题材", short: "题材", detail: "行业梯队、跨日延续、涨停时间分布与关键事件", group: "review", keywords: "题材 事件 账本" },
  { page: "samples", title: "涨停样本", short: "样本", detail: "前一天涨停的票次日表现:按打法、首板封板时间、入场日情绪分开看", group: "review", keywords: "打法 首板 连板接力 封板时间 历史统计 事后名单" },
  { page: "structure", title: "结构变化", short: "结构", detail: "断点是市场变了、数据源变了还是规则变了:涨停结构比较、字段清单、人工登记的制度事件", group: "review", keywords: "字段 制度 数据源 漂移 门槛" },
  { page: "weekly", title: "近 5 天", short: "5天", detail: "热度走势、行业轮动矩阵与龙头谱系", group: "review", keywords: "轮动 龙头 最高标" },
  { page: "history", title: "情绪历史", short: "历史", detail: "六项读数的历史分位、偏热偏冷与逐日读数", group: "review", keywords: "分位 转热 转冷" },
  { page: "verify", title: "明日验证", short: "验证", detail: "晚上写下读数判断、下一场对账与累计战绩", group: "review", keywords: "条件 对账 命中率" },
  { page: "marketReview", title: "市场面板", short: "市场", detail: "每日复盘:市场状态八格、主要指数中长线、成交额与两融、全球市场、行业资金与龙虎榜", group: "review", keywords: "每日复盘 指数 成交额 两融 国债 全球 行业资金 龙虎榜 均线" },
  { page: "daily", title: "每日复盘", short: "日复", detail: "市场观察、题材轮动记录与明日验证", group: "review" },
  { page: "records", title: "交易日志", short: "日志", detail: "记一笔、成交明细、补成交与标注", group: "review", keywords: "成交 流水" },
  { page: "review", title: "交易统计", short: "统计", detail: "收益、胜率、盈亏比、纪律与在险资金", group: "review" },
  { page: "journalRisk", title: "风险与纪律", short: "纪律", detail: "权益曲线与回撤、近 10/20/50 笔、纪律归因、规矩检查、在险资金与费率设置", group: "review", keywords: "回撤 权益曲线 规矩 违规 在险 费率 账户规模 状态漂移" },
  { page: "journalModes", title: "模式卡", short: "模式", detail: "把自己的打法写下来、带版本,业绩按版本分段;改动次日生效、旧版本不动", group: "review", keywords: "打法 版本 规则 首板 低吸 接力 版本比较" },
  { page: "journalExcursion", title: "复查", short: "复查", detail: "复查清单(仓位偏大、持有偏久、计划外、没写止损…)与每笔最多赚到/亏到多少:最大浮盈浮亏与捕获率", group: "review", keywords: "最大浮盈 最大浮亏 MFE MAE 捕获率 盈利回吐 异常交易 收件箱 仓位偏大 持有偏久" },
  { page: "journalEnv", title: "入场环境", short: "环境", detail: "交易按入场那天的情绪强中弱、当时板位(首板/连板/炸板/没碰涨停)分组的胜率与盈亏", group: "review", keywords: "情绪环境 板位 炸板 首板 连板 入场日 分组" },
  { page: "journalAttribution", title: "看错还是没执行好", short: "归因", detail: "前一晚验证条件成不成立 × 那天赚没赚:判对赚、判对亏(执行问题)、判错赚(运气)、判错亏(判断问题)", group: "review", keywords: "归因 判断 执行 四象限 运气 验证条件 对账" },
  { page: "journalProblems", title: "问题记录", short: "问题", detail: "算不进统计的交易记录:卖在买之前/卖超、日期无效、空记录、没填盈亏的补记,逐笔改正或删除;非 A 股记录说明", group: "review", keywords: "修复 卖超 日期 空记录 补记 异常 ETF 非A股 纠错" },
  { page: "research", title: "个股深挖", short: "个股", detail: "搜索沪深北股票：行情、财务、公告、研报、资金与所属板块", group: "research", keywords: "股票 搜索 财务 公告" },
  { page: "rotation", title: "板块轮动", short: "板块", detail: "概念/行业今日与五日资金、成分股与走势", group: "research" },
  { page: "sector", title: "产业研究", short: "产业", detail: "产业链图谱与研究线索", group: "research" },
  { page: "news", title: "资讯雷达", short: "资讯", detail: "十二行业、来源筛选、时间范围与关注词", group: "research" },
  { page: "signals", title: "产业信号", short: "信号", detail: "B200/H100/A100 现货租金与历史趋势", group: "research", keywords: "GPU" },
  { page: "candidate", title: "候选池", short: "候选", detail: "确定性规则筛选与候选观察", group: "research" },
  { page: "backtest", title: "历史回测", short: "回测", detail: "单股均线交叉、历史收益、回撤与成交明细", group: "research" },
  { page: "ai", title: "AI 助手", short: "AI", detail: "研究对话、复盘解读与模型配置", group: "research" },
  { page: "calendar", title: "事件日历", short: "日历", detail: "事件记录、关联股票与后续验证", group: "my" },
  { page: "reminders", title: "提醒规则", short: "提醒", detail: "复盘、价格条件与通知定位", group: "my" },
  { page: "notes", title: "研究记录", short: "记录", detail: "正文检索、分类/股票筛选与本地笔记", group: "my" },
  { page: "reports", title: "我的研报", short: "研报", detail: "保存、查阅与检索报告正文", group: "my" },
  { page: "inbox", title: "收件箱", short: "收件", detail: "研究线索、未读过滤与处理状态", group: "my" },
  { page: "settings", title: "应用设置", short: "设置", detail: "外观、刷新、模型接入、通知与备份恢复", group: "my", keywords: "主题 备份 恢复" },
];
export function workspaceEntry(page: MobilePage): WorkspaceEntry | undefined {
  return WORKSPACE_ENTRIES.find((entry) => entry.page === page);
}
export function workspaceGroup(page: MobilePage): WorkspaceGroup {
  if (page === "stock" || page === "board") return "research";
  if (page === "library" || page === "my") return "my";
  return workspaceEntry(page)?.group ?? "overview";
}
export function groupEntries(group: WorkspaceGroup): WorkspaceEntry[] {
  return WORKSPACE_ENTRIES.filter((entry) => entry.group === group && entry.page !== "functions");
}
export function workspaceMatches(query: string, group?: WorkspaceGroup): WorkspaceEntry[] {
  const words = query.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  return WORKSPACE_ENTRIES.filter((entry) => (!group || entry.group === group)
    && words.every((word) => (entry.title + " " + entry.detail + " " + (entry.keywords ?? "")).toLocaleLowerCase().includes(word)));
}
