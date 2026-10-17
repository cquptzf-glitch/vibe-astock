/**
 * 盯盘 · 竞价与盘中路径(移动版,对应桌面版 shortline/IntradayTab.tsx):
 * 昨日涨停股 09:25 竞价强弱、全天 6 个时点的情绪路径、昨晚验证条件的盘中读数。
 * 快照由 Rust Core 在时点窗口里存(App 开着盯盘或本页时每分钟检查一次),过了时点补不回来;休市和窗口外不冒充当场快照。
 * 路径柱按红涨绿跌上色;盘中读数只是临时对照,成不成立以收盘为准。
 */
import { Camera, CalendarDays, ClipboardCheck, Gavel, RefreshCw, Route } from "lucide-react";
import { useState } from "react";

import { fmtNumber, fmtPercent, useDiscoveryData } from "./discoveryData";
import { chinaToday, fmtRate } from "./emotionData";
import "./emotion.css";
import "./monitor.css";
import { usePresentation } from "./presentation";
import { useForegroundRefresh } from "./useForegroundRefresh";

export type Slot = "09:25" | "09:35" | "10:00" | "11:30" | "14:00" | "15:00";
export type SlotState = "saved" | "failed" | "skipped" | "missing" | "upcoming";
export interface TierStat { sample: number; avg: number; median: number; up_rate: number }
export interface TopBoard { code: string; name: string; boards: number; sector: string; pct: number | null; at_limit: boolean | null }
export interface Cohort {
  expected: number; sample: number; coverage_rate: number; avg: number; median: number; up_count: number; down_count: number; up_rate: number; deep_loss_5: number;
  by_tier: Record<string, TierStat>; top_board_level: number; top_boards: TopBoard[];
}
export interface LiveCounts {
  limit_up: number | null; broken: number | null; limit_down: number | null; highest_board: number | null; consec_count: number | null;
  seal_rate: number | null; broken_rate: number | null; promotion_1to2: number | null; promotion_all: number | null;
}
export interface Snapshot {
  date: string; slot: Slot; captured_at: string; clock: string; drift_minutes: number; basis: "open" | "latest"; prev_date: string;
  cohort: Cohort; live: LiveCounts | null; live_reason?: string; gaps: string[];
}
export interface SlotRow { slot: Slot; window_end: string; state: SlotState; reason: string | null; snapshot: Snapshot | null }
export interface CheckPoint { slot: Slot; value: number | null; actual: string | null; actual_label: string | null; verified: boolean | null }
export interface CheckItem {
  id: string; metric: string; label: string; unit: string; kind: "count" | "rate" | "pct"; eps: number;
  direction: string; direction_label: string; reason: string; base_value: number | null; close_only: boolean; points: CheckPoint[];
}
export interface IntradayData {
  date: string; today: boolean; trading_day: boolean | null; slots: SlotRow[];
  auction: Snapshot | null; auction_reason: string | null;
  checks: { set_date: string | null; items: CheckItem[] };
  warnings: string[]; gaps: string[]; basis_labels: { open: string; latest: string }; source: string; generated_at: string;
}

const STATE_TEXT: Record<SlotState, string> = { saved: "已存", failed: "没存成", skipped: "休市", missing: "没存到", upcoming: "待存" };
const VERDICT = { yes: "暂成立", no: "暂不成立", none: "判不了" } as const;
const tone = (value: number | null | undefined) => (value == null ? "" : value > 0 ? "up" : value < 0 ? "down" : "");
const count = (value: number | null | undefined, unit = "") => (value == null || !Number.isFinite(value) ? "—" : `${value}${unit}`);

/** 09:25 那张是开盘价口径(高开率);盘中各时点是最新价口径(红盘率)。标错是概念错误 */
export const upLabel = (slot: Slot) => (slot === "09:25" ? "高开" : "红盘");

/** 验证条件的读数:占比 → 百分比,涨跌 → 带符号百分比,家数 / 板数 → 整数加单位 */
export function reading(value: number | null, kind: CheckItem["kind"], unit: string): string {
  if (value == null || !Number.isFinite(value)) return "—";
  if (kind === "rate") return fmtRate(value);
  if (kind === "pct") return fmtPercent(value);
  return `${fmtNumber(value, 0)}${unit}`;
}

