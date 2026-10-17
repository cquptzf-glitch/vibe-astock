/**
 * 复盘 · 风险与纪律(移动版,对应桌面版交易日志的「风险」页):权益曲线与回撤、近 10 / 20 / 50 笔状态漂移、纪律归因、
 * 规矩逐条检查、在险资金、按打法 / 是否按计划 / 持有周期分组,以及费率、规矩、账户规模设置。
 * 只对照用户自己写下的规矩和他自己的交易,不替他判断该不该交易;个人成交、持仓、金额只在本机,不进任何 AI,本页也不登记页面上下文。
 */
import { AlertTriangle, RefreshCw, Settings2, ShieldCheck, TrendingDown } from "lucide-react";
import { useState } from "react";

import { fmtNumber, fmtPercent, useDiscoveryData } from "./discoveryData";
import { WebSelect } from "./controls";
import { fmtRate } from "./emotionData";
import { isPrivate, priv, usePrivacyMode } from "./privacy";
import "./emotion.css";
import "./monitor.css";

type Maybe<T> = ({ available: true } & T) | { available: false; reason: string };
export interface Bucket { count: number; scored: number; money_scored: number; net_pnl: number | null; win_rate: number | null; avg: number | null; best: number | null; worst: number | null }
export interface EquityPoint { date: string; cum_pnl: number; pnl: number; drawdown: number }
export interface Equity {
  points: EquityPoint[]; date_note: string; trades: number; net_pnl: number; peak: number; peak_date: string | null; current_drawdown: number;
  max_drawdown: number; max_drawdown_since: string | null; trades_since_peak: number; longest_underwater: number;
  win_rate: number | null; avg_win: number | null; avg_loss: number | null; payoff_ratio: number | null; profit_factor: number | null;
  worst_losing_streak: number; worst_trade: number; net_without_best1: number; net_without_best3: number; best_trade_share: number | null;
}
export interface WindowStat { trades: number; net_pnl?: number; win_rate?: number | null; payoff_ratio?: number | null; profit_factor?: number | null; execution_rate?: number | null; date_from?: string; date_to?: string; window?: number; enough?: boolean }
export interface Rolling { windows: Record<string, WindowStat>; lifetime: WindowStat; win_rate_drift?: number; profit_factor_drift?: number; note: string }
interface DisciplinePart { count: number; win_rate: number | null; avg_pct: number | null; net_pnl: number | null }
export interface Discipline {
  planned: DisciplinePart; unplanned: DisciplinePart; untagged: DisciplinePart; execution_rate: number | null;
  what_if_only_planned: { actual_net: number | null; planned_only_net: number | null; cost_of_indiscipline: number | null }; note: string;
}
export interface Violation { date: string; rule: string; label: string; limit: number; actual: number; detail: string }
export interface Violations {
  rules: Record<string, number>; is_default_rules: boolean; rule_status: Record<string, string>; unchecked: string[]; violations: Violation[]; violation_count: number;
  after_loss_streak: { threshold: number; trades: number; avg_pct: number | null; win_rate: number | null };
}
export interface AtRiskPos { key: string; symbol: string; name: string; date: string; playbook: string | null; shares: number; avg_cost: number; capital: number; planned_stop: number | null; at_risk: number | null; at_risk_pct: number | null; bounded: boolean }
export interface AtRisk {
  positions: AtRiskPos[]; position_count: number; total_capital: number; total_at_risk: number; bounded_count: number; unbounded_count: number; unbounded_capital: number;
  equity_base: number | null; at_risk_of_equity_pct: number | null; capital_of_equity_pct: number | null; equity_base_hint: string | null;
  over_per_trade_limit: { name: string; code: string; pct_of_equity: number }[] | null; over_position_limit: { actual: number; limit: number } | null; unbounded_note: string | null;
}
export interface SettingsView {
  fees: Record<string, number>; fees_default: boolean; rules: Record<string, number>; rules_default: boolean; equity_base: number | null; cost_method?: string; cost_methods?: { value: string; label: string }[];
  labels: { fees: Record<string, string>; rules: Record<string, string> }; defaults: { fees: Record<string, number>; rules: Record<string, number> }; fraction_rules: string[]; note: string;
}
export interface JournalRiskData {
  available: true; settings: SettingsView; stats: { overall: Bucket; by_playbook: (Bucket & { key: string })[]; by_planned: (Bucket & { key: string })[]; by_hold: (Bucket & { key: string })[] };
  risk: { equity: Maybe<Equity>; rolling: Maybe<Rolling>; discipline: Maybe<Discipline>; violations: Maybe<Violations> }; at_risk: Maybe<AtRisk>; notes: string[];
}

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const money = (v: number | null | undefined) => (v == null ? "—" : priv((v > 0 ? "+" : "") + fmtNumber(v, 2)));
const plain = (v: number | null | undefined) => (v == null ? "—" : priv(fmtNumber(v, 2)));

