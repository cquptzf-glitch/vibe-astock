/**
 * 复盘 · 问题记录(移动版,对应桌面版交易日志列出的「不参与计算的流水 / 挂不上的标注 / 不是 A 股的代码」):
 * 找出手机端交易记录里算不进统计的地方 —— 卖在买之前或卖超了、成交日期无效、没有成交的空记录、没填盈亏 % 的补记 —— 并让你逐笔改正或删掉。
 * 不是 A 股股票的记录(ETF、基金、债券)只列出来:交易日志只算 A 股股票,它们只计入持仓。
 * 修复只动你点的那一笔,改完不能比改之前更糟。成交与金额只在本机,不进任何 AI,本页也不登记页面上下文。
 */
import { RefreshCw, Wrench } from "lucide-react";
import { useState } from "react";

import { fmtNumber, useDiscoveryData } from "./discoveryData";
import { priv, usePrivacyMode } from "./privacy";
import "./emotion.css";
import "./monitor.css";
import "./journal-analysis.css";

export interface ProblemFill { id: number; type: "buy" | "sell"; date: string; price: number; shares: number; fee: number | null }
export interface Problem { kind: string; fill_id: number | null; why: string }
export interface ProblemEntry { entry_id: number; kind: "episode" | "manual"; symbol: string; name: string; date: string; pnl_pct: number | null; problems: Problem[]; fills: ProblemFill[] }
export interface ProblemsData {
  available: true; problem_count: number; counts: Record<string, number>; entries: ProblemEntry[];
  non_stock: { entry_id: number; symbol: string; name: string; date: string; fills: number }[];
  info: { entries: number; fills: number; fills_without_fee: number }; notes: string[];
}

const KIND_LABEL: Record<string, string> = { oversell: "卖出比持有多", bad_date: "成交日期无效", no_fills: "没有成交", manual_no_pnl: "补记没填盈亏", manual_bad_date: "补记日期无效" };

async function post(body: unknown): Promise<ProblemsData> {
  const response = await fetch("/api/mobile/journal/problems", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & { view?: ProblemsData }) | null;
  if (!response.ok || !data?.view) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data.view;
}

type Act = (body: unknown, done: string) => Promise<boolean>;

export function FillRow({ fill, flagged, act }: { fill: ProblemFill; flagged: Problem | undefined; act: Act }) {
  const [editing, setEditing] = useState(false);
  const [asking, setAsking] = useState(false);
  const pwType = usePrivacyMode().on ? "password" : "text";
  const [d, setD] = useState({ date: fill.date, price: String(fill.price), shares: String(fill.shares), fee: fill.fee == null ? "" : String(fill.fee) });
  const label = `${fill.type === "buy" ? "买入" : "卖出"} ${fill.date}`;
  return <li className={flagged ? "flagged" : ""}>
    <b>{fill.type === "buy" ? "买" : "卖"} {priv(fmtNumber(fill.shares, 2))} 股 @ {priv(fmtNumber(fill.price, 3))}</b><time>{fill.date}</time>
    <span className="meta">费用 {fill.fee == null ? "没填(按费率估)" : priv(fmtNumber(fill.fee, 2))}</span>
    <span className="jx-actions">
      <button type="button" aria-label={`改 ${label}`} onClick={() => setEditing(!editing)}>{editing ? "收起" : "改"}</button>
      {asking ? <><button type="button" className="discovery-action" aria-label={`确认删除 ${label}`} onClick={() => void act({ action: "delete_fill", fill_id: fill.id }, "已删除这笔成交。").then((ok) => { if (!ok) setAsking(false); })}>确认删除</button><button type="button" onClick={() => setAsking(false)}>不删了</button></>
        : <button type="button" aria-label={`删除 ${label}`} onClick={() => setAsking(true)}>删</button>}
    </span>
    {flagged && <span className="why warn">{flagged.why}</span>}
    {editing && <div className="jr-form jx-edit">
      <label><span>日期</span><input value={d.date} onChange={(e) => setD({ ...d, date: e.target.value })} aria-label="成交日期" /></label>
      <label><span>成交价</span><input type={pwType} autoComplete="off" inputMode="decimal" value={d.price} onChange={(e) => setD({ ...d, price: e.target.value })} aria-label="成交价" /></label>
      <label><span>股数</span><input type={pwType} autoComplete="off" inputMode="decimal" value={d.shares} onChange={(e) => setD({ ...d, shares: e.target.value })} aria-label="股数" /></label>
      <label><span>费用(空 = 按费率估)</span><input type={pwType} autoComplete="off" inputMode="decimal" value={d.fee} onChange={(e) => setD({ ...d, fee: e.target.value })} aria-label="费用" /></label>
      <div className="jr-actions"><button type="button" className="discovery-action" onClick={() => void act({ action: "update_fill", fill_id: fill.id, date: d.date, price: d.price, shares: d.shares, fee: d.fee === "" ? null : d.fee }, "已改正这笔成交。").then((ok) => { if (ok) setEditing(false); })}>保存这笔</button></div>
    </div>}
  </li>;
}

