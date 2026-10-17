/**
 * 我的持仓 · 收益表现与持仓体检(移动版)。要逐只取一年到三年的日线,所以只在点「开始」时才算,结果在本次打开期间记住 30 分钟。
 * 收益表现:时间加权(选的票本身走得怎么样)、资金加权与年化(这些钱实际赚了多少)、逐只盈亏。
 * 持仓体检:集中度、最近一年的波动 / 回撤 / VaR、每只的风险贡献、两两相关;超线只对照用户自己设的提醒线。
 * 只给数字和缺口,不出综合分、不评好坏、不给操作提示。个人持仓与收益只在本机,不进任何 AI。
 */
import { Activity, Gauge, History, Play, RefreshCw } from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import { fmtNumber, fmtPercent } from "./discoveryData";
import { priv } from "./privacy";
import "./portfolio-analysis.css";

// ---------------------------------------------------------------- 类型
export interface Gap { symbol: string; why: string }
export interface PerfSymbol { symbol: string; name: string; invested: number; withdrawn: number; shares: number; value: number; pnl: number }
export type PerfData =
  | { ok: false; reason: string; gaps: Gap[] }
  | { ok: true; from: string; to: string; span_days: number; trading_days: number; twr_pct: number; money_weighted_pct: number; xirr_pct: number | null; xirr_note: string | null;
      invested: number; withdrawn: number; final_value: number; pnl: number; series: { date: string; twr_pct: number }[]; symbols: PerfSymbol[]; gaps: Gap[]; warnings: string[]; note: string };
export interface XrayWeight { symbol: string; name: string; value: number; weight_pct: number; industry: string | null }
export interface XrayAsset { symbol: string; name: string; weight_pct: number; vol_pct: number; risk_share_pct: number }
export interface XrayRisk {
  window: { from: string; to: string; returns: number }; annual_vol_pct: number; max_drawdown_pct: number; downside_dev_pct: number;
  var95_pct: number | null; cvar95_pct: number | null; var99_pct: number | null; best_day_pct: number; worst_day_pct: number; avg_pair_corr: number | null; diversification_ratio: number | null;
  assets: XrayAsset[]; top_pairs: { a: string; a_name: string; b: string; b_name: string; corr: number }[];
}
export type XrayData =
  | { ok: false; reason: string; gaps: Gap[] }
  | { ok: true; concentration: { positions: number; total_value: number; top1_pct: number; top3_pct: number; hhi: number; effective_n: number; top_industry_pct: number | null;
      industries: { industry: string; weight_pct: number; known: boolean }[]; weights: XrayWeight[] }; risk: XrayRisk | null; risk_reason: string | null; gaps: Gap[]; note: string };
export interface ReplayStock { symbol: string; name: string; weight_pct: number; return_pct: number; contribution_pct: number }
export interface ReplayPeriod { return_pct: number; max_drawdown_pct: number; worst_day_pct: number | null; days?: number; label?: string }
export interface ReplayScenario { id: string; label: string; start: string; end: string; covered_pct: number; stocks: ReplayStock[]; gaps: Gap[]; index: ReplayPeriod | null; portfolio: ReplayPeriod | null; portfolio_reason: string | null }
export type ReplayData = { ok: false; reason: string; gaps: Gap[] } | { ok: true; scenarios: ReplayScenario[]; gaps: Gap[]; index_available: boolean; note: string };
export interface Thresholds { top1: number | null; industry: number | null; vol: number | null; drawdown: number | null }

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const signed = (v: number) => priv((v > 0 ? "+" : "") + fmtNumber(v));
const label = (x: { name: string; symbol: string }) => x.name || x.symbol;

// ---------------------------------------------------------------- 提醒线(用户自己设,存本机浏览器)
const TH_KEY = "vibe.portfolio.xray.thresholds";
export const NO_THRESHOLDS: Thresholds = { top1: null, industry: null, vol: null, drawdown: null };
export function parseThresholds(raw: string | null): Thresholds {
  try {
    const v = JSON.parse(raw ?? "null") as Partial<Record<keyof Thresholds, unknown>> | null;
    const num = (x: unknown) => (typeof x === "number" && Number.isFinite(x) && x > 0 && x <= 1000 ? x : null);
    return v ? { top1: num(v.top1), industry: num(v.industry), vol: num(v.vol), drawdown: num(v.drawdown) } : NO_THRESHOLDS;
  } catch { return NO_THRESHOLDS; }
}
function loadThresholds(): Thresholds { try { return parseThresholds(window.localStorage.getItem(TH_KEY)); } catch { return NO_THRESHOLDS; } }

