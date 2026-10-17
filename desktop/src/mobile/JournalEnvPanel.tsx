/**
 * 复盘 · 入场环境(移动版,对应桌面版交易日志「按情绪环境」「按当时板位」两组):把每笔交易按入场那天的市场环境分组。
 * 情绪强中弱是你交易过的这些日子之间的相对比较;当时板位看入场那天收盘时这只票在不在涨停池、炸板池里。
 * 取不到的日子如实列出缺口,不补、不当成「非涨停」;入场那天还没收盘的不分组。这是你自己的行为统计,不是对市场的预测。
 * 成交、持仓、金额只在本机,不进任何 AI,本页也不登记页面上下文。
 */
import { CloudSun, RefreshCw } from "lucide-react";

import { fmtNumber, fmtPercent, useDiscoveryData } from "./discoveryData";
import { fmtRate } from "./emotionData";
import { priv } from "./privacy";
import "./emotion.css";
import "./monitor.css";
import "./journal-analysis.css";

export interface EnvGroup { key: string; count: number; scored: number; money_scored: number; net_pnl: number | null; win_rate: number | null; avg: number | null; best: number | null; worst: number | null }
export interface EnvRow {
  key: string; date: string; regime: string | null; score: number | null; money_effect: number | null; board: string | null;
  boards: number | null; first_seal: string | null; broken_times: number | null; industry: string | null;
}
export interface JournalEnvData {
  available: boolean; reason?: string; rows?: EnvRow[]; by_regime?: EnvGroup[]; by_board?: EnvGroup[]; regime_cuts?: { low: number | null; high: number | null };
  scored_days?: number; unsettled_trades?: number; scanned?: number; days?: number; gaps?: string[]; note?: string; source?: string;
}

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const money = (v: number | null | undefined) => (v == null ? "—" : priv((v > 0 ? "+" : "") + fmtNumber(v, 2)));

function GroupTable({ title, rows }: { title: string; rows: EnvGroup[] }) {
  return <>
    <h3 className="watch-sub">{title}</h3>
    <div className="intra-table-wrap"><table className="intra-table" aria-label={title}>
      <thead><tr><th>分组</th><th>笔数</th><th>胜率</th><th>平均 %</th><th>最好 / 最差</th><th>净盈亏</th></tr></thead>
      <tbody>{rows.map((g) => <tr key={g.key}><td><b>{g.key}</b></td><td>{g.count}</td><td>{fmtRate(g.win_rate)}</td><td className={tone(g.avg)}>{fmtPercent(g.avg)}</td>
        <td>{fmtPercent(g.best)} / {fmtPercent(g.worst)}</td><td className={tone(g.net_pnl)}>{money(g.net_pnl)}</td></tr>)}</tbody>
    </table></div>
  </>;
}

export function JournalEnvView({ data }: { data: JournalEnvData }) {
  if (!data.available) return <p className="watch-empty" role="status">{data.reason}</p>;
  const cuts = data.regime_cuts;
  const gaps = data.gaps ?? [];
  const rows = (data.rows ?? []).filter((r) => r.regime !== null);
  return <div className="watch-live">
    <section className="watch-card" aria-label="按入场日情绪">
      <div className="watch-card-head"><h2><CloudSun size={15} />按入场日情绪与当时板位</h2></div>
      <p className="watch-hint">{data.note}</p>
      <p className="smp-line"><span>查了最近 <b>{data.scanned}</b> 笔、<b>{data.days}</b> 个入场日</span>
        {cuts?.low != null && cuts.high != null ? <span>情绪分三等分线 <b>{fmtNumber(cuts.low, 1)}</b> / <b>{fmtNumber(cuts.high, 1)}</b>(弱 ≤ 前者、强 ≥ 后者)</span> : <span>不分强中弱(情绪读数的交易日不够)</span>}</p>
      <GroupTable title="按入场日情绪(相对你交易过的日子)" rows={data.by_regime ?? []} />
      <GroupTable title="按当时板位" rows={data.by_board ?? []} />
    </section>
    {gaps.length > 0 && <section className="watch-card" aria-label="没取到的部分">
      <div className="watch-card-head"><h2>没取到的部分</h2></div>
      <ul className="jx-list">{gaps.map((g) => <li key={g}><span className="why warn">{g}</span></li>)}</ul>
      <p className="watch-hint">东财涨停 / 炸板池只留约一个月,更早的日子只读本机缓存(情绪历史和本页取过的收盘后池子);缺的日子不补、不当成「非涨停」。</p>
    </section>}
    {rows.length > 0 && <section className="watch-card" aria-label="逐笔环境">
      <div className="watch-card-head"><h2>逐笔环境</h2></div>
      <div className="intra-table-wrap"><table className="intra-table" aria-label="逐笔入场环境">
        <thead><tr><th>入场日</th><th>情绪</th><th>情绪分</th><th>板位</th><th>首封</th><th>行业</th></tr></thead>
        <tbody>{rows.map((r) => <tr key={r.key}><td>{r.date.slice(5)}</td><td>{r.regime}</td><td>{r.score == null ? "—" : fmtNumber(r.score, 1)}</td>
          <td>{r.board}{r.boards != null && r.boards > 1 ? ` ${r.boards}板` : ""}{r.board === "炸板" && r.broken_times != null ? ` · 炸 ${r.broken_times} 次` : ""}</td>
          <td>{r.first_seal ?? "—"}</td><td>{r.industry ?? "—"}</td></tr>)}</tbody>
      </table></div>
    </section>}
    <ul className="watch-notes" aria-label="口径说明">
      <li>数据来源:{data.source}</li>
      <li>成交、持仓与金额只留在本机,不进任何 AI;本页不登记页面上下文</li>
      <li>手机端只有清仓后才算已实现盈亏,没卖完的交易在分组里只有笔数</li>
    </ul>
  </div>;
}

export function JournalEnvPanel() {
  const { data, loading, error, refresh } = useDiscoveryData<JournalEnvData>("/mobile/journal/environment");
  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><CloudSun size={15} /> 交易日志</div>
        <h1>入场环境</h1>
        <p>你的交易在什么市场环境、什么板位下入场,各自的胜率与盈亏。成交与金额只在本机,不进 AI。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={refresh} disabled={loading} aria-label="刷新入场环境"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
    </div>
    {error && <div className="monitor-error" role="alert">入场环境这次没算出来:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {!data && !error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在对照入场日的涨停池与情绪读数,首次较慢…</div>}
    {data && <JournalEnvView data={data} />}
  </section>;
}