function Card({ title, icon, children, aria }: { title: string; icon?: React.ReactNode; children: React.ReactNode; aria?: string }) {
  return <section className="watch-card" aria-label={aria ?? title}><div className="watch-card-head"><h2>{icon}{title}</h2></div>{children}</section>;
}
function Stat({ label, value, tone: t }: { label: string; value: React.ReactNode; tone?: string }) {
  return <div className="emo-stat"><span>{label}</span><strong className={t}>{value}</strong></div>;
}
function Unavailable({ reason }: { reason: string }) {
  return <p className="watch-empty" role="status">{reason}</p>;
}

// ------------------------------------------------------------ 权益曲线

export function EquityChart({ points }: { points: EquityPoint[] }) {
  if (points.length < 2) return null;
  // 隐私模式不画权益曲线:曲线的高低、起伏就是盈亏的大小
  if (isPrivate()) return <p className="watch-hint" role="status">隐私模式下不画权益曲线(曲线高低就是盈亏大小)。</p>;
  const w = 300, h = 110, pad = 6;
  const cum = points.map((p) => p.cum_pnl);
  const peaks: number[] = [];
  let peak = 0;
  for (const v of cum) { peak = Math.max(peak, v); peaks.push(peak); }
  const lo = Math.min(0, ...cum), hi = Math.max(0, ...peaks), span = hi - lo || 1;
  const x = (i: number) => pad + (i / (points.length - 1)) * (w - 2 * pad);
  const y = (v: number) => h - pad - ((v - lo) / span) * (h - 2 * pad);
  const line = (vals: number[]) => vals.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  return <svg className="jr-chart" viewBox={`0 0 ${w} ${h}`} role="img" aria-label={`权益曲线:${points.length} 笔,累计 ${money(cum[cum.length - 1])} 元`}>
    <line x1={pad} x2={w - pad} y1={y(0)} y2={y(0)} className="zero" />
    <polyline points={line(peaks)} className="peak" />
    <polyline points={line(cum)} className="cum" />
  </svg>;
}

function EquityCard({ eq }: { eq: Maybe<Equity> }) {
  if (!eq.available) return <Card title="权益曲线与回撤" icon={<TrendingDown size={15} />}><Unavailable reason={eq.reason} /></Card>;
  return <Card title="权益曲线与回撤" icon={<TrendingDown size={15} />}>
    <p className="watch-hint">{eq.trades} 笔已平仓、{eq.date_note}。累计盈亏逐笔相加,峰值虚线是历史最高点;没卖完的交易不算已实现。</p>
    <EquityChart points={eq.points} />
    <div className="emo-stats emo-stats-4">
      <Stat label="累计净盈亏" value={money(eq.net_pnl)} tone={tone(eq.net_pnl)} />
      <Stat label="峰值" value={`${plain(eq.peak)}${eq.peak_date ? ` · ${eq.peak_date.slice(5)}` : ""}`} />
      <Stat label="当前回撤" value={plain(eq.current_drawdown)} tone={eq.current_drawdown > 0 ? "down" : ""} />
      <Stat label="最大回撤" value={`${plain(eq.max_drawdown)}${eq.max_drawdown_since ? ` · 自 ${eq.max_drawdown_since.slice(5)}` : ""}`} tone={eq.max_drawdown > 0 ? "down" : ""} />
      <Stat label="未创新高" value={`${eq.trades_since_peak} 笔`} />
      <Stat label="最长水下" value={`${eq.longest_underwater} 笔`} />
      <Stat label="胜率(不含持平)" value={fmtRate(eq.win_rate)} />
      <Stat label="盈亏比" value={eq.payoff_ratio == null ? "—" : eq.payoff_ratio} />
      <Stat label="盈利因子" value={eq.profit_factor == null ? "—" : eq.profit_factor} />
      <Stat label="最长连亏" value={`${eq.worst_losing_streak} 笔`} />
      <Stat label="最差一笔" value={money(eq.worst_trade)} tone={tone(eq.worst_trade)} />
      <Stat label="平均盈 / 亏" value={`${money(eq.avg_win)} / ${money(eq.avg_loss)}`} />
    </div>
    <p className="smp-line"><span>去掉最好 1 笔后 <b className={tone(eq.net_without_best1)}>{money(eq.net_without_best1)}</b></span><span>去掉最好 3 笔后 <b className={tone(eq.net_without_best3)}>{money(eq.net_without_best3)}</b></span>
      <span>最好一笔占累计 <b>{eq.best_trade_share == null ? "—(累计不为正)" : fmtRate(eq.best_trade_share)}</b></span></p>
  </Card>;
}