export function ProblemCard({ entry, act }: { entry: ProblemEntry; act: Act }) {
  const [asking, setAsking] = useState(false);
  const [pnl, setPnl] = useState("");
  const byFill = new Map(entry.problems.filter((p) => p.fill_id != null).map((p) => [p.fill_id as number, p]));
  const entryLevel = entry.problems.filter((p) => p.fill_id == null);
  return <section className="watch-card" aria-label={`问题记录 ${entry.name || entry.symbol}`}>
    <div className="watch-card-head"><h2><Wrench size={15} />{entry.name || entry.symbol}<small className="meta"> {entry.symbol} · {entry.kind === "manual" ? "补记" : "记录"} {entry.date.slice(5)}</small></h2></div>
    <ul className="jx-flags" aria-label="问题类型">{[...new Set(entry.problems.map((p) => p.kind))].map((k) => <li key={k}>{KIND_LABEL[k] ?? k}</li>)}</ul>
    {entryLevel.map((p) => <p key={p.kind} className="intra-warn">{p.why}</p>)}
    {entry.kind === "episode" && entry.fills.length > 0 && <ul className="jx-list jx-fills" aria-label="成交明细">{entry.fills.map((f) => <FillRow key={f.id} fill={f} flagged={byFill.get(f.id)} act={act} />)}</ul>}
    {entry.problems.some((p) => p.kind === "manual_no_pnl") && <div className="jr-form">
      <label><span>盈亏 %</span><input inputMode="decimal" value={pnl} placeholder="如 3.5" onChange={(e) => setPnl(e.target.value)} aria-label="补记盈亏 %" /></label>
      <div className="jr-actions"><button type="button" className="discovery-action" onClick={() => void act({ action: "set_manual_pnl", entry_id: entry.entry_id, pnl_pct: pnl }, "已补上盈亏 %。")}>补上盈亏 %</button></div>
    </div>}
    <div className="jr-actions">
      {asking ? <><button type="button" className="discovery-action" onClick={() => void act({ action: "delete_entry", entry_id: entry.entry_id }, `已删除「${entry.name || entry.symbol}」整条记录。`).then((ok) => { if (!ok) setAsking(false); })}>确认删除整条记录</button><button type="button" onClick={() => setAsking(false)}>不删了</button></>
        : <button type="button" onClick={() => setAsking(true)}>删除整条记录</button>}
    </div>
  </section>;
}

export function ProblemsView({ data, onChanged }: { data: ProblemsData; onChanged: (view: ProblemsData) => void }) {
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const act: Act = async (body, done) => {
    setMsg("");
    setErr("");
    try {
      onChanged(await post(body));
      setMsg(done);
      return true;
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      return false;
    }
  };
  return <div className="watch-live">
    <section className="watch-card" aria-label="问题概览">
      <div className="watch-card-head"><h2><Wrench size={15} />问题概览</h2></div>
      <p className="smp-line"><span>共 <b>{data.info.entries}</b> 条记录、<b>{data.info.fills}</b> 笔成交</span><span>需要处理 <b>{data.problem_count}</b> 处</span><span>没填费用的成交 <b>{data.info.fills_without_fee}</b> 笔(按费率估,不算问题)</span></p>
      {data.problem_count === 0 ? <p className="watch-empty" role="status">没有发现算不进统计的记录。</p> : <ul className="jx-flags" aria-label="各类问题">{Object.entries(data.counts).map(([k, n]) => <li key={k}>{KIND_LABEL[k] ?? k} {n}</li>)}</ul>}
      {msg && <p className="watch-hint" role="status">{msg}</p>}
      {err && <p className="intra-warn" role="alert">{err}</p>}
    </section>
    {data.entries.map((e) => <ProblemCard key={e.entry_id} entry={e} act={act} />)}
    {data.non_stock.length > 0 && <section className="watch-card" aria-label="不是 A 股股票的记录">
      <div className="watch-card-head"><h2>不是 A 股股票的记录</h2></div>
      <p className="watch-hint">交易日志只算 A 股股票(ETF、基金、债券、可转债、回购不算):下面这些只计入持仓,不进交易日志的任何统计。这不是错误,不用处理;要删到「交易日志」页删。</p>
      <ul className="jx-list" aria-label="非 A 股记录">{data.non_stock.map((n) => <li key={n.entry_id}><b>{n.name || n.symbol}<small className="meta"> {n.symbol}</small></b><time>{n.date.slice(5)}</time><span className="why">{n.fills} 笔成交</span></li>)}</ul>
    </section>}
    <ul className="watch-notes" aria-label="口径说明">{data.notes.map((n) => <li key={n}>{n}</li>)}</ul>
  </div>;
}

export function JournalProblemsPanel() {
  const { data, loading, error, refresh } = useDiscoveryData<ProblemsData>("/mobile/journal/problems");
  const [override, setOverride] = useState<ProblemsData | null>(null);
  const shown = override ?? data;
  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><Wrench size={15} /> 交易日志</div>
        <h1>问题记录</h1>
        <p>找出算不进统计的交易记录(卖超、日期无效、空记录、没填盈亏的补记),逐笔改正或删掉。成交与金额只在本机,不进 AI。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={() => { setOverride(null); refresh(); }} disabled={loading} aria-label="刷新问题记录"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
    </div>
    {error && <div className="monitor-error" role="alert">问题记录这次没读出来:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {!shown && !error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在检查本机交易记录…</div>}
    {shown && <ProblemsView data={shown} onChanged={setOverride} />}
  </section>;
}
