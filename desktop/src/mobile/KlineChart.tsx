import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { ChevronsRight, Minus, Plus, RefreshCw } from "lucide-react";
import { useDiscoveryData } from "./discoveryData";
import { useCachedResource } from "./emotionData";
import { useForegroundRefresh } from "./useForegroundRefresh";
import { usePresentation } from "./presentation";
import {
  MINUTE_SLOTS, barChange, boll, fmtLots, fmtMoney, fmtPrice, kdj, ma, macd, minuteSlot, parseBars, rsi, type Bar,
} from "./chartIndicators";
import "./kline-chart.css";
import { ChartShareCard } from "./ChartShareCard";

type Tab = "intraday" | "five" | "day" | "week" | "month" | "minute";
type MinutePeriod = "m60" | "m30" | "m15" | "m5";
type Overlay = "ma" | "boll" | "none";
type Sub = "vol" | "macd" | "kdj" | "rsi";
interface Prefs { tab: Tab; minute: MinutePeriod; adjust: "qfq" | "none"; overlay: Overlay; sub: Sub }

const TABS: { id: Tab; label: string }[] = [
  { id: "intraday", label: "分时" }, { id: "five", label: "五日" }, { id: "day", label: "日K" },
  { id: "week", label: "周K" }, { id: "month", label: "月K" }, { id: "minute", label: "分钟" },
];
const MINUTES: { id: MinutePeriod; label: string }[] = [{ id: "m60", label: "60分" }, { id: "m30", label: "30分" }, { id: "m15", label: "15分" }, { id: "m5", label: "5分" }];
const PREFS_KEY = "vibe.chart.prefs";
const DEFAULT_PREFS: Prefs = { tab: "day", minute: "m60", adjust: "qfq", overlay: "ma", sub: "vol" };

function loadPrefs(): Prefs {
  try {
    const saved = JSON.parse(window.localStorage.getItem(PREFS_KEY) ?? "null") as Partial<Prefs> | null;
    if (!saved) return DEFAULT_PREFS;
    return {
      tab: TABS.some((t) => t.id === saved.tab) ? saved.tab! : DEFAULT_PREFS.tab,
      minute: MINUTES.some((m) => m.id === saved.minute) ? saved.minute! : DEFAULT_PREFS.minute,
      adjust: saved.adjust === "none" ? "none" : "qfq",
      overlay: (["ma", "boll", "none"] as const).includes(saved.overlay as Overlay) ? saved.overlay! : "ma",
      sub: (["vol", "macd", "kdj", "rsi"] as const).includes(saved.sub as Sub) ? saved.sub! : "vol",
    };
  } catch { return DEFAULT_PREFS; }
}

function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(340);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setWidth(Math.max(240, Math.round(el.clientWidth || 340)));
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}

const cls = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const pctText = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : `${v > 0 ? "+" : ""}${v.toFixed(2)}%`);

function polyline(values: (number | null)[], x: (i: number) => number, y: (v: number) => number, start = 0): string[] {
  // 遇到 null 断开,返回若干段
  const segs: string[] = [];
  let cur: string[] = [];
  values.forEach((v, i) => {
    if (v == null || !Number.isFinite(v)) { if (cur.length > 1) segs.push(cur.join(" ")); cur = []; return; }
    cur.push(`${x(i + start).toFixed(1)},${y(v).toFixed(1)}`);
  });
  if (cur.length > 1) segs.push(cur.join(" "));
  return segs;
}

function Lines({ values, x, y, className, start = 0 }: { values: (number | null)[]; x: (i: number) => number; y: (v: number) => number; className: string; start?: number }) {
  return <>{polyline(values, x, y, start).map((points, i) => <polyline key={i} className={className} points={points} />)}</>;
}

