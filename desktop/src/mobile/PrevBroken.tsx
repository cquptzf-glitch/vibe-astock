/**
 * 昨日炸板股修复(移动版):前一交易日炸板池里的票,复盘日当天涨跌怎么样,以及反馈矩阵里的「昨日炸板」一行。
 * 这些票不在东财「昨日涨停表现」定稿记录里,所以用前一交易日炸板池名单加实时行情算;行情不属于复盘日就整块标不可用并说原因。
 * 只列已经发生的读数,不给个股倾向。
 */
import { Undo2 } from "lucide-react";

import { fmtPercent } from "./discoveryData";
import { fmtRate, useCachedResource } from "./emotionData";

export interface PrevBrokenRow { code: string; name: string; pct: number; result: string; broken_times: number; sector: string }
export type PrevBrokenResult =
  | { available: false; reason: string; total?: number; missing?: number }
  | { available: true; total: number; sample: number; missing: number; coverage_rate: number; avg: number; median: number; positive_rate: number;
      limit_up_again: number; loss5: number; feedback_row: Record<string, string | number>; result_order: string[]; rows: PrevBrokenRow[]; note?: string };
export interface PrevBrokenData { date: string; prev_date: string | null; settled: boolean; partial: boolean; source: string; note: string; result: PrevBrokenResult }

const tone = (v: number) => (v > 0 ? "up" : v < 0 ? "down" : "");

export function PrevBrokenView({ data, onOpenSymbol }: { data: PrevBrokenData; onOpenSymbol: (symbol: string) => void }) {
  const r = data.result;
  if (!r.available) return <p className="mobile-empty" role="status">昨日炸板股修复暂不可用:{r.reason}</p>;
  const row = r.feedback_row;
  return <div className="emo-body" aria-label="昨日炸板股修复">
    <div className="emo-stats emo-stats-4">
      <div className="emo-stat"><span>当天均</span><strong className={tone(r.avg)}>{fmtPercent(r.avg)}</strong><small>中位 {fmtPercent(r.median)}</small></div>
      <div className="emo-stat"><span>翻红</span><strong>{fmtRate(r.positive_rate)}</strong><small>涨幅 &gt; 0</small></div>
      <div className="emo-stat"><span>再封板</span><strong className={r.limit_up_again > 0 ? "up" : ""}>{r.limit_up_again}</strong><small>涨停价收</small></div>
      <div className="emo-stat"><span>跌超5%</span><strong className={r.loss5 > 0 ? "down" : ""}>{r.loss5}</strong><small>含跌停</small></div>
    </div>
    <div className="emo-matrix" role="table" aria-label="昨日炸板反馈矩阵">
      <div role="row" className="head"><span role="columnheader">昨日</span>{r.result_order.map((k) => <span role="columnheader" key={k}>{k}</span>)}<span role="columnheader">晋级率</span></div>
      <div role="row"><span role="rowheader">{String(row.tier)}<small>{row.合计}</small></span>
        {r.result_order.map((k) => <span role="cell" key={k} className={Number(row[k]) === 0 ? "zero" : k === "晋级涨停" || k === "收红" ? "up" : k === "跌超5%" || k === "跌停" ? "down" : ""}>{row[k] ?? 0}</span>)}
        <span role="cell">{fmtRate(Number(row.晋级率))}</span></div>
    </div>
    <div className="emo-detail-list">{r.rows.map((s) => <button type="button" key={s.code} onClick={() => onOpenSymbol(s.code)}>
      <span>{s.name}<small>炸 {s.broken_times} 次 · {s.sector || "—"}</small></span><em>{s.result}</em><b className={tone(s.pct)}>{fmtPercent(s.pct)}</b></button>)}</div>
    <p className="emo-foot">样本 {r.sample}/{r.total} 只(覆盖 {fmtRate(r.coverage_rate)}{r.missing > 0 ? `,${r.missing} 只没取到复盘日行情` : ""})。{data.partial ? "复盘日还没收盘,这是此刻的读数,收盘前还会变。" : ""}{r.note ?? ""}{data.note}</p>
  </div>;
}

export function PrevBrokenCard({ date, onOpenSymbol }: { date: string; onOpenSymbol: (symbol: string) => void }) {
  const result = useCachedResource<PrevBrokenData>(`/mobile/shortline/prev-broken${date ? "?date=" + date : ""}`);
  return <section className="emo-card" aria-label="昨日炸板股修复卡片">
    <div className="emo-card-head"><h2><Undo2 size={14} /> 昨日炸板股修复</h2><small>前一交易日炸板池 → 今天</small></div>
    {result.data ? <PrevBrokenView data={result.data} onOpenSymbol={onOpenSymbol} />
      : result.error ? <p className="mobile-empty" role="status">这次没取到:{result.error}</p>
        : <p className="emo-foot">加载中…</p>}
  </section>;
}
