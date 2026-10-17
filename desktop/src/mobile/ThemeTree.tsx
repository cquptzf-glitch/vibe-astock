/**
 * 题材事件树(移动版,对应桌面版「题材事件树」):按同花顺涨停原因里的真实事件题材聚合,不是行业分类。
 * 每个题材按复盘顺序摆事实:昨日反馈 → 几点首封 → 扩散几只 → 梯队多高 → 炸了几个。
 * 拿不到题材串时整棵树标不可用,不退回行业分类冒充题材。题材归因来自源站编辑,不是公司确认。
 * 状态只由已发生的读数推出,不含对明天的判断;强 = 红、弱 = 绿(和涨跌同一套)。
 */
import { Network } from "lucide-react";

import { fmtPercent } from "./discoveryData";
import { fmtRate, useCachedResource } from "./emotionData";

export interface ThemeMember { code: string; name: string; boards: number; label: string; first_seal: string | null; broken_times: number }
export interface ThemeNode {
  tag: string; limit_up: number; first_boards: number; consec_boards: number; highest: number; broken: number; limit_down: number;
  first_seal: string | null; last_seal: string | null; broken_rate: number | null; prev_members: number; prev_still_up: number;
  continuation_rate: number | null; state: string; members: ThemeMember[];
}
export type ThemeTree =
  | { available: false; reason: string }
  | { available: true; source_note: string; tag_count: number; prev_covered: number; prev_total: number; has_prev: boolean; themes: ThemeNode[];
      concentration: number | null; covered: number; total_limit_up: number; coverage_rate: number | null };
export interface ThemeTreeData { date: string; prev_date: string | null; settled: boolean; tree: ThemeTree; reasons: Record<string, string>; reason_note?: string }

/** 题材状态的颜色:强(扩散、延续)红,弱(接力断档、没再涨停)绿,分歧黄,新出现蓝 */
export const STATE_CLASS: Record<string, string> = {
  今日新出现: "new", 扩散中: "strong", 延续: "strong", 维持: "flat", 分歧加大: "split", 高标独活: "split",
  接力断档: "weak", 昨日题材未再涨停: "weak", 昨日覆盖样本未见: "flat", 无昨日题材数据: "flat",
};

export function useThemeTree(date: string) {
  return useCachedResource<ThemeTreeData>(`/mobile/shortline/theme-tree${date ? "?date=" + date : ""}`);
}

export function ThemeTreeView({ data, onOpenSymbol }: { data: ThemeTreeData; onOpenSymbol: (symbol: string) => void }) {
  const t = data.tree;
  if (!t.available) return <p className="mobile-empty" role="status">题材事件树暂不可用:{t.reason}</p>;
  return <div className="emo-tree" aria-label="题材事件树">
    <p className="emo-foot">{t.source_note}</p>
    {t.themes.map((n) => <div key={n.tag} className="emo-tree-node">
      <div className="emo-tree-head"><strong>{n.tag}</strong><em className={`emo-state ${STATE_CLASS[n.state] ?? "flat"}`}>{n.state}</em>
        <span>涨停 <b>{n.limit_up}</b>(首板{n.first_boards}/连板{n.consec_boards})· 最高 <b>{n.highest}</b> 板</span>
        {n.broken > 0 && <small className="warn">炸{n.broken}</small>}{n.limit_down > 0 && <small className="down">跌停{n.limit_down}</small>}</div>
      <div className="emo-tree-meta">
        {n.first_seal && <span>首封 {n.first_seal}{n.last_seal && n.last_seal !== n.first_seal ? ` → 末封 ${n.last_seal}` : ""}</span>}
        {n.prev_members > 0 && <span>昨日 {n.prev_members} 只 → 今仍涨停 <b className={(n.continuation_rate ?? 0) >= 0.5 ? "up" : "down"}>{n.prev_still_up}</b>{n.continuation_rate != null ? `(延续 ${fmtRate(n.continuation_rate)})` : ""}</span>}
        {n.broken_rate != null && n.broken > 0 && <span>炸板率 {fmtRate(n.broken_rate)}</span>}
      </div>
      {n.members.length > 0 && <div className="emo-chips">{n.members.slice(0, 4).map((m) =>
        <button type="button" key={m.code} onClick={() => onOpenSymbol(m.code)}>{m.name}<b>{m.label}</b></button>)}</div>}
    </div>)}
    <p className="emo-foot">共 {t.tag_count} 个事件题材 · 题材串覆盖 {t.covered}/{t.total_limit_up} 只涨停{t.coverage_rate != null ? `(${fmtRate(t.coverage_rate)})` : ""}{t.concentration != null ? ` · 头部占全市场涨停 ${fmtRate(t.concentration)}` : ""}。题材归因来自源站编辑,不是公司确认。</p>
  </div>;
}

/** 行业题材页顶部的卡片:自己取数,取不到只在卡片里说原因,不影响下面的行业结构 */
export function ThemeTreeCard({ date, onOpenSymbol }: { date: string; onOpenSymbol: (symbol: string) => void }) {
  const result = useThemeTree(date);
  return <section className="emo-card" aria-label="题材事件树卡片">
    <div className="emo-card-head"><h2><Network size={14} /> 题材事件树</h2><small>同花顺涨停原因 · 不是行业分类</small></div>
    {result.data ? <ThemeTreeView data={result.data} onOpenSymbol={onOpenSymbol} />
      : result.error ? <p className="mobile-error" role="alert">题材事件树这次没取到:{result.error}</p>
        : <p className="mobile-empty" role="status">正在取涨停原因(逐页核对封板日期)…</p>}
  </section>;
}

export const fmtDelta = fmtPercent;