/** 对照用户自己设的线:超过了的项。没设的线不查;风险没算出来时不查波动和回撤 */
export function breaches(data: Extract<XrayData, { ok: true }>, t: Thresholds): string[] {
  const out: string[] = [];
  const c = data.concentration;
  if (t.top1 != null && c.top1_pct > t.top1) out.push(`单只最大占 ${fmtNumber(c.top1_pct)}%,超过你设的 ${t.top1}%`);
  if (t.industry != null && c.top_industry_pct != null && c.top_industry_pct > t.industry) out.push(`最大行业占 ${fmtNumber(c.top_industry_pct)}%,超过你设的 ${t.industry}%`);
  if (data.risk && t.vol != null && data.risk.annual_vol_pct > t.vol) out.push(`年化波动 ${fmtNumber(data.risk.annual_vol_pct)}%,超过你设的 ${t.vol}%`);
  if (data.risk && t.drawdown != null && -data.risk.max_drawdown_pct > t.drawdown) out.push(`最大回撤 ${fmtNumber(-data.risk.max_drawdown_pct)}%,超过你设的 ${t.drawdown}%`);
  return out;
}

// ---------------------------------------------------------------- 小折线
export function Spark({ points, ariaLabel }: { points: { date: string; v: number }[]; ariaLabel: string }) {
  if (points.length < 2) return null;
  const vs = points.map((p) => p.v);
  const lo = Math.min(0, ...vs), hi = Math.max(0, ...vs);
  const span = hi - lo || 1;
  const x = (i: number) => (i / (points.length - 1)) * 300;
  const y = (v: number) => 56 - ((v - lo) / span) * 52;
  const d = points.map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" ");
  return <svg viewBox="0 0 300 60" className="pa-spark" role="img" aria-label={ariaLabel} preserveAspectRatio="none">
    <line x1="0" x2="300" y1={y(0)} y2={y(0)} className="zero" /><path d={d} className="line" /></svg>;
}

function Gaps({ gaps }: { gaps: Gap[] }) {
  if (!gaps.length) return null;
  return <ul className="pa-gaps" aria-label="缺口说明">{gaps.map((g) => <li key={g.symbol + g.why}><b>{g.symbol}</b> {g.why}</li>)}</ul>;
}

// ---------------------------------------------------------------- 收益表现
export function PerfView({ data }: { data: PerfData }) {
  if (!data.ok) return <><p className="pa-empty" role="status">算不了:{data.reason}</p><Gaps gaps={data.gaps} /></>;
  return <div className="pa-body">
    <div className="pa-stats" aria-label="收益表现读数">
      <div><span>时间加权</span><strong className={tone(data.twr_pct)}>{fmtPercent(data.twr_pct)}</strong><small>票本身的涨跌</small></div>
      <div><span>资金加权</span><strong className={tone(data.money_weighted_pct)}>{fmtPercent(data.money_weighted_pct)}</strong><small>区间,不年化</small></div>
      <div><span>年化(XIRR)</span><strong className={tone(data.xirr_pct)}>{data.xirr_pct == null ? "—" : fmtPercent(data.xirr_pct)}</strong><small>{data.xirr_pct == null ? "不满一年不给" : "内部收益率"}</small></div>
      <div><span>累计盈亏</span><strong className={tone(data.pnl)}>{signed(data.pnl)}</strong><small>元(含分红)</small></div>
    </div>
    <Spark ariaLabel="时间加权收益走势" points={data.series.map((s) => ({ date: s.date, v: s.twr_pct }))} />
    <p className="pa-line">{data.from} ~ {data.to} · {data.trading_days} 个交易日 · 总投入 {priv(fmtNumber(data.invested))} · 总取出 {priv(fmtNumber(data.withdrawn))} · 期末市值 {priv(fmtNumber(data.final_value))}</p>
    {data.xirr_note && <p className="pa-note">{data.xirr_note}</p>}
    <div className="intra-table-wrap"><table className="intra-table" aria-label="逐只盈亏">
      <thead><tr><th>股票</th><th>投入</th><th>取出</th><th>市值</th><th>盈亏</th></tr></thead>
      <tbody>{data.symbols.map((s) => <tr key={s.symbol}><td><b>{label(s)}</b></td><td>{priv(fmtNumber(s.invested, 0))}</td><td>{priv(fmtNumber(s.withdrawn, 0))}</td><td>{priv(fmtNumber(s.value, 0))}</td><td className={tone(s.pnl)}>{signed(Math.round(s.pnl))}</td></tr>)}</tbody></table></div>
    {data.warnings.length > 0 && <ul className="pa-warn" aria-label="提醒">{data.warnings.map((w) => <li key={w}>{w}</li>)}</ul>}
    <Gaps gaps={data.gaps} />
    <p className="pa-note">{data.note}</p>
  </div>;
}

