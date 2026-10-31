import { useEffect, useState } from "react";

const KEY = "vibe.mobile.recent-stocks.v1";
const EVENT = "vibe-recent-stocks";
type RecentStock = { symbol: string; name: string };
function read(): RecentStock[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((item): item is RecentStock => Boolean(item && typeof item.symbol === "string" && /^(?:(?:SH|SZ|BJ))?\d{6}$/.test(item.symbol) && typeof item.name === "string")).slice(0, 6) : [];
  } catch { return []; }
}
export function recordRecentStock(symbol: string, name: string) {
  if (!/^(?:(?:SH|SZ|BJ))?\d{6}$/.test(symbol)) return;
  try {
    localStorage.setItem(KEY, JSON.stringify([{ symbol, name: name.slice(0, 40) }, ...read().filter(item => item.symbol !== symbol)].slice(0, 6)));
    window.dispatchEvent(new Event(EVENT));
  } catch { /* Optional local history. */ }
}
export function RecentStocks({ onOpen }: { onOpen: (symbol: string) => void }) {
  const [items, setItems] = useState<RecentStock[]>(read);
  useEffect(() => {
    const update = () => setItems(read());
    window.addEventListener(EVENT, update);
    window.addEventListener("storage", update);
    return () => { window.removeEventListener(EVENT, update); window.removeEventListener("storage", update); };
  }, []);
  return <section className="recent-stocks" aria-label="最近查看">
    <div className="home-section-heading"><h2>最近查看</h2>{items.length > 0 && <button type="button" onClick={() => { try { localStorage.removeItem(KEY); } catch { /* Optional. */ } setItems([]); window.dispatchEvent(new Event(EVENT)); }}>清除记录</button>}</div>
    {items.length ? <div className="recent-stock-list">{items.map(item => <button type="button" key={item.symbol} onClick={() => onOpen(item.symbol)}><strong>{item.name || item.symbol}</strong><span>{item.symbol}</span></button>)}</div> : <p>查看过的股票会显示在这里，可随时清除。</p>}
  </section>;
}
