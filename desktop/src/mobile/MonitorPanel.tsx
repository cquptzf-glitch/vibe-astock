/**
 * 盯盘 · 实时动态(移动版,对应桌面版 shortline/LiveTab.tsx):盘中异动流 + 持仓、自选、昨日梯队、大票、昨日成交额前十的当下读数。
 * 异动由 Rust Core 判断(3 分钟内 ±1.5% 记急拉 / 急跌,碰到 / 离开涨停价记封板 / 开板,同一只同一类 5 分钟不重复),
 * 页面开着才取;切到后台就停。只列客观读数,不排序打分、不给买卖倾向。持仓只显示现价、涨跌、浮动盈亏 % 和止损/目标状态。
 */
import { AlertTriangle, BarChart3, Building2, ChevronRight, Layers, Radio, RefreshCw, Star, Wallet, Zap } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { fmtNumber, fmtPercent, fmtYuan } from "./discoveryData";
import "./monitor.css";
import { usePresentation } from "./presentation";

export type AlertKind = "急拉" | "急跌" | "封板" | "开板";
export interface WatchAlert { ts: string; code: string; name: string; kind: AlertKind; change_pct: number | null; price: number | null; pct: number | null; msg: string; sources: string[] }
export interface WatchRow { code: string; name: string; price: number | null; pct: number | null; amount_yuan: number | null; quote_time: string | null; today: boolean; limit_up?: number | null; limit_down?: number | null }
export type RiskState = "stop_reached" | "target_reached" | "stop_near" | "target_near" | "stale_quote" | "unquoted" | "normal";
export interface HoldingRow extends WatchRow { pnl_pct: number | null; risk_state: RiskState }
export interface LadderRow extends WatchRow { boards: number; label: string; sector: string; at_limit: boolean | null }
export interface WatchView {
  date: string; clock: string; phase: string; phase_label: string;
  trading_day: boolean | null; detecting: boolean; poll_ms: number; quote_day: string | null;
  holdings: HoldingRow[]; watchlist: WatchRow[];
  ladder: { sample_date: string; quote_date: string | null; highest: number; rows: LadderRow[] } | null;
  turnover: { date: string; rows: (WatchRow & { rank: number; prev_amount_yuan: number })[] } | null;
  bigcap: { total: number; date: string | null; as_of: string; refreshed_at: string | null; top: WatchRow[]; bottom: WatchRow[] } | null;
  alerts: WatchAlert[];
  notes: string[];
  rules: { surge_pct: number; window_min: number; cooldown_min: number };
  source: string;
}

export const THRESHOLDS = [1.5, 3, 5];
export type Scope = "all" | "mine";
export type LadderFilter = 1 | 2 | 3;
export type WatchTab = "position" | "watch" | "ladder" | "bigcap" | "turnover";

/** 急拉急跌按幅度筛;封板开板没有幅度,一直留着。「持仓与自选」只看来源里带这两样的 */
export function filterAlerts(alerts: WatchAlert[], threshold: number, scope: Scope): WatchAlert[] {
  return alerts.filter((a) => {
    if (a.change_pct != null && Number.isFinite(a.change_pct) && Math.abs(a.change_pct) < threshold - 1e-9) return false;
    return scope === "all" || a.sources.includes("持仓") || a.sources.includes("自选");
  });
}

/** 昨日涨停的票今天怎么样:只数行情时间是今天的;封住与否说不准(竞价、午休、休市)时是 null,不当 0 */
export function ladderStats(rows: LadderRow[]) {
  const today = rows.filter((r) => r.today && r.pct != null && Number.isFinite(r.pct));
  return {
    total: rows.length, sample: today.length,
    up: today.filter((r) => r.pct! > 0).length,
    down: today.filter((r) => r.pct! < 0).length,
    sealed: rows.some((r) => r.at_limit !== null) ? rows.filter((r) => r.at_limit === true).length : null,
  };
}

const RISK_LABEL: Record<RiskState, string> = {
  stop_reached: "已到止损", target_reached: "已到目标", stop_near: "接近止损", target_near: "接近目标",
  stale_quote: "行情非今日", unquoted: "暂无行情", normal: "",
};
const RISKY: RiskState[] = ["stop_reached", "target_reached", "stop_near", "target_near"];
const KIND_CLASS: Record<AlertKind, string> = { 急拉: "surge", 急跌: "drop", 封板: "seal", 开板: "open" };
const tone = (value: number | null | undefined) => (value == null ? "" : value > 0 ? "up" : value < 0 ? "down" : "");