// ---------------------------------------------------------------- 持仓体检
export function XrayView({ data, thresholds }: { data: XrayData; thresholds: Thresholds }) {
  if (!data.ok) return <><p className="pa-empty" role="status">算不了:{data.reason}</p><Gaps gaps={data.gaps} /></>;
  const c = data.concentration, r = data.risk;
  const hits = breaches(data, thresholds);
  return <div className="pa-body">
    <div className="pa-stats" aria-label="集中度">
      <div><span>单只最大</span><strong>{fmtNumber(c.top1_pct)}%</strong><small>前三合计 {fmtNumber(c.top3_pct)}%</small></div>
      <div><span>等效只数</span><strong>{fmtNumber(c.effective_n)}</strong><small>实际 {c.positions} 只</small></div>
      <div><span>最大行业</span><strong>{c.top_industry_pct == null ? "—" : fmtNumber(c.top_industry_pct) + "%"}</strong><small>东财所属行业</small></div>
      <div><span>持仓市值</span><strong>{priv(fmtNumber(c.total_value, 0))}</strong><small>元</small></div>
    </div>
    {hits.length > 0 ? <ul className="pa-hit" aria-label="超过你设的提醒线">{hits.map((h) => <li key={h}>{h}</li>)}</ul>
      : <p className="pa-note">{Object.values(thresholds).every((v) => v == null) ? "还没设提醒线;设了之后,超线的项会列在这里。" : "没有超过你设的提醒线。"}</p>}
    <div className="intra-table-wrap"><table className="intra-table" aria-label="持仓权重与行业">
      <thead><tr><th>股票</th><th>权重</th><th>行业</th></tr></thead>
      <tbody>{c.weights.map((w) => <tr key={w.symbol}><td><b>{label(w)}</b></td><td>{fmtNumber(w.weight_pct)}%</td><td>{w.industry ?? "行业没取到"}</td></tr>)}</tbody></table></div>
    {r ? <>
      <div className="pa-stats" aria-label="风险读数">
        <div><span>年化波动</span><strong>{fmtNumber(r.annual_vol_pct)}%</strong><small>按现在的比例回看</small></div>
        <div><span>最大回撤</span><strong className="down">{fmtNumber(r.max_drawdown_pct)}%</strong><small>{r.window.from} ~ {r.window.to}</small></div>
        <div><span>历史 VaR95</span><strong>{r.var95_pct == null ? "—" : fmtNumber(r.var95_pct) + "%"}</strong><small>CVaR {r.cvar95_pct == null ? "—" : fmtNumber(r.cvar95_pct) + "%"}</small></div>
        <div><span>下行波动</span><strong>{fmtNumber(r.downside_dev_pct)}%</strong><small>最差一天 {fmtNumber(r.worst_day_pct)}%</small></div>
        <div><span>平均相关</span><strong>{r.avg_pair_corr == null ? "—" : fmtNumber(r.avg_pair_corr)}</strong><small>两两相关均值</small></div>
        <div><span>分散化比</span><strong>{r.diversification_ratio == null ? "—" : fmtNumber(r.diversification_ratio)}</strong><small>&gt;1 才有分散效果</small></div>
      </div>
      <div className="intra-table-wrap"><table className="intra-table" aria-label="每只的波动与风险贡献">
        <thead><tr><th>股票</th><th>权重</th><th>年化波动</th><th>风险贡献</th></tr></thead>
        <tbody>{r.assets.map((a) => <tr key={a.symbol}><td><b>{label(a)}</b></td><td>{fmtNumber(a.weight_pct)}%</td><td>{fmtNumber(a.vol_pct)}%</td><td>{fmtNumber(a.risk_share_pct)}%</td></tr>)}</tbody></table></div>
      {r.top_pairs.length > 0 && <ul className="pa-pairs" aria-label="相关性最高的几对">{r.top_pairs.map((p) => <li key={p.a + p.b}>{p.a_name || p.a} × {p.b_name || p.b}<b>{fmtNumber(p.corr)}</b></li>)}</ul>}
    </> : <p className="pa-empty" role="status">风险读数算不了:{data.risk_reason}</p>}
    <Gaps gaps={data.gaps} />
    <p className="pa-note">{data.note}</p>
  </div>;
}