/** 单指针:拖动平移、点按/长按十字线;双指:捏合缩放;滚轮缩放。 */
function useGestures(opts: { step: number; onPan?: (bars: number) => void; onPanStart?: () => void; onZoom?: (factor: number) => void; onCursor: (x: number | null) => void; cursorOn: boolean }) {
  const pointers = useRef(new Map<number, number>());
  const g = useRef({ mode: "none" as "none" | "pan" | "pinch" | "press", startX: 0, startTime: 0, startDist: 0, timer: 0 as unknown as ReturnType<typeof setTimeout> | 0 });
  const local = (e: ReactPointerEvent<SVGSVGElement>) => e.clientX - e.currentTarget.getBoundingClientRect().left;
  return {
    onPointerDown(e: ReactPointerEvent<SVGSVGElement>) {
      e.currentTarget.setPointerCapture?.(e.pointerId);
      pointers.current.set(e.pointerId, e.clientX);
      if (pointers.current.size === 2) {
        const [a, b] = [...pointers.current.values()];
        g.current.mode = "pinch";
        g.current.startDist = Math.max(10, Math.abs(a! - b!));
        clearTimeout(g.current.timer as ReturnType<typeof setTimeout>);
        return;
      }
      const x = local(e);
      g.current = { ...g.current, mode: "none", startX: e.clientX, startTime: Date.now() };
      clearTimeout(g.current.timer as ReturnType<typeof setTimeout>);
      g.current.timer = setTimeout(() => { if (g.current.mode === "none") { g.current.mode = "press"; opts.onCursor(x); } }, 320);
    },
    onPointerMove(e: ReactPointerEvent<SVGSVGElement>) {
      if (!pointers.current.has(e.pointerId)) {
        if (opts.cursorOn && e.pointerType === "mouse") opts.onCursor(local(e));
        return;
      }
      pointers.current.set(e.pointerId, e.clientX);
      const st = g.current;
      if (st.mode === "pinch" && pointers.current.size === 2) {
        const [a, b] = [...pointers.current.values()];
        const dist = Math.max(10, Math.abs(a! - b!));
        opts.onZoom?.(st.startDist / dist);
        st.startDist = dist;
        return;
      }
      const dx = e.clientX - st.startX;
      if (st.mode === "none" && Math.abs(dx) > 6 && opts.onPan) {
        clearTimeout(st.timer as ReturnType<typeof setTimeout>);
        st.mode = "pan";
        opts.onPanStart?.();
        opts.onCursor(null);
      }
      if (st.mode === "pan" && opts.onPan) {
        const moved = Math.trunc(dx / Math.max(opts.step, 1));
        if (moved) { opts.onPan(moved); st.startX += moved * opts.step; }
      } else if (st.mode === "press" || (st.mode === "none" && opts.cursorOn)) {
        opts.onCursor(local(e));
      }
    },
    onPointerUp(e: ReactPointerEvent<SVGSVGElement>) {
      pointers.current.delete(e.pointerId);
      const st = g.current;
      clearTimeout(st.timer as ReturnType<typeof setTimeout>);
      if (st.mode === "none" && Date.now() - st.startTime < 320) opts.onCursor(opts.cursorOn ? null : local(e));
      if (pointers.current.size === 0) st.mode = "none";
    },
    onPointerCancel(e: ReactPointerEvent<SVGSVGElement>) {
      pointers.current.delete(e.pointerId);
      clearTimeout(g.current.timer as ReturnType<typeof setTimeout>);
      g.current.mode = "none";
    },
    onWheel(e: React.WheelEvent<SVGSVGElement>) {
      if (!opts.onZoom) return;
      opts.onZoom(e.deltaY > 0 ? 1.15 : 1 / 1.15);
    },
  };
}

function Legend({ items }: { items: [string, string, string][] }) {
  return <div className="kc-legend">{items.map(([label, value, className]) => <span key={label} className={className}>{label} <b>{value}</b></span>)}</div>;
}

function InfoStrip({ cells }: { cells: [string, ReactNode, string?][] }) {
  return <div className="kc-info">{cells.map(([label, value, className]) => <div key={label}><span>{label}</span><b className={className}>{value}</b></div>)}</div>;
}

// ------------------------------------------------------------ K 线

export interface KlineData { status: string; period: string; adjust: string; rows: unknown[]; dropped: number; source: string; fallback_reason: string | null; note: string; includes_today: boolean }

export function KlineView(props: { data: KlineData; overlay: Overlay; sub: Sub; width: number; isMinute: boolean; symbol?: string; notice?: string }) {
  const bars = useMemo(() => parseBars(props.data.rows), [props.data]);
  if (bars.length < 2) return <div className="mobile-empty">K 线不足，暂时无法绘制</div>;
  return <KlineCanvas {...props} bars={bars} />;
}