// ------------------------------------------------------------ 轮询

const MAX_BACKOFF_MS = 30_000;

/** 下一次隔多久:成功听后端(连续竞价 3 秒,竞价 / 午休 15 秒,其余 60 秒);失败按连续失败次数翻倍,上限 30 秒 */
export function nextDelay(okMs: number | null, failures: number): number {
  if (failures <= 0) return okMs && okMs > 0 ? okMs : 3_000;
  return Math.min(3_000 * 2 ** failures, MAX_BACKOFF_MS);
}

async function pollOnce(): Promise<WatchView> {
  const response = await fetch("/api/mobile/watch/poll", { method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" }, body: "{}" });
  const body = await response.json().catch(() => null) as (WatchView & { message?: string; error?: string }) | null;
  if (!response.ok || !body) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body;
}

/** 递归 setTimeout:上一次回来再等,请求不叠加;页面切到后台就停,切回来立刻问一次。manual = 只在打开和点刷新时取 */
function useWatchPoll(manual: boolean) {
  const [data, setData] = useState<WatchView | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [live, setLive] = useState(true);
  const kick = useRef<(() => void) | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let failures = 0;
    let lastMs: number | null = null;
    let inFlight = false;
    const clear = () => { if (timer !== null) { clearTimeout(timer); timer = null; } };
    const loop = async () => {
      if (cancelled || inFlight) return;
      clear();
      if (document.visibilityState === "hidden") { setLive(false); return; }
      setLive(true);
      inFlight = true;
      setLoading(true);
      try {
        const view = await pollOnce();
        if (cancelled) return;
        setData(view);
        setError("");
        failures = 0;
        lastMs = view.poll_ms;
      } catch (e) {
        if (cancelled) return;
        failures += 1;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        inFlight = false;
        if (!cancelled) setLoading(false);
      }
      if (!cancelled && !manual) timer = setTimeout(() => void loop(), nextDelay(lastMs, failures));
    };
    kick.current = () => void loop();
    void loop();
    const onVisible = () => {
      if (cancelled) return;
      if (document.visibilityState === "hidden") { clear(); setLive(false); }
      else if (!manual) void loop();
      else setLive(true);
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => { cancelled = true; clear(); kick.current = null; document.removeEventListener("visibilitychange", onVisible); };
  }, [manual]);

  const refresh = useCallback(() => kick.current?.(), []);
  return { data, error, loading, live, refresh };
}

// ------------------------------------------------------------ 展示件

function Price({ r }: { r: WatchRow }) {
  if (r.price == null) return <strong className="muted">没取到</strong>;
  return <strong>{fmtNumber(r.price, 3)}{!r.today && <small className="watch-stale" title={r.quote_time ? `行情时间 ${r.quote_time}` : undefined}>非今日</small>}</strong>;
}

function LimitTags({ r }: { r: WatchRow }) {
  if (!r.today || r.price == null) return null;
  if (r.limit_up != null && r.price >= r.limit_up - 1e-6) return <em className="watch-tag hot">涨停</em>;
  if (r.limit_down != null && r.price <= r.limit_down + 1e-6) return <em className="watch-tag cold">跌停</em>;
  return null;
}

function StockRow({ r, onOpen, lead, sub, extra }: { r: WatchRow; onOpen: (code: string) => void; lead?: ReactNode; sub?: ReactNode; extra?: ReactNode }) {
  return <button type="button" className="watch-row" onClick={() => onOpen(r.code)}>
    {lead != null && <span className="watch-lead">{lead}</span>}
    <span className="watch-name"><b>{r.name}</b><small>{r.code}{sub ? <> · {sub}</> : null}</small></span>
    <span className="watch-quote"><Price r={r} /><span className={tone(r.pct)}>{fmtPercent(r.pct)}</span></span>
    <span className="watch-extra">{extra}<ChevronRight size={14} /></span>
  </button>;
}

function Segmented<T extends string | number>({ label, value, options, onChange }: { label: string; value: T; options: readonly (readonly [T, string])[]; onChange: (value: T) => void }) {
  return <div className="watch-seg" role="group" aria-label={label}>{options.map(([key, text]) =>
    <button type="button" key={String(key)} aria-pressed={value === key} className={value === key ? "active" : ""} onClick={() => onChange(key)}>{text}</button>)}</div>;
}

function AlertFeed({ v, threshold, scope, onThreshold, onScope, onOpen }: {
  v: WatchView; threshold: number; scope: Scope; onThreshold: (n: number) => void; onScope: (s: Scope) => void; onOpen: (code: string) => void;
}) {
  const shown = filterAlerts(v.alerts, threshold, scope);
  return <section className="watch-card" aria-label="盘中异动">
    <div className="watch-card-head"><h2><Zap size={15} />盘中异动</h2><small>今天 {v.alerts.length} 条</small></div>
    <p className="watch-hint">急拉 / 急跌({v.rules.window_min} 分钟内 ±{v.rules.surge_pct}%)· 封板 / 开板 · 同一只同一类 {v.rules.cooldown_min} 分钟内不重复。客观行情事件,不是推荐也不是预测。</p>
    <div className="watch-filters">
      <Segmented label="急拉急跌的幅度" value={threshold} options={THRESHOLDS.map((t) => [t, `≥${t}%`] as const)} onChange={onThreshold} />
      <Segmented label="异动显示范围" value={scope} options={[["all", "全部"], ["mine", "持仓与自选"]] as const} onChange={onScope} />
    </div>
    {shown.length === 0
      ? <p className="watch-empty" role="status">{v.alerts.length ? `按当前筛选没有异动(今天共记下 ${v.alerts.length} 条)。`
        : v.detecting ? "还没有异动。急拉、急跌、封板、开板会实时出现在这里。"
        : `${v.phase_label},不判断异动${v.trading_day === false ? "" : ";连续竞价时段(9:30–11:30、13:00–14:57)页面开着就会盯"}。`}</p>
      : <ul className="watch-alerts" aria-label="异动列表">{shown.map((a, i) =>
        <li key={`${a.ts}-${a.code}-${a.kind}-${i}`}><button type="button" onClick={() => onOpen(a.code)}>
          <span className="watch-alert-top"><time>{a.ts}</time><em className={`watch-kind ${KIND_CLASS[a.kind]}`}>{a.kind}</em><b>{a.name}</b><small>{a.code}</small></span>
          <span className="watch-alert-msg">{a.msg}</span>
          {a.sources.length > 0 && <span className="watch-sources">{a.sources.map((s) => <i key={s}>{s}</i>)}</span>}
        </button></li>)}</ul>}
  </section>;
}

function LadderPanel({ v, ladderMin, onLadderMin, onOpen }: { v: WatchView; ladderMin: LadderFilter; onLadderMin: (n: LadderFilter) => void; onOpen: (code: string) => void }) {
  if (!v.ladder) return <p className="watch-empty">昨日梯队这次没取到,看页面下方的说明。</p>;
  const ls = ladderStats(v.ladder.rows);
  const rows = v.ladder.rows.filter((r) => r.boards >= ladderMin);
  return <>
    <p className="watch-hint">{v.ladder.sample_date} 收盘涨停 {ls.total} 只、最高 {v.ladder.highest} 板,今天走得怎么样。「封住」= 现价在涨停价(只在连续竞价和收盘后说)。</p>
    <div className="watch-ladder-stats" aria-label="昨日梯队统计">
      {ls.sample === 0
        ? <span>今天还没取到这 {ls.total} 只的行情{v.quote_day && v.quote_day !== v.date ? `,下面是 ${v.quote_day} 的读数(标「非今日」)` : ""}</span>
        : <>
          <span>今日行情 <b>{ls.sample}/{ls.total}</b></span>
          <span>上涨 <b className="up">{ls.up}</b></span>
          <span>下跌 <b className="down">{ls.down}</b></span>
          <span>封住 <b className={ls.sealed == null ? "" : "up"}>{ls.sealed ?? "—"}</b></span>
        </>}
    </div>
    <Segmented label="梯队按板位筛" value={ladderMin} options={[[1, "全部"], [2, "2板以上"], [3, "3板以上"]] as const} onChange={onLadderMin} />
    {rows.length === 0 ? <p className="watch-empty">这个板位以上没有票。</p>
      : <div className="watch-list" aria-label="昨日梯队">{rows.map((r) =>
        <StockRow key={r.code} r={r} onOpen={onOpen} lead={<em className="watch-board">{r.label}</em>} sub={r.sector || "—"}
          extra={r.at_limit ? <em className="watch-tag hot">封住</em> : null} />)}</div>}
  </>;
}

export function LiveWatchView({ v, tab, threshold, scope, ladderMin, onTab, onThreshold, onScope, onLadderMin, onOpenSymbol, error, paused = false, manual = false }: {
  v: WatchView; tab: WatchTab; threshold: number; scope: Scope; ladderMin: LadderFilter;
  onTab: (t: WatchTab) => void; onThreshold: (n: number) => void; onScope: (s: Scope) => void; onLadderMin: (n: LadderFilter) => void;
  onOpenSymbol: (code: string) => void; error: string; paused?: boolean; manual?: boolean;
}) {
  const detecting = v.detecting && !paused;
  const risky = v.holdings.filter((h) => RISKY.includes(h.risk_state)).length;
  const tabs: [WatchTab, string, number | null][] = [
    ["position", "持仓", v.holdings.length], ["watch", "自选", v.watchlist.length], ["ladder", "昨日梯队", v.ladder?.rows.length ?? null],
    ["bigcap", "大票", null], ["turnover", "成交额前十", null],
  ];
  return <div className="watch-live">
    <div className="watch-status">
      <span className={`watch-phase${detecting ? " on" : ""}`}><Radio size={13} />{detecting ? `盯盘中 · ${v.phase_label}` : v.phase_label}</span>
      <span>北京时间 <b>{v.clock}</b></span>
      <span>{paused ? "页面在后台,已暂停" : manual ? "仅手动刷新" : v.detecting ? "每 3 秒刷新" : `每 ${Math.round(v.poll_ms / 1000)} 秒刷新`}</span>
    </div>
    {manual && <p className="watch-hint">设置里选了「仅手动刷新」:异动要连续采样至少 1 分钟才判得出来,手动刷新时基本看不到急拉急跌。</p>}
    {error && <div className="monitor-error" role="alert">这次没取到:{error}。先显示上一次的内容,稍后自动重试。</div>}

    <AlertFeed v={v} threshold={threshold} scope={scope} onThreshold={onThreshold} onScope={onScope} onOpen={onOpenSymbol} />

    <div className="watch-tabs" role="tablist" aria-label="盯盘名单">{tabs.map(([key, label, count]) =>
      <button type="button" role="tab" key={key} aria-selected={tab === key} className={tab === key ? "active" : ""} onClick={() => onTab(key)}>
        {label}{count != null && <small>{count}</small>}</button>)}</div>

    <section className="watch-card" aria-label="名单读数">
      {tab === "position" && <>
        <p className="watch-hint"><Wallet size={13} /> 只列现价、今日涨跌、浮动盈亏 % 和止损/目标状态;股数和成本在「我的持仓」。</p>
        {risky > 0 && <div className="monitor-risk-summary"><AlertTriangle size={15} /><span>{risky} 个持仓到了止损/目标附近或已触发,打开个股核对。</span></div>}
        {v.holdings.length === 0 ? <p className="watch-empty">还没有 A 股持仓;成交记录会自动汇总到这里。</p>
          : <div className="watch-list" aria-label="持仓">{v.holdings.map((r) =>
            <StockRow key={r.code} r={r} onOpen={onOpenSymbol} sub={<>浮盈亏 <b className={tone(r.pnl_pct)}>{fmtPercent(r.pnl_pct)}</b></>}
              extra={<><LimitTags r={r} />{RISK_LABEL[r.risk_state] && <em className={`watch-tag${RISKY.includes(r.risk_state) ? " warn" : ""}`}>{RISK_LABEL[r.risk_state]}</em>}</>} />)}</div>}
      </>}
      {tab === "watch" && (v.watchlist.length === 0 ? <p className="watch-empty"><Star size={14} /> 自选里还没有 A 股,在个股页加入自选。</p>
        : <div className="watch-list" aria-label="自选">{v.watchlist.map((r) =>
          <StockRow key={r.code} r={r} onOpen={onOpenSymbol} sub={`成交 ${fmtYuan(r.amount_yuan)}`} extra={<LimitTags r={r} />} />)}</div>)}
      {tab === "ladder" && <LadderPanel v={v} ladderMin={ladderMin} onLadderMin={onLadderMin} onOpen={onOpenSymbol} />}
      {tab === "bigcap" && (!v.bigcap ? <p className="watch-empty"><Building2 size={14} /> 大票名单这次没取到。</p> : <>
        <p className="watch-hint">总市值 500 亿以上 {v.bigcap.total} 只(名单 {v.bigcap.as_of} 取),每 30 秒取一次行情。</p>
        <h3 className="watch-sub">涨幅前十</h3>
        {v.bigcap.top.length === 0 ? <p className="watch-empty">大票行情还没取到。</p>
          : <div className="watch-list" aria-label="大票涨幅前十">{v.bigcap.top.map((r) => <StockRow key={r.code} r={r} onOpen={onOpenSymbol} sub={`成交 ${fmtYuan(r.amount_yuan)}`} />)}</div>}
        {v.bigcap.bottom.length > 0 && <><h3 className="watch-sub">跌幅前五</h3>
          <div className="watch-list" aria-label="大票跌幅前五">{v.bigcap.bottom.map((r) => <StockRow key={r.code} r={r} onOpen={onOpenSymbol} sub={`成交 ${fmtYuan(r.amount_yuan)}`} />)}</div></>}
      </>)}
      {tab === "turnover" && (!v.turnover ? <p className="watch-empty"><BarChart3 size={14} /> 昨日成交额前十没有:原因见下方说明。</p> : <>
        <p className="watch-hint">{v.turnover.date} 成交额最大的十只,看今天的表现;「今成交」是今天的成交额。</p>
        <div className="watch-list" aria-label="昨日成交额前十">{v.turnover.rows.map((r) =>
          <StockRow key={r.code} r={r} onOpen={onOpenSymbol} lead={<em className="watch-rank">{r.rank}</em>} sub={`昨 ${fmtYuan(r.prev_amount_yuan)} · 今 ${fmtYuan(r.amount_yuan)}`} />)}</div>
      </>)}
    </section>

    {v.notes.length > 0 && <ul className="watch-notes" aria-label="缺口说明">{v.notes.map((n) => <li key={n}>{n}</li>)}</ul>}
    <p className="monitor-note"><Layers size={12} /> 数据源:{v.source}。取不到的显示「没取到」,不当成 0。</p>
  </div>;
}

export function MonitorPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const { refreshSeconds } = usePresentation();
  const manual = refreshSeconds === 0;
  const { data, error, loading, live, refresh } = useWatchPoll(manual);
  const [tab, setTab] = useState<WatchTab | null>(null);
  const [threshold, setThreshold] = useState(THRESHOLDS[0]!);
  const [scope, setScope] = useState<Scope>("all");
  const [ladderMin, setLadderMin] = useState<LadderFilter>(1);
  const shownTab: WatchTab = tab ?? (data?.holdings.length ? "position" : data?.watchlist.length ? "watch" : "ladder");

  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><Radio size={15} /> 盯盘</div>
        <h1>实时动态</h1>
        <p>盘中异动、持仓与自选、昨日梯队、大票和昨日成交额前十。页面开着才取行情,切到后台就停。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={refresh} disabled={loading} aria-label="刷新盯盘">
        <RefreshCw size={17} className={loading ? "spin" : ""} />
      </button>
    </div>
    {!data && (error
      ? <div className="monitor-error" role="alert">实时动态这次没取到:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>
      : <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在取名单和几百只票的行情,第一次要几秒…</div>)}
    {data && <LiveWatchView v={data} tab={shownTab} threshold={threshold} scope={scope} ladderMin={ladderMin}
      onTab={setTab} onThreshold={setThreshold} onScope={setScope} onLadderMin={setLadderMin}
      onOpenSymbol={onOpenSymbol} error={error} paused={!live} manual={manual} />}
  </section>;
}