// ------------------------------------------------------------ 近 10 / 20 / 50 笔

function RollingCard({ r }: { r: Maybe<Rolling> }) {
  if (!r.available) return <Card title="近 10 / 20 / 50 笔" icon={<TrendingDown size={15} />}><Unavailable reason={r.reason} /></Card>;
  const cols: [string, WindowStat][] = [["近 10 笔", r.windows["10"]!], ["近 20 笔", r.windows["20"]!], ["近 50 笔", r.windows["50"]!], ["终身", r.lifetime]];
  return <Card title="近 10 / 20 / 50 笔" icon={<TrendingDown size={15} />}>
    <p className="watch-hint">{r.note}</p>
    <div className="intra-table-wrap"><table className="intra-table" aria-label="滚动窗口">
      <thead><tr><th>窗口</th><th>笔数</th><th>净盈亏</th><th>胜率</th><th>盈亏比</th><th>盈利因子</th><th>按计划</th></tr></thead>
      <tbody>{cols.map(([name, w]) => {
        const short = w.enough === false;
        return <tr key={name} className={short ? "jr-short" : ""}>
          <td><b>{name}</b>{short && <small> 样本不足</small>}</td><td>{w.trades}</td><td className={tone(w.net_pnl)}>{money(w.net_pnl)}</td>
          <td>{fmtRate(w.win_rate)}</td><td>{w.payoff_ratio ?? "—"}</td><td>{w.profit_factor ?? "—"}</td><td>{fmtRate(w.execution_rate)}</td></tr>;
      })}</tbody>
    </table></div>
    {(r.win_rate_drift != null || r.profit_factor_drift != null) && <p className="smp-line">
      {r.win_rate_drift != null && <span>近 10 笔胜率比终身 <b className={tone(r.win_rate_drift)}>{r.win_rate_drift > 0 ? "+" : ""}{fmtNumber(r.win_rate_drift * 100, 1)} 个点</b></span>}
      {r.profit_factor_drift != null && <span>盈利因子比终身 <b className={tone(r.profit_factor_drift)}>{r.profit_factor_drift > 0 ? "+" : ""}{r.profit_factor_drift}</b></span>}</p>}
  </Card>;
}

// ------------------------------------------------------------ 纪律归因

function DisciplineCard({ d }: { d: Maybe<Discipline> }) {
  if (!d.available) return <Card title="纪律归因" icon={<ShieldCheck size={15} />}><Unavailable reason={d.reason} /></Card>;
  const rows: [string, DisciplinePart][] = [["按计划", d.planned], ["计划外", d.unplanned], ["未标注", d.untagged]];
  const w = d.what_if_only_planned;
  return <Card title="纪律归因" icon={<ShieldCheck size={15} />}>
    <p className="watch-hint">{d.note}</p>
    <div className="intra-table-wrap"><table className="intra-table" aria-label="纪律归因">
      <thead><tr><th>分组</th><th>笔数</th><th>胜率</th><th>平均 %</th><th>净盈亏</th></tr></thead>
      <tbody>{rows.map(([name, p]) => <tr key={name}><td><b>{name}</b></td><td>{p.count}</td><td>{fmtRate(p.win_rate)}</td><td className={tone(p.avg_pct)}>{fmtPercent(p.avg_pct)}</td><td className={tone(p.net_pnl)}>{money(p.net_pnl)}</td></tr>)}</tbody>
    </table></div>
    <p className="smp-line"><span>执行率 <b>{fmtRate(d.execution_rate)}</b></span><span>实际净盈亏 <b className={tone(w.actual_net)}>{money(w.actual_net)}</b></span>
      <span>只做按计划的 <b className={tone(w.planned_only_net)}>{money(w.planned_only_net)}</b></span><span>计划外的代价 <b className={tone(w.cost_of_indiscipline)}>{money(w.cost_of_indiscipline)}</b></span></p>
  </Card>;
}

