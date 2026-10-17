/**
 * 复盘 · 涨停样本(移动版,对应桌面版 shortline/SamplesTab.tsx):前一天涨停的票,第二天收盘表现的历史统计
 * (按打法、按首板最后封板时间、按入场那天的情绪强弱分开)。
 * 🔴 事后名单的警告原样保留、不弱化:样本是「事后知道封住了」的票,冲板失败的完全不在统计里,不是策略回测。
 * 最好 / 最差、柱子按红涨绿跌上色。
 */
import { Clock3, Info, ListChecks } from "lucide-react";
import { useState } from "react";

import { fmtPercent } from "./discoveryData";
import { fmtRate, useCachedResource, useEmotionDate } from "./emotionData";
import { ReviewDateBar } from "./HistoryPanels";
import "./emotion.css";
import "./monitor.css";

export type Regime = "情绪强" | "情绪中" | "情绪弱" | "环境未覆盖";
export const REGIMES: Regime[] = ["情绪强", "情绪中", "情绪弱", "环境未覆盖"];
export interface Stats { sample: number; win_rate: number | null; avg: number | null; median: number | null; best: number | null; worst: number | null; limit_up_rate: number | null }
export interface SealBucket extends Stats { bucket: string }
export interface StrategyDay { date: string; sample: number; avg: number | null; regime: Regime; equity: number }
export interface StrategyResult { name: string; desc: string; overall: Stats; by_regime: Record<Regime, Stats>; daily: StrategyDay[]; final_equity: number }
export interface SamplesData {
  date: string; days: number; layer: "market_phenomenon"; sample_caveat: string; available: boolean; reason: string | null;
  days_used: number; date_from: string | null; date_to: string | null; missing_days: string[]; unsettled_days: string[];
  regime_cuts: { low: number | null; high: number | null }; seal_curve: SealBucket[]; strategies: StrategyResult[]; gaps: string[]; source: string;
}

export const WINDOWS = [20, 30, 60, 90];
/** 「全体涨停」是基准线,其它打法对照它才知道有没有多赚 */
export const BASELINE = "全体涨停";
const SMALL_SAMPLE = 60;
const EMPTY: Stats = { sample: 0, win_rate: null, avg: null, median: null, best: null, worst: null, limit_up_rate: null };
const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");

function RegimeRow({ name, s }: { name: string; s: Stats }) {
  if (!s.sample) return null;
  return <div className="smp-regime"><span>{name}</span><b className={tone(s.avg)}>{fmtPercent(s.avg)}</b><span>胜 {fmtRate(s.win_rate)}</span><small>{s.sample} 只次</small></div>;
}

function SealCurve({ curve }: { curve: SealBucket[] }) {
  const rows = curve.filter((b) => b.sample > 0);
  return <section className="watch-card" aria-label="首板最后封板时间曲线">
    <div className="watch-card-head"><h2><Clock3 size={15} />首板 · 最后封板时间曲线</h2></div>
    <p className="watch-hint">同样是首板,什么时候把板最终封住决定了期望是正是负。数据源给的是最后封板时间,所以「晚」同时意味着封得晚、或中途炸开过又回封 —— 两者都指向同一件事:这个板不稳。只统计首板,连板股的封板时间含义不同。</p>
    {rows.length === 0 ? <p className="watch-empty">这个窗口里没有首板样本。</p> : <>
      <div className="smp-curve">{rows.map((b) => {
        const avg = b.avg ?? 0;
        const w = Math.min(100, Math.abs(avg) * 30);
        return <div key={b.bucket} className="smp-curve-row">
          <span className="smp-bucket">{b.bucket}</span>
          <div className="smp-axis" aria-hidden><div className="neg">{avg < 0 && <i style={{ width: `${w}%` }} />}</div><div className="mid" /><div className="pos">{avg > 0 && <i style={{ width: `${w}%` }} />}</div></div>
          <b className={tone(b.avg)}>{fmtPercent(b.avg)}</b>
          <small>胜 {fmtRate(b.win_rate)} · 再涨停 {fmtRate(b.limit_up_rate)} · {b.sample} 只次</small>
        </div>;
      })}</div>
      <p className="watch-hint">柱长 = 次日均收益;以 0 为中轴,右红左绿(红涨绿跌)。</p>
    </>}
  </section>;
}

