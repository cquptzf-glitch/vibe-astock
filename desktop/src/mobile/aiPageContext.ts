// 「附带当前页面」给 AI:公开行情页面把自己的读数登记在这里,AI 对话里用户明确打开开关才会随这一问发出去。
//
// 🔴 个人页面(交易日志各页、持仓、盯盘里的持仓)**不得登记**,并且进入这些页面时清掉上一页留下的登记:
//    成交、持仓、成本、盈亏、金额不进 AI(AGENTS.md 6.2)。MobileApp 里的 PERSONAL_PAGES 负责清,这个文件的源码里不出现个人数据字段。
// 登记的内容只在内存里,不落盘;切到别的页面登记也不会自动发出,只有用户在 AI 对话里点开「附带」才发。

import { useEffect, useSyncExternalStore } from "react";

import type { EmotionData, EmotionStock } from "./emotionData";
import type { HistoryData, WeeklyData } from "./HistoryPanels";
import type { MobilePage } from "./mobileRoutes";

export interface AiPageContext { title: string; text: string; at: number }

const TEXT_MAX = 7_500;
const TITLE_MAX = 60;

let current: AiPageContext | null = null;
let queuedPrompt: string | null = null;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((fn) => fn());

export function setAiPage(title: string, text: string): void {
  const t = text.trim();
  if (!t) return;
  const next = { title: title.slice(0, TITLE_MAX), text: t.length > TEXT_MAX ? `${t.slice(0, TEXT_MAX)}\n…(已截断)` : t, at: Date.now() };
  if (current && current.title === next.title && current.text === next.text) return; // 同样内容不重复通知
  current = next;
  emit();
}

export function clearAiPage(): void {
  if (!current) return;
  current = null;
  emit();
}

export function getAiPage(): AiPageContext | null {
  return current;
}

export function useAiPageContext(): AiPageContext | null {
  return useSyncExternalStore((fn) => { listeners.add(fn); return () => { listeners.delete(fn); }; }, () => current, () => null);
}

/** 页面里调用:有内容就登记(离开页面后保留,直到被别的页面覆盖或进入个人页面时清掉) */
export function useRegisterAiPage(title: string, text: string | null): void {
  useEffect(() => { if (text) setAiPage(title, text); }, [title, text]);
}

/** 「问 AI」按钮:把问题放进待发队列,AI 对话打开时取走(并默认附带当前页面) */
export function queueAiPrompt(question: string): void {
  queuedPrompt = question.trim() || null;
}

export function takeQueuedAiPrompt(): string | null {
  const q = queuedPrompt;
  queuedPrompt = null;
  return q;
}

/** 这些页面显示个人成交 / 持仓 / 盈亏,进入时清掉登记,绝不给 AI */
export const PERSONAL_PAGES: ReadonlySet<MobilePage> = new Set<MobilePage>([
  "hold", "records", "daily", "review", "journalRisk", "journalModes", "journalExcursion", "journalEnv", "journalAttribution", "journalProblems", "monitor", "intraday",
]);

// ------------------------------------------------------------ 页面文字(都是公开行情读数,不含个人数据)

const pct = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : `${v >= 0 ? "+" : ""}${v.toFixed(2)}%`);
const rate = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : `${Math.round(v * 1000) / 10}%`);
const yi = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : `${(v / 1e8).toFixed(1)}亿`);

/** 今日情绪页:四项读数 + 梯队 + 周期,只写页面上已有的数字;取不到的写原因,不写 0 */
export function emotionContext(d: EmotionData): string {
  const s = d.summary;
  const lines = [
    `复盘日 ${d.date}${d.settled ? "" : "(盘中未收盘)"}。涨停 ${s.limit_up},炸板 ${s.broken ?? "—"}(炸板率 ${rate(s.broken_rate)}),跌停 ${s.limit_down ?? "—"}(封板率 ${rate(s.seal_rate)}),最高 ${s.highest} 板,连板 ${s.consec_count},首板 ${s.first_count}。`,
  ];
  const me = d.money_effect;
  lines.push(me.available ? `赚钱效应(昨日涨停股今日表现):中位 ${pct(me.median)},均值 ${pct(me.avg)},翻红率 ${rate(me.positive_rate)},再度涨停 ${rate(me.limit_up_again_rate)},样本 ${me.sample} 只。` : `赚钱效应:暂不可用(${me.reason})。`);
  const pr = d.promotion;
  lines.push(pr.available ? `晋级率:整体 ${pr.overall.promoted}/${pr.overall.base}(${rate(pr.overall.rate)});${Object.entries(pr.tiers).map(([k, t]) => `${k} ${t.promoted}/${t.base}`).join("、")}。` : `晋级率:暂不可用(${pr.reason})。`);
  const cp = d.consec_premium;
  lines.push(cp.available ? `连板溢价(昨日 2 板以上):中位 ${pct(cp.median)},均值 ${pct(cp.avg)},翻红率 ${rate(cp.positive_rate)},样本 ${cp.sample} 只。` : `连板溢价:暂不可用(${cp.reason})。`);
  const gap = d.ladder_gap;
  lines.push(gap.available ? `梯队:${gap.tiers.map((t) => `${t.boards}板×${t.count}`).join("、") || "无 2 板以上"}${gap.gaps.length ? `;断档 ${gap.gaps.map((g) => `${g}板`).join("、")}` : ""}。` : `梯队:暂不可用(${gap.reason})。`);
  const sq = d.seal_quality;
  if (sq.available) lines.push(`封板质量:没炸过 ${sq.never_broken}(${rate(sq.never_broken_rate)}),炸后回封 ${sq.reopened},开盘秒板 ${sq.opening_seconds},14:30 后封 ${sq.late_seal}。`);
  lines.push("口径:只描述已经发生的盘面现象。");
  return lines.join("\n");
}