const verdictOf = (verified: boolean | null) => (verified === true ? VERDICT.yes : verified === false ? VERDICT.no : VERDICT.none);

function SlotStrip({ slots }: { slots: SlotRow[] }) {
  return <ol className="intra-slots" aria-label="今天各时点的快照">{slots.map((s) =>
    <li key={s.slot} className={`intra-slot ${s.state}`} title={s.reason ?? undefined}>
      <span><b>{s.slot}</b><em>{STATE_TEXT[s.state]}</em></span>
      <small>{s.state === "saved" && s.snapshot ? `${s.snapshot.clock} 存 · ${s.snapshot.cohort.sample}/${s.snapshot.cohort.expected} 只`
        : s.state === "upcoming" ? `${s.slot}–${s.window_end} 自动存` : s.reason ?? ""}</small>
    </li>)}</ol>;
}

function Auction({ snap, reason, basisLabel }: { snap: Snapshot | null; reason: string | null; basisLabel: string }) {
  if (!snap) {
    return <section className="watch-card" aria-label="竞价强弱"><div className="watch-card-head"><h2><Gavel size={15} />昨日涨停股 · 今日竞价强弱</h2></div>
      <p className="watch-empty" role="status">{reason ?? "09:25 竞价快照没有"}</p></section>;
  }
  const c = snap.cohort;
  const tiers = Object.entries(c.by_tier);
  return <section className="watch-card" aria-label="竞价强弱">
    <div className="watch-card-head"><h2><Gavel size={15} />竞价强弱</h2><small>{snap.clock} 存</small></div>
    <p className="watch-hint">样本是 {snap.prev_date} 收盘封住的 {c.expected} 只,取到竞价行情 {c.sample} 只。口径:{basisLabel}。</p>
    <div className="intra-big">
      <div><strong className={tone(c.median)}>{fmtPercent(c.median)}</strong><span>中位数(多数人的体感)</span></div>
      <div><strong className={tone(c.avg)}>{fmtPercent(c.avg)}</strong><span>均值</span></div>
      <div><strong>{fmtRate(c.up_rate)}</strong><span>高开占比({c.up_count}/{c.sample})</span></div>
      <div><strong className="down">{c.deep_loss_5}</strong><span>低开超 5%</span></div>
    </div>
    {c.coverage_rate < 0.8 && <p className="intra-warn">当时只取到 {fmtRate(c.coverage_rate)} 的样本,读数不代表全体。</p>}
    {tiers.length > 0 && <><h3 className="watch-sub">分昨日板位 —— 首板和高位板的竞价含义完全不同</h3>
      <div className="intra-tiers">{tiers.map(([name, t]) => <div key={name}>
        <b>{name} <small>({t.sample} 只)</small></b><strong className={tone(t.median)}>{fmtPercent(t.median)}</strong>
        <small>高开 {fmtRate(t.up_rate)} · 均 {fmtPercent(t.avg)}</small></div>)}</div></>}
    {c.top_boards.length > 0 && <><h3 className="watch-sub">最高标({c.top_board_level} 板)· 客观读数</h3>
      <div className="intra-tops">{c.top_boards.map((t) => <span key={t.code}><b>{t.name}</b> <small>{t.code}{t.sector ? ` · ${t.sector}` : ""}</small> <b className={tone(t.pct)}>{fmtPercent(t.pct)}</b></span>)}</div></>}
    {snap.live_reason && <p className="watch-hint">全市场涨停 / 炸板家数:{snap.live_reason}。</p>}
  </section>;
}

