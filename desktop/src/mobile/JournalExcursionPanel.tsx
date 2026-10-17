/**
 * 复盘 · 复查(移动版,对应桌面版交易日志「复查」页的两块):复查清单 + 最大浮盈浮亏(MFE / MAE 与盈利回吐)。
 * 复查清单按客观条件筛出几笔值得回头看的交易,基准全部来自用户自己;只读已缓存的行情,不联网。
 * 最大浮盈浮亏要逐笔取不复权日线,用户点「取行情并计算」才联网,取到的永久存本机。
 * 日线看不出高低点发生在买入之前还是之后,偏差说明一直和读数摆在一起。
 * 成交、持仓、金额只在本机,不进任何 AI,本页也不登记页面上下文。
 */
import { ListChecks, LineChart, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";

import { fmtNumber, fmtPercent, useDiscoveryData } from "./discoveryData";
import { fmtRate } from "./emotionData";
import "./emotion.css";
import "./monitor.css";
import "./journal-analysis.css";

export interface InboxFlag { key: string; text: string }
export interface InboxItem { key: string; date: string; symbol: string; name: string; playbook: string | null; pnl_pct: number | null; note: string; closed: boolean; flags: InboxFlag[] }
export interface InboxData {
  available: boolean; reason?: string; count?: number; items?: InboxItem[]; scanned?: number; by_flag?: Record<string, number>;
  baseline?: { median_capital: number | null; median_hold_days: number | null; history_enough: boolean; min_history: number };
  rules_is_default?: boolean; excursion_available?: boolean; excursion_skipped?: number; excursion_hint?: string | null; note?: string;
}
export interface ExcItem {
  key: string; symbol: string; name: string; date: string; exit_date: string; realized_pct: number; mfe_pct: number; mae_pct: number;
  mfe_certain: number | null; mae_certain: number | null; certain_note: string | null; bars: number; bars_inner: number; same_day: boolean;
  precision: string; give_back_pct: number; capture_rate: number | null; capture_note: string | null;
}
export interface ExcursionData {
  available: boolean; reason?: string; scanned: number; fetch_online: boolean; cached: number;
  counts: { not_closed: number; split: number; bj: number; failed: number; uncached: number };
  failures: { key: string; symbol: string; name: string; date: string; why: string }[];
  intraday_note: string | null; not_counted_note: string | null; source: string;
  trades?: number; enough_samples?: boolean; median_capture_rate?: number | null; capture_samples?: number; median_give_back?: number | null; median_mae?: number | null;
  endured_count?: number; bad_entry_count?: number; same_day_count?: number; lost_with_move_count?: number; capture_note?: string | null;
  items?: ExcItem[]; bias_note?: string; caveat?: string;
}

const FLAG_LABEL: Record<string, string> = {
  over_loss_limit: "亏超自己的上限", oversized: "仓位偏大", held_long: "持有偏久", unplanned: "计划外", no_stop: "没写止损", deep_mae: "扛过大幅浮亏", low_capture: "没吃到行情",
};
const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");

function Card({ title, icon, children }: { title: string; icon?: React.ReactNode; children: React.ReactNode }) {
  return <section className="watch-card" aria-label={title}><div className="watch-card-head"><h2>{icon}{title}</h2></div>{children}</section>;
}
function Stat({ label, value, tone: t }: { label: string; value: React.ReactNode; tone?: string }) {
  return <div className="emo-stat"><span>{label}</span><strong className={t}>{value}</strong></div>;
}

export function InboxCard({ inbox }: { inbox: InboxData }) {
  if (!inbox.available) return <Card title="复查清单" icon={<ListChecks size={15} />}><p className="watch-empty" role="status">{inbox.reason}</p></Card>;
  const items = inbox.items ?? [];
  const byFlag = Object.entries(inbox.by_flag ?? {});
  return <Card title="复查清单" icon={<ListChecks size={15} />}>
    <p className="watch-hint">{inbox.note}</p>
    <p className="smp-line"><span>查了最近 <b>{inbox.scanned}</b> 笔</span><span>值得回头看 <b>{inbox.count}</b> 笔</span>
      {inbox.baseline?.history_enough ? <span>你的中位仓位 <b>{fmtNumber(inbox.baseline.median_capital, 0)}</b> 元、中位持有 <b>{inbox.baseline.median_hold_days}</b> 天</span>
        : <span>不到 {inbox.baseline?.min_history} 笔,不做「相对你自己习惯」的判定</span>}</p>
    {inbox.rules_is_default && <p className="watch-hint">单笔亏损上限用的是默认规矩(不是推荐值),可在「风险与纪律」里改成你自己的。</p>}
    {byFlag.length > 0 && <ul className="jx-flags" aria-label="各类笔数">{byFlag.map(([k, n]) => <li key={k}>{FLAG_LABEL[k] ?? k} {n}</li>)}</ul>}
    {inbox.excursion_hint && <p className="intra-warn">{inbox.excursion_hint}</p>}
    {items.length === 0 ? <p className="watch-empty">按这些条件筛,最近没有需要回头看的交易。</p>
      : <ul className="jx-list" aria-label="复查清单">{items.map((it) => <li key={it.key}>
        <b>{it.name || it.symbol}<small className="meta"> {it.symbol}{it.playbook ? ` · ${it.playbook}` : ""}{it.closed ? "" : " · 还在手上"}</small></b>
        <time>{it.date.slice(5)}</time>
        <span className={`meta ${tone(it.pnl_pct)}`}>{it.pnl_pct == null ? "盈亏未定" : fmtPercent(it.pnl_pct)}</span><span />
        <ul className="jx-flags">{it.flags.map((f) => <li key={f.key} title={FLAG_LABEL[f.key]}>{f.text}</li>)}</ul>
        {it.note && <span className="why">备注:{it.note}</span>}
      </li>)}</ul>}
  </Card>;
}

export function ExcursionCard({ data, busy, onFetch }: { data: ExcursionData; busy: boolean; onFetch: () => void }) {
  const c = data.counts;
  const action = <div className="jr-actions"><button type="button" className="discovery-action" disabled={busy} onClick={onFetch}><RefreshCw size={13} className={busy ? "spin" : ""} />{c.uncached > 0 || c.failed > 0 ? "取行情并计算" : "重新取没定稿的"}</button></div>;
  return <Card title="最大浮盈浮亏" icon={<LineChart size={15} />}>
    <p className="watch-hint">一笔的盈亏只说了终点,不说过程:最多曾赚到多少(最大浮盈)、最多曾亏到多少(最大浮亏)、实际落袋占最大浮盈的多少(捕获率)。只算卖光了的、有成交明细的沪深股票;用不复权日线对照你的成交成本。点「取行情并计算」才会联网,取到的永久存本机。</p>
    {action}
    {data.not_counted_note && <p className="watch-hint">没算进来的:{data.not_counted_note}。</p>}
    {data.intraday_note && <p className="intra-warn">{data.intraday_note}</p>}
    {!data.available ? <p className="watch-empty" role="status">{data.reason}</p> : <>
      {!data.enough_samples && <p className="intra-warn">只有 {data.trades} 笔,不到 6 笔,下面的中位数只当参考,不能说明习惯。</p>}
      <div className="emo-stats emo-stats-4">
        <Stat label="捕获率中位数" value={fmtRate(data.median_capture_rate)} tone={tone(data.median_capture_rate)} />
        <Stat label="盈利回吐中位数" value={data.median_give_back == null ? "—" : `${fmtNumber(data.median_give_back, 2)}%`} />
        <Stat label="最大浮亏中位数" value={data.median_mae == null ? "—" : `${fmtNumber(data.median_mae, 2)}%`} tone={tone(data.median_mae)} />
        <Stat label="算了几笔" value={`${data.trades} 笔`} />
        <Stat label="赚钱但扛过 ≥5% 浮亏" value={`${data.endured_count} 笔`} />
        <Stat label="亏钱且没怎么涨过" value={`${data.bad_entry_count} 笔`} />
        <Stat label="同日进出(只给上界)" value={`${data.same_day_count} 笔`} />
        <Stat label="涨过却亏着走" value={`${data.lost_with_move_count} 笔`} />
      </div>
      {data.capture_note && <p className="watch-hint">{data.capture_note}</p>}
      <p className="intra-warn">{data.bias_note}</p>
      <div className="intra-table-wrap"><table className="intra-table" aria-label="逐笔最大浮盈浮亏">
        <thead><tr><th>入场日</th><th>名称</th><th>落袋</th><th>最大浮盈</th><th>最大浮亏</th><th>捕获率</th></tr></thead>
        <tbody>{(data.items ?? []).map((it) => <tr key={it.key} title={it.precision}>
          <td>{it.date.slice(5)}{it.date !== it.exit_date ? `→${it.exit_date.slice(5)}` : ""}</td><td><b>{it.name || it.symbol}</b>{it.same_day && <small className="jm-change">同日进出</small>}</td>
          <td className={tone(it.realized_pct)}>{fmtPercent(it.realized_pct)}</td><td>{fmtPercent(it.mfe_pct)}{it.mfe_certain != null && <small className="jm-change">确定 {fmtPercent(it.mfe_certain)}</small>}</td>
          <td className={tone(it.mae_pct)}>{fmtPercent(it.mae_pct)}{it.mae_certain != null && <small className="jm-change">确定 {fmtPercent(it.mae_certain)}</small>}</td>
          <td>{it.capture_rate == null ? "—" : fmtRate(it.capture_rate)}{it.capture_note && <small className="jm-change">{it.capture_note}</small>}</td></tr>)}</tbody>
      </table></div>
      <p className="watch-hint">{data.caveat}</p>
    </>}
    {data.failures.length > 0 && <ul className="jx-list" aria-label="没取到行情的交易">{data.failures.map((f) => <li key={f.key}><b>{f.name || f.symbol}</b><time>{f.date.slice(5)}</time><span className="why warn">行情没取到:{f.why}</span></li>)}</ul>}
    <p className="watch-hint">数据来源:{data.source}。</p>
  </Card>;
}

export function JournalExcursionView({ inbox, excursion, busy, onFetch }: { inbox: InboxData; excursion: ExcursionData; busy: boolean; onFetch: () => void }) {
  return <div className="watch-live">
    <InboxCard inbox={inbox} />
    <ExcursionCard data={excursion} busy={busy} onFetch={onFetch} />
    <ul className="watch-notes" aria-label="口径说明">
      <li>成交、持仓与金额只留在本机,不进任何 AI;本页不登记页面上下文</li>
      <li>只有清仓后的交易才算最大浮盈浮亏;手机端没有送转流水,期间单日收盘跌超 21% 的整轮按疑似送转不算(只能识别大比例送转)</li>
      <li>北交所腾讯日 K 取不到,不算;数据只来自行情日线,不构成任何买卖建议</li>
    </ul>
  </div>;
}

export function JournalExcursionPanel() {
  const [mode, setMode] = useState<"cache" | "fetch">("cache");
  const inbox = useDiscoveryData<InboxData>("/mobile/journal/inbox");
  const exc = useDiscoveryData<ExcursionData>(`/mobile/journal/excursion?mode=${mode}`);
  const fetchedDone = mode === "fetch" && !exc.loading && !!exc.data;
  // 取完行情后复查清单里「没吃到行情 / 扛大幅浮亏」才有数据,刷新一次(只读缓存)
  useEffect(() => { if (fetchedDone) inbox.refresh(); }, [fetchedDone]); // eslint-disable-line react-hooks/exhaustive-deps
  const refresh = () => { inbox.refresh(); exc.refresh(); };
  const busy = exc.loading && mode === "fetch";
  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><ListChecks size={15} /> 交易日志</div>
        <h1>复查</h1>
        <p>复查清单与最大浮盈浮亏:按你自己的基准筛出值得回头看的交易,看每一笔最多赚到过、亏到过多少。成交与金额只在本机,不进 AI。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={refresh} disabled={inbox.loading || exc.loading} aria-label="刷新复查"><RefreshCw size={17} className={inbox.loading || exc.loading ? "spin" : ""} /></button>
    </div>
    {(inbox.error || exc.error) && <div className="monitor-error" role="alert">复查这次没算出来:{inbox.error || exc.error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {(!inbox.data || !exc.data) && !inbox.error && !exc.error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" />{busy ? " 正在逐笔取行情,首次较慢,取到的永久存本机…" : " 正在读取本机交易日志…"}</div>}
    {inbox.data && exc.data && <JournalExcursionView inbox={inbox.data} excursion={exc.data} busy={busy} onFetch={() => { if (mode === "fetch") exc.refresh(); else setMode("fetch"); }} />}
  </section>;
}