const stockLine = (s: EmotionStock, reason?: string) =>
  `${s.name} ${s.code} | ${s.sector || "—"} | 首封 ${s.first_seal ? s.first_seal.slice(0, 5) : "—"} | 炸板 ${s.broken_times} | 涨幅 ${pct(s.ret)} | 成交 ${yi(s.amount)} | 流通 ${yi(s.float_cap)}${s.rebound ? ` | ${s.label}` : ""}${reason ? ` | 原因:${reason}` : ""}`;

/** 首板名单页:汇总 + 逐只一行(最多 40 只,超出写明) */
export function firstBoardContext(d: EmotionData, reasons?: Record<string, string>): string | null {
  const fb = d.first_board;
  if (!fb.available) return null;
  const shown = fb.stocks.slice(0, 40);
  const lines = [
    `复盘日 ${d.date}。全部涨停 ${fb.total_limit_up} 只,其中首板 ${fb.first_count} 只(反包 ${fb.rebound_count} 只)。按首封时间排列:`,
    ...shown.map((s) => stockLine(s, reasons?.[s.code])),
  ];
  if (fb.stocks.length > shown.length) lines.push(`…另有 ${fb.stocks.length - shown.length} 只未列出。`);
  if (!reasons || Object.keys(reasons).length === 0) lines.push("涨停原因这次没取到或没有覆盖,不要凭印象补原因。");
  return lines.join("\n");
}

/** 连板天梯页:最高板在前逐层一行;股票总数最多列 40 只 */
export function ladderContext(d: EmotionData): string | null {
  const l = d.ladder;
  if (!l.available) return null;
  const lines = [`复盘日 ${d.date}。全部涨停 ${l.total_limit_up} 只,最高 ${l.highest} 板,连板 ${l.consec_count} 只${l.prev_available ? "" : "(前一交易日数据没取到,晋级率不可用)"}。`];
  let left = 40;
  const levels = [...l.levels].sort((a, b) => b.boards - a.boards);
  for (const level of levels) {
    const shown = level.stocks.slice(0, Math.max(0, left));
    left -= shown.length;
    const rateText = level.rate == null ? "晋级率不可用" : `晋级 ${level.promoted ?? "—"}/${level.base ?? "—"}(${rate(level.rate)})`;
    lines.push(`${level.boards === 1 ? "首板" : `${level.boards}板`} ${level.stocks.length} 只,${rateText}:` + (shown.length ? shown.map((s) => `${s.name} ${s.code} | ${s.sector || "—"} | 首封 ${s.first_seal ? s.first_seal.slice(0, 5) : "—"} | 炸板 ${s.broken_times}`).join(";") : "(未列出)"));
    if (shown.length < level.stocks.length) lines.push(`…该层另有 ${level.stocks.length - shown.length} 只未列出。`);
  }
  return lines.join("\n");
}

/** 题材页:题材逐个一行,最多 20 个 */
export function themesContext(d: EmotionData): string | null {
  const t = d.themes;
  if (!t.available) return null;
  const shown = t.themes.slice(0, 20);
  const lines = [`复盘日 ${d.date}。涨停分布在 ${t.theme_count} 个行业,集中度 ${rate(t.concentration)}(按东财所属行业归类)。`];
  for (const th of shown) {
    const cont = th.continuation_rate == null ? "" : `,昨日涨停延续 ${rate(th.continuation_rate)}`;
    lines.push(`${th.sector}:涨停 ${th.limit_up}(首板 ${th.first_boards}、连板 ${th.consec_boards}),最高 ${th.highest} 板,炸板 ${th.broken}${cont}`);
  }
  if (t.themes.length > shown.length) lines.push(`…另有 ${t.themes.length - shown.length} 个行业未列出。`);
  return lines.join("\n");
}