// ------------------------------------------------------------ 规矩检查

const RULE_ORDER = ["max_loss_per_trade_pct", "max_loss_per_day_pct", "max_positions", "max_trades_per_day", "pause_after_losses", "max_unplanned_ratio"];

function ViolationsCard({ v, labels }: { v: Maybe<Violations>; labels: Record<string, string> }) {
  if (!v.available) return <Card title="规矩检查" icon={<AlertTriangle size={15} />}><Unavailable reason={v.reason} /></Card>;
  const a = v.after_loss_streak;
  return <Card title="规矩检查" icon={<AlertTriangle size={15} />}>
    <p className="watch-hint">{v.is_default_rules ? "现在用的是默认规矩(只是能跑起来的初值,不是推荐值),在下面「设置」里改成你自己的。" : "按你自己设的规矩逐条对照历史。"}只陈述历史,不下「该不该」的结论。</p>
    <ul className="jr-rules" aria-label="规矩查了没有">{RULE_ORDER.map((k) => {
      const status = v.rule_status[k] ?? "";
      const unchecked = status.startsWith("unavailable");
      return <li key={k} className={unchecked ? "no" : "yes"}><b>{labels[k] ?? k}</b><span>{v.rules[k]}</span><em>{unchecked ? `没查:${status.slice("unavailable:".length)}` : "已查"}</em></li>;
    })}</ul>
    {v.violations.length === 0 ? <p className="watch-empty">{v.unchecked.length ? "已查的规矩里没有违反(没查的见上面)。" : "所有规矩都查了,没有违反。"}</p>
      : <ul className="jr-violations" aria-label="违反记录">{v.violations.map((x, i) => <li key={`${x.date}-${x.rule}-${i}`}><time>{x.date}</time><b>{x.label}</b><span>{priv(x.detail)}</span><small>规矩 {priv(String(x.limit))} · 实际 {priv(String(x.actual))}</small></li>)}</ul>}
    <p className="smp-line"><span>连亏 {a.threshold} 笔之后又做的 <b>{a.trades}</b> 笔</span><span>平均 <b className={tone(a.avg_pct)}>{fmtPercent(a.avg_pct)}</b></span><span>胜率 <b>{fmtRate(a.win_rate)}</b></span></p>
  </Card>;
}

// ------------------------------------------------------------ 在险资金

function AtRiskCard({ r }: { r: Maybe<AtRisk> }) {
  if (!r.available) return <Card title="在险资金" icon={<AlertTriangle size={15} />}><Unavailable reason={r.reason} /></Card>;
  return <Card title="在险资金" icon={<AlertTriangle size={15} />}>
    <p className="watch-hint">没卖完的 {r.position_count} 笔:占用本金 {plain(r.total_capital)} 元;写了计划止损的 {r.bounded_count} 笔合计在险 {plain(r.total_at_risk)} 元(止损高于成本算 0,不算负数)。</p>
    <div className="emo-stats emo-stats-4">
      <Stat label="总在险(下限)" value={plain(r.total_at_risk)} /><Stat label="占账户" value={r.at_risk_of_equity_pct == null ? "—" : `${r.at_risk_of_equity_pct}%`} />
      <Stat label="本金占账户" value={r.capital_of_equity_pct == null ? "—" : `${r.capital_of_equity_pct}%`} /><Stat label="没写止损" value={`${r.unbounded_count} 笔`} />
    </div>
    {r.equity_base_hint && <p className="watch-hint">{r.equity_base_hint}</p>}
    {r.unbounded_note && <p className="intra-warn">{r.unbounded_note}</p>}
    {r.over_position_limit && <p className="intra-warn">同时持有 {r.over_position_limit.actual} 只,超过你的上限 {r.over_position_limit.limit} 只。</p>}
    {r.over_per_trade_limit && r.over_per_trade_limit.length > 0 && <p className="intra-warn">在险超过单笔上限:{r.over_per_trade_limit.map((p) => `${p.name || p.code} ${p.pct_of_equity}%`).join("、")}</p>}
    <div className="intra-table-wrap"><table className="intra-table" aria-label="在险明细">
      <thead><tr><th>名称</th><th>买入日</th><th>本金</th><th>止损</th><th>在险</th><th>%</th></tr></thead>
      <tbody>{r.positions.map((p) => <tr key={p.key}><td><b>{p.name || p.symbol}</b></td><td>{p.date.slice(5)}</td><td>{plain(p.capital)}</td><td>{plain(p.planned_stop)}</td>
        <td>{p.bounded ? plain(p.at_risk) : "未知"}</td><td>{p.at_risk_pct == null ? "—" : `${p.at_risk_pct}%`}</td></tr>)}</tbody>
    </table></div>
  </Card>;
}