function StrategyCard({ s, base }: { s: StrategyResult; base: number | null }) {
  const o = s.overall ?? EMPTY;
  const isBase = s.name === BASELINE;
  const excess = !isBase && base != null && o.avg != null ? o.avg - base : null;
  return <section className={`watch-card${isBase ? " smp-base" : ""}`} aria-label={s.name}>
    <div className="smp-title">
      <h3>{s.name}</h3>
      {isBase && <em className="smp-chip">基准</em>}
      {excess != null && <em className={`smp-chip ${excess > 0 ? "up" : excess < 0 ? "down" : ""}`}>比基准 {fmtPercent(excess)}</em>}
      {o.sample > 0 && o.sample < SMALL_SAMPLE && <em className="smp-chip warn">样本偏小</em>}
    </div>
    <p className="watch-hint">{s.desc}</p>
    {!o.sample ? <p className="watch-empty">这个窗口里没有样本。</p> : <>
      <div className="intra-big smp-big">
        <div><strong className={tone(o.avg)}>{fmtPercent(o.avg)}</strong><span>次日均值</span></div>
        <div><strong className={tone(o.median)}>{fmtPercent(o.median)}</strong><span>中位数</span></div>
        <div><strong>{fmtRate(o.win_rate)}</strong><span>收红比例</span></div>
      </div>
      <p className="smp-line"><span>样本 {o.sample} 只次</span><span>再涨停 <b>{fmtRate(o.limit_up_rate)}</b></span><span>最好 <b className={tone(o.best)}>{fmtPercent(o.best)}</b></span><span>最差 <b className={tone(o.worst)}>{fmtPercent(o.worst)}</b></span></p>
      <h4 className="smp-sub">分情绪环境(按入场那天)</h4>
      {REGIMES.map((k) => <RegimeRow key={k} name={k} s={s.by_regime?.[k] ?? EMPTY} />)}
      <div className="smp-days" aria-hidden>{s.daily.map((d) => {
        const a = d.avg ?? 0;
        const h = `${Math.min(100, Math.abs(a) * 10)}%`;
        return <div key={d.date} title={`${d.date} | ${d.regime} | 均 ${fmtPercent(d.avg)} | ${d.sample} 只`}>
          <span className="top">{a > 0 && <i className="pos" style={{ height: h }} />}</span><span className="bot">{a < 0 && <i className="neg" style={{ height: h }} />}</span></div>;
      })}</div>
      <p className="watch-hint">逐日均收益(上红下绿 · 红涨绿跌)</p>
    </>}
  </section>;
}

/** 事后名单的警告不能删也不能弱化 */
export function Caveat({ text }: { text?: string }) {
  return <div className="smp-caveat" role="note"><Info size={14} /><span>{text || "样本是收盘仍在涨停池里的事后名单,包含一字板等未必能成交的股票,未排除排队买不到的情况。冲板失败且未回封的股票不在样本内。没有成交模拟,不能推导真实交易收益或打法期望。"}</span></div>;
}

export function SamplesView({ data: r }: { data: SamplesData }) {
  const base = r.strategies.find((s) => s.name === BASELINE)?.overall?.avg ?? null;
  const cuts = r.regime_cuts;
  return <div className="emo-body">
    {!r.available ? <div className="mobile-empty">这个窗口统计不了:{r.reason || "原因不明"}</div> : <>
      <p className="emo-meta"><Info size={13} /> {r.date_from} ~ {r.date_to} · 请求 {r.days} 个交易日,实际可用 <b>{r.days_used}</b> 天
        {cuts.low != null && cuts.high != null && <>。情绪环境 = 入场那天的情绪分(涨停家数 ×(1 + 最高连板)×(1 − 炸板率))在窗口里三等分:≤{cuts.low} 算弱,≥{cuts.high} 算强</>}</p>
      <SealCurve curve={r.seal_curve} />
      <h2 className="smp-heading"><ListChecks size={15} />按打法分开看 <small>样本少于 {SMALL_SAMPLE} 只次的标「样本偏小」</small></h2>
      {r.strategies.map((s) => <StrategyCard key={s.name} s={s} base={base} />)}
    </>}
    {r.missing_days.length > 0 && <p className="emo-foot">没取到、已剔除的 {r.missing_days.length} 个交易日:{r.missing_days.slice(0, 6).join("、")}{r.missing_days.length > 6 ? " …" : ""}</p>}
    {r.unsettled_days.length > 0 && <p className="emo-warn" role="status">还没收盘、没算进去的:{r.unsettled_days.join("、")}</p>}
    {r.gaps.length > 0 && <ul className="watch-notes" aria-label="缺口说明">{r.gaps.map((g) => <li key={g}>{g}</li>)}</ul>}
    <p className="emo-foot">{r.source}。东财池子只留约一个月;已收盘的日子存在本机,样本会越用越长。</p>
  </div>;
}

export function SamplesPanel() {
  const [date] = useEmotionDate();
  const [days, setDays] = useState(30);
  const result = useCachedResource<SamplesData>(`/mobile/shortline/samples?days=${days}${date ? "&date=" + date : ""}`);
  return <section className="mobile-section emo-page">
    <ReviewDateBar title="涨停样本" detail="前一天涨停的票,第二天收盘怎么样 · 不是策略回测" loading={result.loading} error={result.error} onRefresh={result.refresh} shownDate={result.data?.date} />
    <Caveat text={result.data?.sample_caveat} />
    <div className="mobile-stock-tabs" role="tablist" aria-label="涨停样本看多少个交易日">{WINDOWS.map((n) =>
      <button key={n} type="button" role="tab" aria-selected={days === n} className={days === n ? "active" : ""} onClick={() => setDays(n)}>{n} 天</button>)}</div>
    {result.data && <SamplesView data={result.data} />}
  </section>;
}