function PathCard({ slots, closed, today }: { slots: SlotRow[]; closed: boolean; today: boolean }) {
  const saved = slots.filter((s) => s.snapshot);
  const maxAbs = Math.max(1, ...saved.map((s) => Math.abs(s.snapshot!.cohort.median)));
  return <section className="watch-card" aria-label="今日情绪路径">
    <div className="watch-card-head"><h2><Route size={15} />今日情绪路径</h2></div>
    <p className="watch-hint">同样是收盘 -2%,「早上 +3% 一路杀下来」和「早上 -5% 修复上来」是完全不同的两天。上排是昨日涨停股的中位涨幅(承接强弱),下面是当时全市场的涨停 / 炸板家数。</p>
    {saved.length === 0
      ? <p className="watch-empty" role="status">{closed ? "这一天休市,没有盘中路径。" : today ? "这一天还没有存到时点快照。App 开着盯盘的话,会在 09:25 / 09:35 / 10:00 / 11:30 / 14:00 / 15:00 自动存。" : "这一天没存到时点快照(当时 App 没开着盯盘,或取数失败)。"}</p>
      : <>
        <div className="intra-bars" aria-hidden>{slots.map((s) => {
          const m = s.snapshot?.cohort.median;
          return <div key={s.slot}><span className={tone(m)}>{m == null ? "—" : fmtPercent(m)}</span>
            <i className={m == null ? "none" : m > 0 ? "pos" : m < 0 ? "neg" : "none"} style={{ height: `${m == null ? 4 : Math.max(4, (Math.abs(m) / maxAbs) * 56)}px` }} /><small>{s.slot}</small></div>;
        })}</div>
        <div className="intra-table-wrap"><table className="intra-table" aria-label="各时点读数">
          <thead><tr>{["时点", "存的时间", "中位", "均值", "红盘 / 高开", "跌超 5%", "样本", "涨停", "炸板", "跌停", "最高板", "封板率", "1进2"].map((h) => <th key={h}>{h}</th>)}</tr></thead>
          <tbody>{saved.map((s) => { const p = s.snapshot!, c = p.cohort, l = p.live; return <tr key={s.slot}>
            <td><b>{s.slot}</b></td><td>{p.clock}</td><td className={tone(c.median)}>{fmtPercent(c.median)}</td><td className={tone(c.avg)}>{fmtPercent(c.avg)}</td>
            <td>{upLabel(s.slot)} {fmtRate(c.up_rate)}</td><td className="down">{c.deep_loss_5}</td><td className={c.coverage_rate < 0.8 ? "warn" : ""}>{c.sample}/{c.expected}</td>
            <td className="up">{l ? count(l.limit_up) : "—"}</td><td>{l ? count(l.broken) : "—"}</td><td className="down">{l ? count(l.limit_down) : "—"}</td>
            <td>{l ? count(l.highest_board, "板") : "—"}</td><td>{l ? fmtRate(l.seal_rate) : "—"}</td><td>{l ? fmtRate(l.promotion_1to2) : "—"}</td></tr>; })}</tbody>
        </table></div>
        <p className="watch-hint">「红盘 / 高开」:09:25 按开盘价算是高开占比,之后各时点按当时最新价算是红盘占比。样本标黄 = 当时行情只回来不到八成。09:25 是集合竞价,还没有当天的涨停 / 炸板池,所以那一行全市场家数是「—」。</p>
      </>}
  </section>;
}

function ChecksCard({ checks, date, closed, today }: { checks: IntradayData["checks"]; date: string; closed: boolean; today: boolean }) {
  const empty = closed ? `${date} 休市,没有盘中读数。`
    : checks.set_date ? `${checks.set_date} 晚上没写验证条件。在复盘的「明日验证」里写。`
    : today ? "还没存到时点快照,存下第一张后才有读数可对。条件在复盘的「明日验证」里写。"
    : "这一天没存到时点快照,没有盘中读数可对。";
  return <section className="watch-card" aria-label="验证条件盘中读数">
    <div className="watch-card-head"><h2><ClipboardCheck size={15} />昨晚的验证条件 · 盘中读数</h2></div>
    <p className="watch-hint">{checks.set_date ?? "上一个交易日"} 晚上在「明日验证」里写的条件,拿 {date} 各时点的读数先对一对。盘中读数只是临时对照,成不成立以收盘为准;只在收盘后才有的读数如实标出来,不硬凑。</p>
    {checks.items.length === 0 ? <p className="watch-empty">{empty}</p>
      : <div className="intra-checks">{checks.items.map((it) => <div key={it.id} className="intra-check">
        <div className="intra-check-head"><b>{it.label}</b><em>预期{it.direction_label}</em><span>昨天 {reading(it.base_value, it.kind, it.unit)}</span>{it.reason && <small>· {it.reason}</small>}</div>
        {it.close_only ? <p className="watch-hint">这项只在收盘后才有读数,盘中不判断。</p>
          : <div className="intra-points">{it.points.map((p) => <span key={p.slot}><b>{p.slot}</b>{reading(p.value, it.kind, it.unit)}{p.actual_label && <small>{p.actual_label}</small>}
            <em className={p.verified === true ? "yes" : ""}>{verdictOf(p.verified)}</em></span>)}</div>}
      </div>)}</div>}
  </section>;
}