// ------------------------------------------------------------ 分组统计

function GroupsCard({ stats }: { stats: JournalRiskData["stats"] }) {
  const table = (title: string, rows: (Bucket & { key: string })[]) => <>
    <h3 className="watch-sub">{title}</h3>
    <div className="intra-table-wrap"><table className="intra-table" aria-label={title}>
      <thead><tr><th>分组</th><th>笔数</th><th>胜率</th><th>平均 %</th><th>最好 / 最差</th><th>净盈亏</th></tr></thead>
      <tbody>{rows.map((g) => <tr key={g.key}><td><b>{g.key}</b></td><td>{g.count}</td><td>{fmtRate(g.win_rate)}</td><td className={tone(g.avg)}>{fmtPercent(g.avg)}</td>
        <td>{fmtPercent(g.best)} / {fmtPercent(g.worst)}</td><td className={tone(g.net_pnl)}>{money(g.net_pnl)}</td></tr>)}</tbody>
    </table></div></>;
  return <Card title="分组统计">
    <p className="watch-hint">对你自己历史行为的统计,不是对市场的预测。样本少时读数没有意义,每组都带笔数;胜率不算持平。</p>
    {table("按打法", stats.by_playbook)}{table("按是否按计划", stats.by_planned)}{table("按持有周期", stats.by_hold)}
  </Card>;
}

// ------------------------------------------------------------ 设置

type Draft = { fees: Record<string, string>; rules: Record<string, string>; equity: string; costMethod: string };
const toDraft = (s: SettingsView): Draft => ({
  fees: Object.fromEntries(Object.entries(s.fees).map(([k, v]) => [k, String(v)])),
  rules: Object.fromEntries(Object.entries(s.rules).map(([k, v]) => [k, String(v)])),
  equity: s.equity_base == null ? "" : String(s.equity_base),
  costMethod: s.cost_method ?? "average",
});