// ---------------------------------------------------------------- 历史回放
export function ReplayView({ data }: { data: ReplayData }) {
  if (!data.ok) return <><p className="pa-empty" role="status">算不了:{data.reason}</p><Gaps gaps={data.gaps} /></>;
  return <div className="pa-body">
    {data.scenarios.map((s) => <article key={s.id} className="pa-scenario" aria-label={s.label}>
      <h3>{s.label}<small>{s.start} ~ {s.end}</small></h3>
      {s.portfolio ? <div className="pa-stats" aria-label={`${s.label} 组合读数`}>
        <div><span>组合涨跌</span><strong className={tone(s.portfolio.return_pct)}>{fmtPercent(s.portfolio.return_pct)}</strong><small>覆盖 {fmtNumber(s.covered_pct, 0)}% 市值</small></div>
        <div><span>最大回撤</span><strong className="down">{fmtNumber(s.portfolio.max_drawdown_pct)}%</strong><small>期间</small></div>
        <div><span>最差单日</span><strong className="down">{s.portfolio.worst_day_pct == null ? "—" : fmtNumber(s.portfolio.worst_day_pct) + "%"}</strong><small>{s.portfolio.days} 个交易日</small></div>
        <div><span>沪深 300</span><strong className={tone(s.index?.return_pct)}>{s.index ? fmtPercent(s.index.return_pct) : "—"}</strong><small>{s.index ? "回撤 " + fmtNumber(s.index.max_drawdown_pct) + "%" : "没取到"}</small></div>
      </div> : <p className="pa-empty" role="status">{s.portfolio_reason}</p>}
      {s.stocks.length > 0 && <ul className="pa-pairs" aria-label={`${s.label} 逐只`}>{s.stocks.map((x) => <li key={x.symbol}><span>{label(x)} <small>权重 {fmtNumber(x.weight_pct)}%</small></span><b className={tone(x.return_pct)}>{fmtPercent(x.return_pct)}</b></li>)}</ul>}
      {s.gaps.length > 0 && <Gaps gaps={s.gaps} />}
    </article>)}
    <Gaps gaps={data.gaps} />
    <p className="pa-note">{data.note}</p>
  </div>;
}

export function ThresholdEditor({ value, onSave }: { value: Thresholds; onSave: (t: Thresholds) => void }) {
  const [draft, setDraft] = useState({ top1: value.top1 == null ? "" : String(value.top1), industry: value.industry == null ? "" : String(value.industry), vol: value.vol == null ? "" : String(value.vol), drawdown: value.drawdown == null ? "" : String(value.drawdown) });
  const field = (key: keyof Thresholds, text: string) => <label key={key}><span>{text}</span>
    <input inputMode="decimal" aria-label={text} value={draft[key]} onChange={(e) => setDraft({ ...draft, [key]: e.target.value })} placeholder="不设" /></label>;
  const num = (s: string) => { const v = Number(s.trim()); return s.trim() && Number.isFinite(v) && v > 0 && v <= 1000 ? v : null; };
  return <div className="pa-thresholds" aria-label="提醒线">
    <p className="pa-note">自己定的线(%),超过才在体检结果里列出;不设就不查。只存在本机浏览器。</p>
    <div className="pa-th-grid">{field("top1", "单只占比不超过")}{field("industry", "单个行业不超过")}{field("vol", "年化波动不超过")}{field("drawdown", "最大回撤不超过")}</div>
    <button type="button" onClick={() => onSave({ top1: num(draft.top1), industry: num(draft.industry), vol: num(draft.vol), drawdown: num(draft.drawdown) })}>保存提醒线</button>
  </div>;
}