function KlineCanvas({ data, overlay, sub, width, isMinute, bars, symbol, notice }: { data: KlineData; overlay: Overlay; sub: Sub; width: number; isMinute: boolean; bars: Bar[]; symbol?: string; notice?: string }) {
  const chartRef = useRef<SVGSVGElement>(null);
  const closes = useMemo(() => bars.map((b) => b.close), [bars]);
  const ind = useMemo(() => ({
    ma5: ma(closes, 5), ma10: ma(closes, 10), ma20: ma(closes, 20), ma60: ma(closes, 60),
    boll: boll(closes), macd: macd(closes), kdj: kdj(bars), rsi6: rsi(closes, 6), rsi12: rsi(closes, 12), rsi24: rsi(closes, 24),
    vol5: ma(bars.map((b) => b.volume), 5), vol10: ma(bars.map((b) => b.volume), 10),
  }), [bars, closes]);
  const [count, setCount] = useState(60);
  const [offset, setOffset] = useState(0);
  const [cursor, setCursor] = useState<number | null>(null);
  const panBase = useRef(0);
  const n = bars.length;
  const visibleCount = Math.min(count, n);
  const end = Math.max(visibleCount, n - offset);
  const start = Math.max(0, end - visibleCount);
  const view = bars.slice(start, end);
  const step = width / Math.max(visibleCount, 1);
  const MAIN = 210, GAP = 20, SUB = 78, H = MAIN + GAP + SUB;

  const overlaySeries = overlay === "ma" ? [ind.ma5, ind.ma10, ind.ma20, ind.ma60] : overlay === "boll" ? [ind.boll.up, ind.boll.mid, ind.boll.low] : [];
  const lows = view.map((b) => b.low), highs = view.map((b) => b.high);
  overlaySeries.forEach((s) => s.slice(start, end).forEach((v) => { if (v != null) { lows.push(v); highs.push(v); } }));
  let max = Math.max(...highs), min = Math.min(...lows);
  const pad = (max - min || max * 0.01 || 1) * 0.06;
  max += pad; min -= pad;
  const x = (i: number) => (i - start) * step + step / 2;
  const y = (v: number) => 4 + ((max - v) / (max - min)) * (MAIN - 8);
  const body = Math.max(1, Math.min(step * 0.72, 14));
  const hiIdx = start + view.reduce((best, b, i) => (b.high > view[best]!.high ? i : best), 0);
  const loIdx = start + view.reduce((best, b, i) => (b.low < view[best]!.low ? i : best), 0);
  const ci = cursor == null ? null : Math.min(end - 1, Math.max(start, cursor));
  const at = ci ?? end - 1;
  const bar = bars[at]!;
  const ch = barChange(bars, at);

  // 副图
  const subTop = MAIN + GAP;
  let subMax = 1, subMin = 0;
  const visible = (s: (number | null)[]) => s.slice(start, end).filter((v): v is number => v != null && Number.isFinite(v));
  if (sub === "vol") { subMax = Math.max(1, ...view.map((b) => b.volume), ...visible(ind.vol5), ...visible(ind.vol10)); subMin = 0; }
  if (sub === "macd") { const all = [...visible(ind.macd.dif), ...visible(ind.macd.dea), ...visible(ind.macd.hist)]; subMax = Math.max(1e-9, ...all.map(Math.abs)); subMin = -subMax; }
  if (sub === "kdj") { const all = [...visible(ind.kdj.k), ...visible(ind.kdj.d), ...visible(ind.kdj.j), 0, 100]; subMax = Math.max(...all); subMin = Math.min(...all); }
  if (sub === "rsi") { subMax = 100; subMin = 0; }
  const sy = (v: number) => subTop + 3 + ((subMax - v) / (subMax - subMin || 1)) * (SUB - 6);

  const gestures = useGestures({
    step,
    onPanStart: () => { panBase.current = offset; },
    onPan: (moved) => setOffset((o) => Math.min(Math.max(0, o + moved), Math.max(0, n - visibleCount))),
    onZoom: (factor) => setCount((c) => Math.round(Math.min(Math.max(c * factor, 20), Math.min(n, 240)))),
    onCursor: (px) => setCursor(px == null ? null : start + Math.min(visibleCount - 1, Math.max(0, Math.floor(px / step)))),
    cursorOn: cursor != null,
  });

  const timeLabel = (t: string) => (isMinute ? t.slice(5, 16) : t.slice(0, 10));
  const val = (s: (number | null)[], digits = 2) => fmtPrice(s[at] ?? null, digits);
  const mainLegend: [string, string, string][] = overlay === "ma"
    ? [["MA5", val(ind.ma5), "ma5"], ["MA10", val(ind.ma10), "ma10"], ["MA20", val(ind.ma20), "ma20"], ["MA60", val(ind.ma60), "ma60"]]
    : overlay === "boll" ? [["BOLL(20,2) 中", val(ind.boll.mid), "ma10"], ["上", val(ind.boll.up), "ma5"], ["下", val(ind.boll.low), "ma20"]] : [];
  const subLegend: [string, string, string][] = sub === "vol"
    ? [["VOL", fmtLots(bar.volume), "plain"], ["MA5", fmtLots(ind.vol5[at]), "ma5"], ["MA10", fmtLots(ind.vol10[at]), "ma10"]]
    : sub === "macd" ? [["MACD(12,26,9) DIF", val(ind.macd.dif, 3), "ma5"], ["DEA", val(ind.macd.dea, 3), "ma10"], ["MACD", val(ind.macd.hist, 3), cls(ind.macd.hist[at])]]
    : sub === "kdj" ? [["KDJ(9,3,3) K", val(ind.kdj.k), "ma5"], ["D", val(ind.kdj.d), "ma10"], ["J", val(ind.kdj.j), "ma20"]]
    : [["RSI6", val(ind.rsi6), "ma5"], ["RSI12", val(ind.rsi12), "ma10"], ["RSI24", val(ind.rsi24), "ma20"]];
  const grid = [0, 1, 2, 3].map((k) => min + ((max - min) * (3 - k)) / 3);
  const label = (i: number, text: string, yy: number) => {
    const right = x(i) > width * 0.6;
    return <text className="kc-mark" x={right ? x(i) - 4 : x(i) + 4} y={yy} textAnchor={right ? "end" : "start"}>{right ? `${text} →` : `← ${text}`}</text>;
  };

  return <div className="kc-body">
    <InfoStrip cells={[
      ["时间", timeLabel(bar.time)], ["开", fmtPrice(bar.open)], ["高", fmtPrice(bar.high)], ["低", fmtPrice(bar.low)],
      ["收", fmtPrice(bar.close), cls(ch.pct)], ["涨幅", pctText(ch.pct), cls(ch.pct)], ["振幅", pctText(ch.amplitude).replace("+", "")],
      ["量", fmtLots(bar.volume)], ["额", fmtMoney(bar.amount)], ["换手", bar.turnover == null ? "—" : bar.turnover.toFixed(2) + "%"],
    ]} />
    <Legend items={mainLegend} />
    <svg ref={chartRef} className="kc-svg" width={width} height={H} viewBox={`0 0 ${width} ${H}`} role="img"
      aria-label={`K 线图，显示 ${timeLabel(view[0]!.time)} 至 ${timeLabel(view[view.length - 1]!.time)} 共 ${view.length} 根`} {...gestures}>
      {grid.map((g, k) => <g key={k}><line className="kc-grid" x1={0} x2={width} y1={y(g)} y2={y(g)} /><text className="kc-axis" x={2} y={k === 0 ? y(g) + 10 : y(g) - 2}>{fmtPrice(g)}</text></g>)}
      {view.map((b, k) => {
        const i = start + k;
        const up = b.close >= b.open;
        const top = y(Math.max(b.open, b.close)), h = Math.max(1, Math.abs(y(b.open) - y(b.close)));
        return <g key={b.time} className={up ? "kc-up" : "kc-down"}>
          <line x1={x(i)} x2={x(i)} y1={y(b.high)} y2={y(b.low)} />
          <rect x={x(i) - body / 2} y={top} width={body} height={h} />
        </g>;
      })}
      {overlay === "ma" && <>
        <Lines values={ind.ma5.slice(start, end)} x={x} y={y} start={start} className="kc-line ma5" />
        <Lines values={ind.ma10.slice(start, end)} x={x} y={y} start={start} className="kc-line ma10" />
        <Lines values={ind.ma20.slice(start, end)} x={x} y={y} start={start} className="kc-line ma20" />
        <Lines values={ind.ma60.slice(start, end)} x={x} y={y} start={start} className="kc-line ma60" />
      </>}
      {overlay === "boll" && <>
        <Lines values={ind.boll.up.slice(start, end)} x={x} y={y} start={start} className="kc-line ma5" />
        <Lines values={ind.boll.mid.slice(start, end)} x={x} y={y} start={start} className="kc-line ma10" />
        <Lines values={ind.boll.low.slice(start, end)} x={x} y={y} start={start} className="kc-line ma20" />
      </>}
      {label(hiIdx, fmtPrice(bars[hiIdx]!.high), y(bars[hiIdx]!.high) - 3)}
      {label(loIdx, fmtPrice(bars[loIdx]!.low), y(bars[loIdx]!.low) + 10)}
      <line className="kc-grid" x1={0} x2={width} y1={MAIN} y2={MAIN} />
      {[0, Math.floor((view.length - 1) / 2), view.length - 1].map((k, j) => <text key={j} className="kc-axis" x={j === 0 ? 2 : j === 2 ? width - 2 : width / 2} y={MAIN + 14}
        textAnchor={j === 0 ? "start" : j === 2 ? "end" : "middle"}>{timeLabel(view[k]!.time)}</text>)}
      {sub === "vol" && view.map((b, k) => <rect key={b.time} className={b.close >= b.open ? "kc-up" : "kc-down"} x={x(start + k) - body / 2} y={sy(b.volume)} width={body} height={Math.max(0.5, sy(0) - sy(b.volume))} />)}
      {sub === "vol" && <><Lines values={ind.vol5.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma5" /><Lines values={ind.vol10.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma10" /></>}
      {sub === "macd" && <>
        <line className="kc-grid" x1={0} x2={width} y1={sy(0)} y2={sy(0)} />
        {ind.macd.hist.slice(start, end).map((v, k) => <line key={k} className={v >= 0 ? "kc-bar up" : "kc-bar down"} x1={x(start + k)} x2={x(start + k)} y1={sy(0)} y2={sy(v)} strokeWidth={Math.max(1, body * 0.6)} />)}
        <Lines values={ind.macd.dif.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma5" />
        <Lines values={ind.macd.dea.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma10" />
      </>}
      {sub === "kdj" && <>
        {[20, 80].map((v) => <line key={v} className="kc-grid dash" x1={0} x2={width} y1={sy(v)} y2={sy(v)} />)}
        <Lines values={ind.kdj.k.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma5" />
        <Lines values={ind.kdj.d.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma10" />
        <Lines values={ind.kdj.j.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma20" />
      </>}
      {sub === "rsi" && <>
        {[30, 70].map((v) => <line key={v} className="kc-grid dash" x1={0} x2={width} y1={sy(v)} y2={sy(v)} />)}
        <Lines values={ind.rsi6.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma5" />
        <Lines values={ind.rsi12.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma10" />
        <Lines values={ind.rsi24.slice(start, end)} x={x} y={sy} start={start} className="kc-line ma20" />
      </>}
      {ci != null && <g className="kc-cross">
        <line x1={x(ci)} x2={x(ci)} y1={0} y2={H} />
        <line x1={0} x2={width} y1={y(bar.close)} y2={y(bar.close)} />
        <rect x={0} y={y(bar.close) - 8} width={52} height={16} rx={3} /><text x={26} y={y(bar.close) + 4} textAnchor="middle">{fmtPrice(bar.close)}</text>
      </g>}
    </svg>
    <Legend items={subLegend} />
    <div className="kc-tools">
      <button type="button" aria-label="缩小（显示更多 K 线）" onClick={() => setCount((c) => Math.min(Math.round(c * 1.4), Math.min(n, 240)))}><Minus size={15} /></button>
      <button type="button" aria-label="放大（显示更少 K 线）" onClick={() => setCount((c) => Math.max(Math.round(c / 1.4), 20))}><Plus size={15} /></button>
      <button type="button" aria-label="回到最新" disabled={offset === 0} onClick={() => { setOffset(0); setCursor(null); }}><ChevronsRight size={15} /></button>
      <span>{view.length} 根 · {timeLabel(view[0]!.time).slice(5)} ~ {timeLabel(view[view.length - 1]!.time).slice(5)}</span>
    </div>
    <ChartShareCard chartRef={chartRef} identity={data} key={`${start}-${end}-${overlay}-${sub}-${cursor}-${symbol}-${notice}-${width}`} slug={`kline-${symbol ?? "stock"}-${data.period}`} spec={{
      title: `${symbol ?? "个股"} · ${{day:"日K",week:"周K",month:"月K",m5:"5分钟K",m15:"15分钟K",m30:"30分钟K",m60:"60分钟K"}[data.period] ?? data.period}`,
      subtitle: `${timeLabel(view[0]!.time)} → ${timeLabel(view[view.length - 1]!.time)} · ${view.length} 根 · ${data.adjust === "qfq" ? "前复权" : data.adjust === "none" ? "不复权" : data.adjust}`,
      source: data.source,
      details: [mainLegend.map(([label,value]) => `${label} ${value}`).join(" · "), subLegend.map(([label,value]) => `${label} ${value}`).join(" · "), `读数时点：${timeLabel(bar.time)} · 收 ${fmtPrice(bar.close)}`].filter(Boolean),
      notes: [notice ?? "", data.includes_today ? "含当日K线，盘中读数可能变化。" : "", data.dropped ? `已剔除 ${data.dropped} 根异常数据。` : "", data.fallback_reason ? `数据源回退：${data.fallback_reason}` : "", data.note].filter(Boolean),
    }} />
    {(data.fallback_reason || data.dropped > 0 || data.note) && <p className="kc-note">{[data.fallback_reason ? `已改用腾讯：${data.fallback_reason}` : "", data.dropped ? `剔除 ${data.dropped} 根异常数据` : "", data.note].filter(Boolean).join("；")}</p>}
  </div>;
}

// ------------------------------------------------------------ 分时 / 五日

export interface IntradayData { status: string; days: { date: string; pre_close: number | null; points: [string, number, number | null, number, number][] }[]; source: string; fallback_reason: string | null }

export function IntradayView({ data, width, symbol, notice }: { data: IntradayData; width: number; symbol?: string; notice?: string }) {
  const days = data.days.filter((d) => d.points.length);
  const points = useMemo(() => days.flatMap((d, di) => d.points.flatMap(([t, price, avg, vol, amount]) => {
    const slot = minuteSlot(t);
    return slot == null ? [] : [{ day: di, date: d.date, time: t, slot: di * MINUTE_SLOTS + slot, price, avg, vol, amount }];
  })), [data]);
  if (!days.length || points.length < 2) return <div className="mobile-empty">还没有分时数据（未开盘、停牌或数据源暂不可用）</div>;
  return <IntradayCanvas data={data} days={days} points={points} width={width} symbol={symbol} notice={notice} />;
}

interface MinutePoint { day: number; date: string; time: string; slot: number; price: number; avg: number | null; vol: number; amount: number }

function IntradayCanvas({ data, days, points, width, symbol, notice }: { data: IntradayData; days: IntradayData["days"]; points: MinutePoint[]; width: number; symbol?: string; notice?: string }) {
  const chartRef = useRef<SVGSVGElement>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const PRICE = 190, GAP = 18, VOL = 64, H = PRICE + GAP + VOL;
  const slots = MINUTE_SLOTS * Math.max(days.length, 1);
  const step = width / slots;
  const base = days.length === 1 ? days[0]!.pre_close ?? points[0]!.price : days[0]!.pre_close ?? points[0]!.price;
  const prices = points.flatMap((p) => [p.price, ...(p.avg ? [p.avg] : [])]);
  const dev = Math.max(...prices.map((p) => Math.abs(p - base)), base * 0.002);
  const max = base + dev * 1.08, min = base - dev * 1.08;
  const x = (slot: number) => slot * step + step / 2;
  const y = (v: number) => 4 + ((max - v) / (max - min)) * (PRICE - 8);
  const maxVol = Math.max(1, ...points.map((p) => p.vol));
  const vy = (v: number) => PRICE + GAP + VOL - (v / maxVol) * (VOL - 4);
  const gestures = useGestures({
    step,
    onCursor: (px) => {
      if (px == null) { setCursor(null); return; }
      const slot = Math.floor(px / step);
      let best = 0;
      points.forEach((p, i) => { if (Math.abs(p.slot - slot) < Math.abs(points[best]!.slot - slot)) best = i; });
      setCursor(best);
    },
    cursorOn: cursor != null,
  });
  const at = points[cursor ?? points.length - 1]!;
  const dayBase = days[at.day]!.pre_close ?? base;
  const byDay = days.map((_, di) => points.filter((p) => p.day === di));
  const lastDay = days[days.length - 1]!;
  const totalVol = byDay[byDay.length - 1]!.reduce((s, p) => s + p.vol, 0);
  return <div className="kc-body">
    <InfoStrip cells={[
      ["时间", days.length > 1 ? `${at.date.slice(5)} ${at.time}` : at.time], ["价格", fmtPrice(at.price), cls(at.price - dayBase)],
      ["涨跌", pctText(dayBase ? (at.price / dayBase - 1) * 100 : null), cls(at.price - dayBase)], ["均价", fmtPrice(at.avg)],
      ["量", fmtLots(at.vol)], ["额", fmtMoney(at.amount)], ["昨收", fmtPrice(dayBase)], ["当日量", fmtLots(totalVol)],
    ]} />
    <div className="kc-legend"><span className="price">价格</span><span className="avg">均价</span><span className="plain">{days.length > 1 ? `${days[0]!.date} ~ ${lastDay.date}` : lastDay.date}</span></div>
    <svg ref={chartRef} className="kc-svg" width={width} height={H} viewBox={`0 0 ${width} ${H}`} role="img" aria-label={days.length > 1 ? "五日分时图" : "分时图"} {...gestures}>
      {[max, (max + base) / 2, base, (min + base) / 2, min].map((v, k) => <g key={k}>
        <line className={k === 2 ? "kc-grid base" : "kc-grid"} x1={0} x2={width} y1={y(v)} y2={y(v)} />
        <text className={`kc-axis ${cls(v - base)}`} x={2} y={k === 0 ? y(v) + 10 : y(v) - 2}>{fmtPrice(v)}</text>
        <text className={`kc-axis ${cls(v - base)}`} x={width - 2} y={k === 0 ? y(v) + 10 : y(v) - 2} textAnchor="end">{pctText(((v / base) - 1) * 100)}</text>
      </g>)}
      {days.length > 1 && days.slice(1).map((_, k) => <line key={k} className="kc-grid dash" x1={(k + 1) * MINUTE_SLOTS * step} x2={(k + 1) * MINUTE_SLOTS * step} y1={0} y2={H} />)}
      {byDay.map((pts, di) => <g key={di}>
        <polyline className="kc-line price" points={pts.map((p) => `${x(p.slot).toFixed(1)},${y(p.price).toFixed(1)}`).join(" ")} />
        <polyline className="kc-line avg" points={pts.filter((p) => p.avg).map((p) => `${x(p.slot).toFixed(1)},${y(p.avg!).toFixed(1)}`).join(" ")} />
      </g>)}
      {points.map((p, i) => <line key={i} className={`kc-bar ${i > 0 && p.day === points[i - 1]!.day && p.price < points[i - 1]!.price ? "down" : "up"}`}
        x1={x(p.slot)} x2={x(p.slot)} y1={vy(0)} y2={vy(p.vol)} strokeWidth={Math.max(0.6, step * 0.7)} />)}
      <line className="kc-grid" x1={0} x2={width} y1={PRICE} y2={PRICE} />
      {days.length > 1
        ? days.map((d, di) => <text key={d.date} className="kc-axis" x={(di + 0.5) * MINUTE_SLOTS * step} y={PRICE + 13} textAnchor="middle">{d.date.slice(5)}</text>)
        : [["09:30", 0, "start"], ["11:30/13:00", 121, "middle"], ["15:00", MINUTE_SLOTS - 1, "end"]].map(([t, s, anchor]) =>
          <text key={t as string} className="kc-axis" x={anchor === "start" ? 2 : anchor === "end" ? width - 2 : x(s as number)} y={PRICE + 13} textAnchor={anchor as "start"}>{t}</text>)}
      {cursor != null && <g className="kc-cross">
        <line x1={x(at.slot)} x2={x(at.slot)} y1={0} y2={H} />
        <line x1={0} x2={width} y1={y(at.price)} y2={y(at.price)} />
        <rect x={0} y={y(at.price) - 8} width={52} height={16} rx={3} /><text x={26} y={y(at.price) + 4} textAnchor="middle">{fmtPrice(at.price)}</text>
      </g>}
    </svg>
    <ChartShareCard chartRef={chartRef} identity={data} key={`${cursor ?? "latest"}-${symbol}-${notice}-${width}`} slug={`intraday-${symbol ?? "stock"}-${days.length}`} spec={{
      title: `${symbol ?? "个股"} · ${days.length > 1 ? "五日分时" : "分时"}`,
      subtitle: `${days[0]!.date} → ${lastDay.date} · 价格 / 均价 / 成交量`, source: data.source,
      details: [`读数时点：${at.date} ${at.time} · 价格 ${fmtPrice(at.price)} · 均价 ${fmtPrice(at.avg)}`, `当日量 ${fmtLots(totalVol)} · 昨收基准 ${fmtPrice(dayBase)}`],
      notes: [notice ?? "", data.fallback_reason ? `数据源回退：${data.fallback_reason}` : "", "盘中读数随数据源更新，不是交易所实时逐笔行情。"].filter(Boolean),
    }} />
    {data.fallback_reason && <p className="kc-note">已改用东财：{data.fallback_reason}</p>}
  </div>;
}

// ------------------------------------------------------------ 外壳

function KlineSection({ symbol, period, adjust, overlay, sub, width }: { symbol: string; period: string; adjust: string; overlay: Overlay; sub: Sub; width: number }) {
  const result = useCachedResource<KlineData>(`/mobile/stock/kline?symbol=${encodeURIComponent(symbol)}&period=${period}&adjust=${adjust}&limit=300`);
  return <>
    {result.loading && !result.data && <div className="mobile-empty"><RefreshCw size={15} className="spin" /> 正在加载 K 线…</div>}
    {result.error && <div className="mobile-error" role="alert">{result.error}<button className="discovery-action" onClick={result.refresh}>重试</button></div>}
    {result.data && <KlineView key={`${period}-${adjust}`} data={result.data} overlay={overlay} sub={sub} width={width} isMinute={period.startsWith("m")} symbol={symbol} notice={result.error} />}
  </>;
}

function IntradaySection({ symbol, days, width }: { symbol: string; days: 1 | 5; width: number }) {
  const { refreshSeconds } = usePresentation();
  const result = useDiscoveryData<IntradayData>(`/mobile/stock/intraday?symbol=${encodeURIComponent(symbol)}&days=${days}`);
  useForegroundRefresh(async () => { if (days === 1) result.refresh(); }, days === 1 ? refreshSeconds : 0);
  return <>
    {result.loading && !result.data && <div className="mobile-empty"><RefreshCw size={15} className="spin" /> 正在加载分时…</div>}
    {result.error && <div className="mobile-error" role="alert">{result.error}<button className="discovery-action" onClick={result.refresh}>重试</button></div>}
    {result.data && <IntradayView data={result.data} width={width} symbol={symbol} notice={result.error} />}
  </>;
}

export function StockChart({ symbol }: { symbol: string }) {
  const [prefs, setPrefsState] = useState<Prefs>(loadPrefs);
  const [ref, width] = useWidth();
  const setPrefs = (patch: Partial<Prefs>) => setPrefsState((p) => {
    const next = { ...p, ...patch };
    try { window.localStorage.setItem(PREFS_KEY, JSON.stringify(next)); } catch { /* Optional persistence. */ }
    return next;
  });
  const isKline = prefs.tab !== "intraday" && prefs.tab !== "five";
  const period = prefs.tab === "minute" ? prefs.minute : prefs.tab;
  return <section className="kc-card" aria-label="K 线与分时">
    <div className="kc-tabs" role="tablist" aria-label="图表周期">{TABS.map((t) =>
      <button key={t.id} type="button" role="tab" aria-selected={prefs.tab === t.id} className={prefs.tab === t.id ? "active" : ""} onClick={() => setPrefs({ tab: t.id })}>{t.label}</button>)}</div>
    {prefs.tab === "minute" && <div className="kc-subtabs" role="tablist" aria-label="分钟周期">{MINUTES.map((m) =>
      <button key={m.id} type="button" role="tab" aria-selected={prefs.minute === m.id} className={prefs.minute === m.id ? "active" : ""} onClick={() => setPrefs({ minute: m.id })}>{m.label}</button>)}</div>}
    <div ref={ref} className="kc-stage">
      {isKline
        ? <KlineSection symbol={symbol} period={period} adjust={prefs.tab === "minute" ? "none" : prefs.adjust} overlay={prefs.overlay} sub={prefs.sub} width={width} />
        : <IntradaySection key={prefs.tab} symbol={symbol} days={prefs.tab === "five" ? 5 : 1} width={width} />}
    </div>
    {isKline && <div className="kc-options">
      <div className="kc-seg" role="group" aria-label="主图指标">{([["ma", "均线"], ["boll", "BOLL"], ["none", "无"]] as const).map(([id, label]) =>
        <button key={id} type="button" aria-pressed={prefs.overlay === id} className={prefs.overlay === id ? "active" : ""} onClick={() => setPrefs({ overlay: id })}>{label}</button>)}</div>
      <div className="kc-seg" role="group" aria-label="副图指标">{([["vol", "VOL"], ["macd", "MACD"], ["kdj", "KDJ"], ["rsi", "RSI"]] as const).map(([id, label]) =>
        <button key={id} type="button" aria-pressed={prefs.sub === id} className={prefs.sub === id ? "active" : ""} onClick={() => setPrefs({ sub: id })}>{label}</button>)}</div>
      {prefs.tab !== "minute" && <div className="kc-seg" role="group" aria-label="复权">{([["qfq", "前复权"], ["none", "不复权"]] as const).map(([id, label]) =>
        <button key={id} type="button" aria-pressed={prefs.adjust === id} className={prefs.adjust === id ? "active" : ""} onClick={() => setPrefs({ adjust: id })}>{label}</button>)}</div>}
    </div>}
    <p className="kc-note">{isKline ? "拖动平移，双指或 ± 缩放，点按/长按查看十字线。" : "点按/长按查看每分钟价格、均价与成交。"}指标只描述已发生的价格，不构成买卖建议。</p>
  </section>;
}
