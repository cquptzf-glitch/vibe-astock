import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { fmtNumber, fmtPercent, fmtYuan } from "./discoveryData";
import { usePresentation } from "./presentation";
import { useForegroundRefresh } from "./useForegroundRefresh";
import { SectionLink } from "./MobileHeader";

export interface WatchQuote { symbol: string; name: string; price: number | null; change_pct: number | null; amount_yuan: number | null; quote_date: string | null; available: boolean; is_today: boolean; pe_ttm?: number | null; pb?: number | null; turnover_pct?: number | null; group?: string; tags?: string[] }
export function WatchQuoteRows({ items, onOpenSymbol }: { items: WatchQuote[]; onOpenSymbol: (symbol: string) => void }) {
  return <div className="watch-quote-table">
    <div className="watch-quote-columns" aria-hidden="true"><span>名称 / 代码</span><span>最新价</span><span>涨跌幅</span></div>
    {items.map((item) => <button className="watch-quote-row" key={item.symbol} onClick={() => onOpenSymbol(item.symbol)} type="button">
      <span className="watch-quote-name"><strong>{item.name || item.symbol}</strong><small>{item.symbol} · {!item.available ? "暂无报价" : !item.is_today ? item.quote_date ?? "时间未知" : "今日"}</small></span>
      <span className="watch-quote-price"><strong>{fmtNumber(item.price)}</strong><small>额 {fmtYuan(item.amount_yuan)}</small></span>
      <span className={item.change_pct == null ? "watch-quote-change" : item.change_pct < 0 ? "watch-quote-change negative" : "watch-quote-change positive"}>{fmtPercent(item.change_pct)}</span>
    </button>)}
  </div>;
}
export function WatchQuotesPanel({ watchlist, onOpenSymbol, onOpenAll, limit = 50 }: {
  watchlist: { symbol: string; name: string }[]; onOpenSymbol: (symbol: string) => void; onOpenAll?: () => void; limit?: number;
}) {
  const { refreshSeconds } = usePresentation();
  const [data, setData] = useState<{ watchlist: WatchQuote[]; fetched_at: string; missing: string[] } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const request = useRef(0);
  const pending = useRef(false);
  const refresh = useCallback(async () => {
    if (!watchlist.length || pending.current) return;
    const id = ++request.current;
    pending.current = true;
    setLoading(true);
    setError("");
    try {
      const response = await fetch("/api/mobile/monitor", { headers: { Accept: "application/json" } });
      const body = await response.json();
      if (!response.ok) throw new Error(body.message || "行情暂不可用");
      if (request.current === id) setData(body);
    } catch (e) { if (request.current === id) setError(e instanceof Error ? e.message : String(e)); }
    finally { if (request.current === id) { pending.current = false; setLoading(false); } }
  }, [watchlist]);
  useEffect(() => { pending.current = false; setData(null); void refresh(); return () => { request.current++; pending.current = false; }; }, [refresh]);
  useForegroundRefresh(refresh, watchlist.length ? refreshSeconds : 0);
  const bySymbol = new Map(data?.watchlist.map((row) => [row.symbol, row]));
  const items: WatchQuote[] = watchlist.slice(0, limit).map((item) => bySymbol.get(item.symbol) ?? { ...item, price: null, change_pct: null, amount_yuan: null, quote_date: null, available: false, is_today: false });
  return <section className="home-watch-panel" aria-label="自选行情">
    <div className="home-section-heading"><div><h2>自选行情 <small>{watchlist.length}</small></h2><span>{data ? new Date(data.fetched_at).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }) + " 更新" : loading ? "正在获取报价" : "本地自选"}</span></div>
      <div className="home-section-actions"><button className="mobile-header-action" type="button" aria-label="刷新自选行情" disabled={loading || !watchlist.length} onClick={() => void refresh()}><RefreshCw size={16} className={loading ? "spin" : ""} /></button>{onOpenAll && <SectionLink label="全部" onClick={onOpenAll} />}</div>
    </div>
    {error && <p className="home-inline-error">{error}{data ? "，保留上次快照。" : "，可手动重试。"}</p>}
    {items.length ? <WatchQuoteRows items={items} onOpenSymbol={onOpenSymbol} /> : <div className="home-watch-empty">搜索股票后点星标加入自选，这里直接显示价格、涨跌和成交额。</div>}
    {data && watchlist.length > data.watchlist.length && <p className="home-inline-error">部分自选未进入本次报价范围，显示 —。</p>}
  </section>;
}