export function IntradayView({ r, onCapture, capturing, captureMsg }: { r: IntradayData; onCapture: (() => void) | null; capturing: boolean; captureMsg: string | null }) {
  const closed = r.trading_day === false;
  return <div className="watch-live">
    <p className="watch-hint">{r.date}{r.today ? "(今天)" : ""} · {closed ? "休市,不存快照" : r.today ? "App 开着盯盘时在每个时点的窗口里自动存一张,过了时点补不回来" : "当天存下的时点快照,没存的补不回来"}</p>
    {onCapture && !closed && <button type="button" className="intra-capture" onClick={onCapture} disabled={capturing}><Camera size={14} className={capturing ? "spin" : ""} />现在存一张</button>}
    {captureMsg && <p className="watch-hint" role="status">{captureMsg}</p>}
    <SlotStrip slots={r.slots} />
    <Auction snap={r.auction} reason={r.auction_reason} basisLabel={r.basis_labels.open} />
    <PathCard slots={r.slots} closed={closed} today={r.today} />
    <ChecksCard checks={r.checks} date={r.date} closed={closed} today={r.today} />
    {r.warnings.map((w) => <p key={w} className="intra-warn" role="status">⚠️ {w}</p>)}
    {r.gaps.length > 0 && <ul className="watch-notes" aria-label="缺口说明">{r.gaps.map((g) => <li key={g}>{g}</li>)}</ul>}
    <p className="monitor-note">数据源:{r.source}。取不到的显示「—」,不当成 0。</p>
  </div>;
}

export function IntradayPanel() {
  const today = chinaToday();
  const [date, setDate] = useState("");
  const { refreshSeconds } = usePresentation();
  const { data, loading, error, refresh } = useDiscoveryData<IntradayData>(`/mobile/intraday${date ? `?date=${date}` : ""}`);
  const [capturing, setCapturing] = useState(false);
  const [captureMsg, setCaptureMsg] = useState<string | null>(null);
  // 今天的页面开着时每分钟重读一次:时点到了会自动存;设置里选了「仅手动刷新」就不自动读
  useForegroundRefresh(async () => { if (!date || date === today) refresh(); }, refreshSeconds === 0 ? 0 : 60);

  const capture = async () => {
    setCapturing(true);
    setCaptureMsg(null);
    try {
      const response = await fetch("/api/mobile/intraday", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ action: "capture" }) });
      const body = await response.json().catch(() => null) as { slot?: string; ok?: boolean; already?: boolean; reason?: string | null; message?: string } | null;
      if (!response.ok || !body) setCaptureMsg(body?.message ?? `HTTP ${response.status}`);
      else setCaptureMsg(body.already ? `${body.slot} 这张已经存过了。` : body.ok ? `${body.slot} 存好了。` : `${body.slot} 没存成:${body.reason ?? "原因不明"}`);
      refresh();
    } catch (e) {
      setCaptureMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setCapturing(false);
    }
  };

  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><Route size={15} /> 盯盘</div>
        <h1>竞价与盘中</h1>
        <p>昨日涨停股的竞价强弱、全天 6 个时点的情绪路径,以及昨晚验证条件的盘中读数。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={refresh} disabled={loading} aria-label="刷新竞价与盘中"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
    </div>
    <div className="emo-date-row">
      <label><CalendarDays size={15} /><span className="sr-only">日期</span>
        <input type="date" aria-label="快照日期" max={today} value={date || today} onChange={(event) => setDate(event.target.value === today ? "" : event.target.value)} /></label>
      <button type="button" className={!date ? "active" : ""} onClick={() => setDate("")}>今天</button>
    </div>
    {error && <div className="monitor-error" role="alert">竞价与盘中这次没取到:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {!data && !error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在读取时点快照…</div>}
    {data && <IntradayView r={data} onCapture={data.today ? () => void capture() : null} capturing={capturing} captureMsg={captureMsg} />}
  </section>;
}
