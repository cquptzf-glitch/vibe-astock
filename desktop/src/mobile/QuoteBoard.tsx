import { useState } from "react";
import { RefreshCw } from "lucide-react";
import { useDiscoveryData } from "./discoveryData";
import { fmtLots, fmtMoney, fmtPrice } from "./chartIndicators";
import { usePresentation } from "./presentation";
import { useForegroundRefresh } from "./useForegroundRefresh";
import "./kline-chart.css";

interface Level { price: number | null; volume: number | null }
export interface Snapshot {
  status: string; name: string; symbol: string; market: string; price: number | null; prev_close: number | null; open: number | null;
  high: number | null; low: number | null; change: number | null; change_pct: number | null; volume_lots: number | null; amount_yuan: number | null;
  turnover_pct: number | null; amplitude_pct: number | null; volume_ratio: number | null; avg_price: number | null;
  limit_up: number | null; limit_down: number | null; pe_ttm: number | null; pe_dynamic: number | null; pe_static: number | null; pb: number | null;
  total_mcap_yuan: number | null; float_mcap_yuan: number | null; outer_lots: number | null; inner_lots: number | null; commission_ratio: number | null;
  bids: Level[]; asks: Level[]; quote_time: string | null; is_today: boolean; source: string; fallback_reason: string | null;
}

const cls = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const vsPrev = (v: number | null, prev: number | null) => (v == null || prev == null ? "" : cls(v - prev));
const pct = (v: number | null) => (v == null ? "—" : `${v > 0 ? "+" : ""}${v.toFixed(2)}%`);
const plain = (v: number | null, suffix = "") => (v == null || !Number.isFinite(v) ? "—" : v.toFixed(2) + suffix);

export function QuoteBoardView({ data, showBook = false }: { data: Snapshot; showBook?: boolean }) {
  const [book, setBook] = useState(showBook);
  const p = data.prev_close;
  const outer = data.outer_lots ?? 0, inner = data.inner_lots ?? 0;
  const hasBook = data.bids.some((b) => b.price) || data.asks.some((a) => a.price);
  const cells: [string, string, string?][] = [
    ["今开", fmtPrice(data.open), vsPrev(data.open, p)], ["昨收", fmtPrice(p)], ["最高", fmtPrice(data.high), vsPrev(data.high, p)], ["最低", fmtPrice(data.low), vsPrev(data.low, p)],
    ["成交量", fmtLots(data.volume_lots)], ["成交额", fmtMoney(data.amount_yuan)], ["换手率", plain(data.turnover_pct, "%")], ["振幅", plain(data.amplitude_pct, "%")],
    ["量比", plain(data.volume_ratio)], ["均价", fmtPrice(data.avg_price), vsPrev(data.avg_price, p)], ["涨停", fmtPrice(data.limit_up), "up"], ["跌停", fmtPrice(data.limit_down), "down"],
    ["市盈TTM", plain(data.pe_ttm)], ["市盈动", plain(data.pe_dynamic)], ["市净率", plain(data.pb)], ["委比", plain(data.commission_ratio, "%"), cls(data.commission_ratio)],
    ["总市值", fmtMoney(data.total_mcap_yuan)], ["流通值", fmtMoney(data.float_mcap_yuan)], ["外盘", fmtLots(data.outer_lots), "up"], ["内盘", fmtLots(data.inner_lots), "down"],
  ];
  return <section className="qb-card" aria-label="行情盘口">
    <div className="qb-head">
      <strong className={cls(data.change)}>{fmtPrice(data.price)}</strong>
      <span className={cls(data.change)}>{data.change == null ? "—" : `${data.change > 0 ? "+" : ""}${data.change.toFixed(2)}`}</span>
      <span className={cls(data.change_pct)}>{pct(data.change_pct)}</span>
      <small>{data.quote_time ?? "—"}{data.is_today ? "" : " · 非今日行情"} · {data.source}</small>
    </div>
    <div className="qb-grid">{cells.map(([label, value, className]) => <div key={label}><span>{label}</span><b className={className}>{value}</b></div>)}</div>
    {outer + inner > 0 && <><div className="qb-bar" aria-label="外盘内盘比例"><i style={{ width: `${(outer / (outer + inner)) * 100}%` }} /><b style={{ width: `${(inner / (outer + inner)) * 100}%` }} /></div>
      <div className="qb-foot"><span>外盘 {Math.round((outer / (outer + inner)) * 100)}%</span><span>内盘 {Math.round((inner / (outer + inner)) * 100)}%</span></div></>}
    {hasBook && <button type="button" className="qb-toggle" aria-expanded={book} onClick={() => setBook(!book)}>{book ? "收起五档" : "展开五档盘口"}</button>}
    {hasBook && book && <div className="qb-book">
      <ol aria-label="卖盘">{[...data.asks].reverse().map((a, i) => <li key={i}><span>卖{5 - i}</span><b className={vsPrev(a.price, p)}>{fmtPrice(a.price)}</b><em>{a.volume ?? "—"}</em></li>)}</ol>
      <ol aria-label="买盘">{data.bids.map((b, i) => <li key={i}><span>买{i + 1}</span><b className={vsPrev(b.price, p)}>{fmtPrice(b.price)}</b><em>{b.volume ?? "—"}</em></li>)}</ol>
    </div>}
    {data.fallback_reason && <p className="kc-note">已改用东财快照（无五档）：{data.fallback_reason}</p>}
  </section>;
}

export function QuoteBoard({ symbol }: { symbol: string }) {
  const { refreshSeconds } = usePresentation();
  const result = useDiscoveryData<Snapshot>(`/mobile/stock/snapshot?symbol=${encodeURIComponent(symbol)}`);
  useForegroundRefresh(async () => result.refresh(), refreshSeconds);
  if (result.data) return <QuoteBoardView data={result.data} />;
  return <section className="qb-card">{result.loading ? <div className="mobile-empty"><RefreshCw size={15} className="spin" /> 正在加载盘口…</div>
    : <div className="mobile-error" role="alert">{result.error}<button className="discovery-action" onClick={result.refresh}>重试</button></div>}</section>;
}
