// 「附带当前页面」给 AI:公开行情页面把自己的读数登记在这里,AI 对话里用户明确打开开关才会随这一问发出去。
//
// 🔴 个人页面(交易日志各页、持仓、盯盘里的持仓)**不得登记**,并且进入这些页面时清掉上一页留下的登记:
//    成交、持仓、成本、盈亏、金额不进 AI(AGENTS.md 6.2)。MobileApp 里的 PERSONAL_PAGES 负责清,这个文件的源码里不出现个人数据字段。
// 登记的内容只在内存里,不落盘;切到别的页面登记也不会自动发出,只有用户在 AI 对话里点开「附带」才发。

import { useEffect, useSyncExternalStore } from "react";

import type { EmotionData, EmotionStock } from "./emotionData";
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
