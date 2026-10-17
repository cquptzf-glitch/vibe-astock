/**
 * 复盘 · 涨停分档(移动版,对应桌面版 ReviewTiersCard):复盘那天的涨停股按东财行业归类,和前两个交易日比,按只数分四档。
 * 只数是事实,档位是按只数的固定规则分的:不判断能不能持续、不预测、不排名个股。每个行业里排第一的是当天「最高板」(固定规则,不是推荐)。
 */
import { Layers3 } from "lucide-react";
import { useState } from "react";

import { useCachedResource } from "./emotionData";

export interface TierStock { symbol: string; name: string; industry: string; boards: number | null; first_seal: string | null; broken_times: number | null; tags: string[] }
export interface TierGroup { industry: string; tier: "mainline" | "today" | "scattered" | "single"; today: number; prev: [number | null, number | null]; stocks: TierStock[]; tags: { tag: string; count: number }[]; note: string }
export interface TiersData {
  date: string; settled: boolean; prev_dates: [string | null, string | null]; total: number | null; groups: TierGroup[];
  tiers: { id: TierGroup["tier"]; label: string; rule: string; industries: number; stocks: number }[]; gaps: string[]; source: string;
}

const TIER_CLASS: Record<TierGroup["tier"], string> = { mainline: "strong", today: "new", scattered: "flat", single: "flat" };
const prevText = (p: number | null) => (p == null ? "—" : String(p));

export function TiersView({ data, onOpenSymbol }: { data: TiersData; onOpenSymbol: (symbol: string) => void }) {
  const [tier, setTier] = useState<TierGroup["tier"] | "all">("mainline");
  const shown = data.groups.filter((g) => tier === "all" || g.tier === tier);
  return <div className="emo-tree" aria-label="涨停分档">
    <div className="emo-stats emo-stats-4">{data.tiers.map((t) => <button type="button" key={t.id} className={`emo-stat tier-btn${tier === t.id ? " on" : ""}`} aria-pressed={tier === t.id} onClick={() => setTier(t.id)} title={t.rule}>
      <span>{t.label}</span><strong>{t.stocks}</strong><small>{t.industries} 个行业</small></button>)}</div>
    <p className="emo-foot">{data.tiers.find((t) => t.id === (tier === "all" ? "mainline" : tier))?.rule}。前两个交易日:{data.prev_dates.map((d) => d ?? "—").join("、")}。</p>
    <button type="button" className="discovery-action" aria-pressed={tier === "all"} onClick={() => setTier(tier === "all" ? "mainline" : "all")}>{tier === "all" ? "只看当前档" : "显示全部档位"}</button>
    {data.total == null && <p className="mobile-empty" role="status">这一天的涨停池没取到,没法分档。</p>}
    {data.total != null && shown.length === 0 && <p className="mobile-empty">这个档位今天没有行业。</p>}
    {shown.map((g) => <div key={g.industry} className="emo-tree-node">
      <div className="emo-tree-head"><strong>{g.industry}</strong><em className={`emo-state ${TIER_CLASS[g.tier]}`}>{data.tiers.find((t) => t.id === g.tier)?.label}</em>
        <span>今天 <b>{g.today}</b> 只 · 前两日 {prevText(g.prev[0])} / {prevText(g.prev[1])}</span></div>
      {g.note && <p className="emo-warn">{g.note}</p>}
      {g.tags.length > 0 && <div className="emo-tree-meta"><span>题材标签:{g.tags.map((t) => `${t.tag}×${t.count}`).join("、")}</span></div>}
      <div className="emo-chips">{g.stocks.slice(0, 6).map((s) => <button type="button" key={s.symbol} onClick={() => onOpenSymbol(s.symbol)}>{s.name}<b>{s.boards ?? "—"}板{s.first_seal ? ` ${s.first_seal.slice(0, 5)}` : ""}</b></button>)}
        {g.stocks.length > 6 && <span>等 {g.stocks.length} 只</span>}</div>
    </div>)}
    {data.gaps.length > 0 && <ul className="watch-notes" aria-label="缺口说明">{data.gaps.map((x) => <li key={x}>{x}</li>)}</ul>}
    <p className="emo-foot">{data.source}。每个行业里按连板数、首封时间、代码排,第一只只是当天的最高板,不是推荐。</p>
  </div>;
}

export function TiersCard({ date, onOpenSymbol }: { date: string; onOpenSymbol: (symbol: string) => void }) {
  const result = useCachedResource<TiersData>(`/mobile/review/tiers${date ? "?date=" + date : ""}`);
  return <section className="watch-card" aria-label="涨停分档卡片">
    <div className="watch-card-head"><h2><Layers3 size={15} />涨停分档</h2></div>
    {result.data ? <TiersView data={result.data} onOpenSymbol={onOpenSymbol} />
      : result.error ? <p className="mobile-error" role="alert">涨停分档这次没取到:{result.error}</p>
        : <p className="mobile-empty" role="status">正在按行业归类涨停…</p>}
  </section>;
}