export function SettingsCard({ s, onSaved }: { s: SettingsView; onSaved: () => void }) {
  const pwType = usePrivacyMode().on ? "password" : "text";
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<Draft>(() => toDraft(s));
  const [msg, setMsg] = useState("");
  const [saving, setSaving] = useState(false);
  const post = async (body: unknown, done: string) => {
    setSaving(true);
    setMsg("");
    try {
      const response = await fetch("/api/mobile/journal/settings", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const data = await response.json().catch(() => null) as { message?: string; error?: string } | null;
      if (!response.ok) setMsg(data?.message ?? data?.error ?? `HTTP ${response.status}`);
      else { setMsg(done); onSaved(); }
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };
  const num = (text: string) => (text.trim() === "" ? null : Number(text));
  const save = () => void post({
    fees: Object.fromEntries(Object.keys(s.labels.fees).map((k) => [k, num(draft.fees[k] ?? "")])),
    rules: Object.fromEntries(Object.keys(s.labels.rules).map((k) => [k, num(draft.rules[k] ?? "")])),
    equity_base: num(draft.equity),
    cost_method: draft.costMethod,
  }, "已保存");
  return <section className="watch-card" aria-label="设置">
    <div className="watch-card-head"><h2><Settings2 size={15} />费率、规矩与账户规模</h2>
      <button type="button" className="jr-toggle" aria-expanded={open} onClick={() => { setOpen(!open); setDraft(toDraft(s)); setMsg(""); }}>{open ? "收起" : "修改"}</button></div>
    <p className="watch-hint">费率{s.fees_default ? "用默认值" : "是你填的"}、规矩{s.rules_default ? "用默认值" : "是你填的"}、账户规模{s.equity_base == null ? "没填(单日亏损占比算不了)" : `${plain(s.equity_base)} 元`}、成本法{(s.cost_methods ?? []).find((m) => m.value === (s.cost_method ?? "average"))?.label ?? "移动加权平均"}。{s.note}</p>
    {open && <div className="jr-form">
      <h3 className="watch-sub">费率(没填实际费用的成交按这个估)</h3>
      {Object.entries(s.labels.fees).map(([k, label]) => <label key={k}><span>{label}</span><input type={pwType} autoComplete="off" inputMode="decimal" value={draft.fees[k] ?? ""} onChange={(e) => setDraft({ ...draft, fees: { ...draft.fees, [k]: e.target.value } })} aria-label={label} /></label>)}
      <h3 className="watch-sub">风险规矩(你自己的,不是推荐值)</h3>
      {Object.entries(s.labels.rules).map(([k, label]) => <label key={k}><span>{label}{s.fraction_rules.includes(k) ? (k.endsWith("_pct") ? "(%)" : "(0–1)") : ""}</span><input inputMode="decimal" value={draft.rules[k] ?? ""} onChange={(e) => setDraft({ ...draft, rules: { ...draft.rules, [k]: e.target.value } })} aria-label={label} /></label>)}
      <h3 className="watch-sub">成本法</h3>
      <label><span>卖出怎么结转成本</span><WebSelect value={draft.costMethod} onChange={(e) => setDraft({ ...draft, costMethod: e.target.value })} aria-label="成本法">{(s.cost_methods ?? [{ value: "average", label: "移动加权平均" }, { value: "fifo", label: "先进先出" }]).map((m) => <option key={m.value} value={m.value}>{m.label}</option>)}</WebSelect></label>
      <p className="watch-hint">只改已实现盈亏和剩余成本的算法,不改你记的成交。先进先出:卖出从最早买入的那笔开始扣,买入费用跟着那笔走;移动加权平均:按卖出那一刻的均价扣。总盈亏清仓后两种一样,差别在分批卖出的逐日盈亏和没清仓时的剩余成本。</p>
      <h3 className="watch-sub">账户规模</h3>
      <label><span>账户规模(元,选填)</span><input type={pwType} autoComplete="off" inputMode="decimal" value={draft.equity} placeholder="不填 = 不查占比" onChange={(e) => setDraft({ ...draft, equity: e.target.value })} aria-label="账户规模" /></label>
      <div className="jr-actions">
        <button type="button" className="discovery-action" disabled={saving} onClick={save}>保存</button>
        <button type="button" disabled={saving} onClick={() => void post({ fees: null, rules: Object.fromEntries(Object.keys(s.labels.rules).map((k) => [k, null])), equity_base: null, cost_method: null }, "已恢复默认")}>全部恢复默认</button>
      </div>
      <p className="watch-hint">费率四项要么全填要么全清空;填错会被拒绝并说明原因,不会改动已有设置。设置会进备份。</p>
    </div>}
    {msg && <p className="watch-hint" role="status">{msg}</p>}
  </section>;
}

export function JournalRiskView({ data, onSaved }: { data: JournalRiskData; onSaved: () => void }) {
  return <div className="watch-live">
    <EquityCard eq={data.risk.equity} />
    <RollingCard r={data.risk.rolling} />
    <DisciplineCard d={data.risk.discipline} />
    <ViolationsCard v={data.risk.violations} labels={data.settings.labels.rules} />
    <AtRiskCard r={data.at_risk} />
    <GroupsCard stats={data.stats} />
    <SettingsCard s={data.settings} onSaved={onSaved} />
    <ul className="watch-notes" aria-label="口径说明">{data.notes.map((n) => <li key={n}>{n}</li>)}</ul>
  </div>;
}

export function JournalRiskPanel() {
  const { data, loading, error, refresh } = useDiscoveryData<JournalRiskData>("/mobile/journal/risk");
  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><ShieldCheck size={15} /> 交易日志</div>
        <h1>风险与纪律</h1>
        <p>权益曲线与回撤、近 N 笔状态、纪律归因、规矩检查与在险资金。只对照你自己的规矩;成交、持仓与金额只在本机,不进 AI。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={refresh} disabled={loading} aria-label="刷新风险与纪律"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
    </div>
    {error && <div className="monitor-error" role="alert">风险与纪律这次没算出来:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {!data && !error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在读取本机交易日志…</div>}
    {data && <JournalRiskView data={data} onSaved={refresh} />}
  </section>;
}