// ---------------------------------------------------------------- 取数卡片
const CACHE_MS = 30 * 60_000;
const cache = new Map<string, { at: number; body: unknown }>();

function useRun<T>(path: string) {
  const [state, setState] = useState<{ data: T | null; loading: boolean; error: string }>(() => {
    const hit = cache.get(path);
    return { data: hit && Date.now() - hit.at < CACHE_MS ? hit.body as T : null, loading: false, error: "" };
  });
  const run = useCallback(async () => {
    setState((s) => ({ ...s, loading: true, error: "" }));
    try {
      const response = await fetch("/api" + path, { headers: { Accept: "application/json" } });
      const body = await response.json();
      if (!response.ok) throw new Error(body?.message ?? `HTTP ${response.status}`);
      cache.set(path, { at: Date.now(), body });
      setState({ data: body as T, loading: false, error: "" });
    } catch (e) {
      setState((s) => ({ ...s, loading: false, error: e instanceof Error ? e.message : String(e) }));
    }
  }, [path]);
  return { ...state, run };
}

export function PortfolioAnalysis() {
  const perf = useRun<PerfData>("/mobile/portfolio/perf");
  const xray = useRun<XrayData>("/mobile/portfolio/xray");
  const replay = useRun<ReplayData>("/mobile/portfolio/replay");
  const [thresholds, setThresholds] = useState(loadThresholds);
  const [editing, setEditing] = useState(false);
  const save = useMemo(() => (t: Thresholds) => { setThresholds(t); setEditing(false); try { window.localStorage.setItem(TH_KEY, JSON.stringify(t)); } catch { /* 可选 */ } }, []);
  return <>
    <section className="portfolio-analysis" aria-label="收益表现">
      <div className="pa-title"><h2><Activity size={14} /> 收益表现</h2>
        <button type="button" disabled={perf.loading} onClick={() => void perf.run()}>{perf.loading ? <RefreshCw size={13} className="spin" /> : <Play size={13} />}{perf.data ? "重新计算" : "开始计算"}</button></div>
      {!perf.data && !perf.loading && !perf.error && <p className="pa-note">逐只取日线计算,要几秒到几十秒,点「开始计算」才会联网取价。</p>}
      {perf.loading && !perf.data && <p className="pa-note">正在取价格并计算…</p>}
      {perf.error && <p className="intra-warn" role="alert">计算失败:{perf.error}</p>}
      {perf.data && <PerfView data={perf.data} />}
    </section>
    <section className="portfolio-analysis" aria-label="持仓体检">
      <div className="pa-title"><h2><Gauge size={14} /> 持仓体检</h2>
        <span><button type="button" onClick={() => setEditing(!editing)}>提醒线</button>
          <button type="button" disabled={xray.loading} onClick={() => void xray.run()}>{xray.loading ? <RefreshCw size={13} className="spin" /> : <Play size={13} />}{xray.data ? "重新体检" : "开始体检"}</button></span></div>
      {editing && <ThresholdEditor value={thresholds} onSave={save} />}
      {!xray.data && !xray.loading && !xray.error && <p className="pa-note">取在手持仓最近一年的日线和所属行业来算,点「开始体检」才会联网。</p>}
      {xray.loading && !xray.data && <p className="pa-note">正在取日线并计算…</p>}
      {xray.error && <p className="intra-warn" role="alert">体检失败:{xray.error}</p>}
      {xray.data && <XrayView data={xray.data} thresholds={thresholds} />}
    </section>
    <section className="portfolio-analysis" aria-label="历史回放">
      <div className="pa-title"><h2><History size={14} /> 历史回放</h2>
        <button type="button" disabled={replay.loading} onClick={() => void replay.run()}>{replay.loading ? <RefreshCw size={13} className="spin" /> : <Play size={13} />}{replay.data ? "重新回放" : "开始回放"}</button></div>
      {!replay.data && !replay.loading && !replay.error && <p className="pa-note">按现在的持仓比例,把 2015、2016、2018、2020、2021–2024 几段大跌期重放一遍,和同期沪深 300 并排。要逐只取多年日线,点「开始回放」才会联网。</p>}
      {replay.loading && !replay.data && <p className="pa-note">正在取多年日线并回放…</p>}
      {replay.error && <p className="intra-warn" role="alert">回放失败:{replay.error}</p>}
      {replay.data && <ReplayView data={replay.data} />}
    </section>
  </>;
}