const fmtMetric = (value: number | null | undefined, kind: "count" | "rate" | "pct", unit: string) =>
  value == null || !Number.isFinite(value) ? "—" : kind === "rate" ? rate(value) : kind === "pct" ? pct(value) : `${Number.isInteger(value) ? value : value.toFixed(1)}${unit ? ` ${unit}` : ""}`;

/** 近 5 天页:逐日读数 + 行业轮动 + 龙头谱系(都是公开行情读数;本页不含也不会带上「我的交易」) */
export function weeklyContext(d: WeeklyData): string | null {
  if (!d.days.length) return null;
  const lines = [`截至 ${d.date},近 ${d.days.length} 个交易日(从早到晚):`];
  for (const day of d.days) {
    const leaders = day.leaders.slice(0, 3).map((l) => `${l.name}${l.boards}板`).join("、");
    lines.push(`${day.date}${day.settled === false ? "(未收盘)" : ""} | 涨停 ${day.limit_up ?? "—"} | 最高 ${day.highest_board ?? "—"} 板 | 炸板率 ${rate(day.broken_rate)} | 昨日涨停今日均涨跌 ${pct(day.money_effect)} | 1进2晋级率 ${rate(day.promotion_1to2)} | 龙头 ${leaders || "—"}${day.status !== "ok" && day.note ? ` | 说明:${day.note}` : ""}`);
  }
  const themes = d.rotation.themes.slice(0, 8);
  if (themes.length) {
    lines.push("行业轮动(按东财行业):");
    for (const t of themes) lines.push(`${t.sector}:今日涨停 ${t.today},近期活跃 ${t.active_days} 天,进前三 ${t.top3_days} 天,连续 ${t.streak} 天${t.streak_partial ? "(窗口内数据不全)" : ""}`);
  }
  const lineage = d.lineage.slice(0, 8);
  if (lineage.length) {
    lines.push("龙头谱系(窗口内出现过的高板):");
    for (const l of lineage) lines.push(`${l.name} ${l.code} | ${l.sector || "—"} | ${l.appear_date} 时 ${l.boards_then} 板${l.is_current_top ? "(现在仍是最高)" : ""} | 之后累计 ${pct(l.cum_return_since)}`);
  }
  if (d.gaps.length) lines.push(`数据缺口:${d.gaps.join(";")}。`);
  lines.push("口径:只描述已经发生的盘面现象。");
  return lines.join("\n");
}

/** 情绪历史页:各项读数在窗口内的位置(分位)与相对前一日的变化;样本不足的原样写 */
export function historyContext(d: HistoryData): string | null {
  if (!d.series.length) return null;
  const lines = [`截至 ${d.date},统计窗口 ${d.days} 天,实际有数据 ${d.series.filter((x) => x.status !== "missing").length} 天。`];
  if (d.context.available) {
    lines.push(`读数位置(样本 ${d.context.sample_days} 天,至少需要 ${d.context.min_samples} 天才给分位):`);
    for (const it of d.context.items) lines.push(`${it.label}:${fmtMetric(it.value, it.kind, it.unit)},分位 ${it.percentile == null ? "—" : `${Math.round(it.percentile * 100)}%`}${it.extreme ? `(${it.extreme})` : ""},窗口内 ${fmtMetric(it.hist_min, it.kind, it.unit)} ~ ${fmtMetric(it.hist_max, it.kind, it.unit)},中位 ${fmtMetric(it.hist_median, it.kind, it.unit)}`);
  } else {
    lines.push(`读数位置:暂不可用(${d.context.reason})。`);
  }
  if (d.diff.available) {
    lines.push(`相对 ${d.diff.prev_date} 的变化:` + (d.diff.changes.length ? d.diff.changes.map((c) => `${c.label} ${fmtMetric(c.prev, c.kind, "")}→${fmtMetric(c.cur, c.kind, "")}`).join(";") : "没有变化") + `(${d.diff.unchanged_count} 项持平)`);
  } else {
    lines.push(`相对前一日的变化:暂不可用(${d.diff.reason})。`);
  }
  if (d.gaps.length) lines.push(`数据缺口:${d.gaps.join(";")}。`);
  lines.push("口径:只描述已经发生的盘面现象。");
  return lines.join("\n");
}
