/**
 * 复盘 · 看错了,还是没执行好(移动版,对应桌面版交易日志「复查」页的判断与执行归因):
 * 把「前一晚写给下一场的验证条件成立没成立」和「那天自己赚没赚」交叉成四格。
 * 看对了却亏钱和看错了才亏钱是两个病:一个要改执行,一个要改读盘;「判错还赚钱」单独点出来,不混进总体盈利。
 * 按入场日对齐;只有卖光了、有成交明细的交易才有金额;盈亏为 0 的日子不进任何一格。
 * 成交、持仓、金额只在本机,不进任何 AI,本页也不登记页面上下文。
 */
import { Scale, RefreshCw } from "lucide-react";

import { fmtNumber, useDiscoveryData } from "./discoveryData";
import { priv } from "./privacy";
import "./emotion.css";
import "./monitor.css";
import "./journal-analysis.css";

type QuadKey = "right_win" | "right_lose" | "wrong_win" | "wrong_lose";
export interface AttrDay { date: string; pnl: number; trades: number; unplanned: number; set_date: string; decided: number; hit: number; right: boolean }
export interface AttributionData {
  available: boolean; reason?: string; window?: { from: string | null; to: string | null }; gaps?: string[]; date?: string; source?: string; note?: string;
  days_counted?: number; enough_samples?: boolean; min_days?: number; quadrant_labels?: Record<QuadKey, string>;
  quadrants?: Record<QuadKey, { days: number; pnl: number; days_list: string[] }>; cells?: Record<QuadKey, AttrDay[]>;
  skipped_no_amount?: number; skipped_no_read?: number; skipped_undecided?: number;
}

const HINT = "把「前一晚写给下一场的验证条件成立没成立」和「那天自己赚没赚」交叉起来。看对了却亏钱和看错了才亏钱是两个病:一个要改执行,一个要改读盘。";
const CELLS: { key: QuadKey; tag?: string; luck?: boolean }[] = [
  { key: "right_win" }, { key: "right_lose", tag: "执行问题" }, { key: "wrong_win", tag: "运气", luck: true }, { key: "wrong_lose", tag: "判断问题" },
];
const tone = (v: number) => (v > 0 ? "up" : v < 0 ? "down" : "");
const money = (v: number) => priv((v > 0 ? "+" : "") + fmtNumber(v, 2));

export function AttributionView({ data }: { data: AttributionData }) {
  const skips = [
    data.skipped_no_read ? `${data.skipped_no_read} 笔那天前一晚没写验证条件(或还没对账),没算进来` : null,
    data.skipped_undecided ? `${data.skipped_undecided} 笔那天的条件一半成立一半不成立,不算判对也不算判错` : null,
    data.skipped_no_amount ? `${data.skipped_no_amount} 笔没有成交明细或还没卖光,算不出金额` : null,
  ].filter((x): x is string => x !== null);
  const notes = <>
    {skips.length > 0 && <ul className="jx-list" aria-label="没算进来的">{skips.map((s) => <li key={s}><span className="why">{s}</span></li>)}</ul>}
    {(data.gaps ?? []).length > 0 && <ul className="jx-list" aria-label="缺口">{(data.gaps ?? []).map((g) => <li key={g}><span className="why warn">{g}</span></li>)}</ul>}
  </>;
  if (!data.available) return <section className="watch-card" aria-label="判断与执行归因">
    <div className="watch-card-head"><h2><Scale size={15} />看错了,还是没执行好</h2></div>
    <p className="watch-hint">{HINT}</p><p className="watch-empty" role="status">{data.reason}</p>{notes}
  </section>;
  const labels = data.quadrant_labels!, q = data.quadrants!, cells = data.cells!;
  return <div className="watch-live">
    <section className="watch-card" aria-label="判断与执行归因">
      <div className="watch-card-head"><h2><Scale size={15} />看错了,还是没执行好</h2></div>
      <p className="watch-hint">{HINT} {data.note}</p>
      <div className="jx-quad" role="group" aria-label="四格">
        {CELLS.map((c) => <div key={c.key} className={c.luck && q[c.key].days > 0 ? "luck" : ""}>
          <span>{labels[c.key]}{c.tag && q[c.key].days > 0 ? ` · ${c.tag}` : ""}</span>
          <strong className={q[c.key].days > 0 ? tone(q[c.key].pnl) : ""}>{q[c.key].days} 天{q[c.key].days > 0 ? ` · ${money(q[c.key].pnl)} 元` : ""}</strong>
          {q[c.key].days > 0 && <small>{q[c.key].days_list.slice(-6).map((d) => d.slice(5)).join("、")}{q[c.key].days_list.length > 6 ? " 等" : ""}</small>}
        </div>)}
      </div>
      <p className="smp-line"><span>共 <b>{data.days_counted}</b> 个有盈亏的交易日({data.window?.from} 至 {data.window?.to})</span>
        {!data.enough_samples && <span className="intra-warn">不到 {data.min_days} 天,样本太少,别急着下结论</span>}</p>
      {notes}
    </section>
    {CELLS.filter((c) => cells[c.key].length > 0).map((c) => <section key={c.key} className="watch-card" aria-label={`${labels[c.key]} 明细`}>
      <div className="watch-card-head"><h2>{labels[c.key]}{c.tag ? ` · ${c.tag}` : ""}</h2></div>
      <div className="intra-table-wrap"><table className="intra-table" aria-label={`${labels[c.key]} 逐日`}>
        <thead><tr><th>入场日</th><th>笔数</th><th>计划外</th><th>前一晚条件</th><th>盈亏</th></tr></thead>
        <tbody>{cells[c.key].map((d) => <tr key={d.date}><td>{d.date.slice(5)}</td><td>{d.trades}</td><td>{d.unplanned}</td><td>{d.hit}/{d.decided} 成立</td><td className={tone(d.pnl)}>{money(d.pnl)}</td></tr>)}</tbody>
      </table></div>
    </section>)}
    <ul className="watch-notes" aria-label="口径说明">
      <li>数据来源:{data.source}</li>
      <li>成交、持仓与金额只留在本机,不进任何 AI;本页不登记页面上下文</li>
      <li>只有卖光了、有成交明细的交易才有金额;「判对没判对」只覆盖「明日验证」能对账的最近约 60 个交易日</li>
    </ul>
  </div>;
}

export function JournalAttributionPanel() {
  const { data, loading, error, refresh } = useDiscoveryData<AttributionData>("/mobile/journal/attribution");
  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><Scale size={15} /> 交易日志</div>
        <h1>看错了,还是没执行好</h1>
        <p>判断(前一晚的验证条件成不成立)和结果(那天赚没赚)交叉成四格。需要先在「明日验证」里写条件并对完账。成交与金额只在本机,不进 AI。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={refresh} disabled={loading} aria-label="刷新判断与执行归因"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
    </div>
    {error && <div className="monitor-error" role="alert">归因这次没算出来:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {!data && !error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在对照验证条件的对账结果…</div>}
    {data && <AttributionView data={data} />}
  </section>;
}
