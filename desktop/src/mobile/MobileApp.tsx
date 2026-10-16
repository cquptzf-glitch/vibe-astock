import { useEffect, useMemo, useState } from "react";
import {
  BookmarkPlus,
  ChartNoAxesCombined,
  ChevronRight,
  Database,
  Flame,
  Layers3,
  NotebookPen,
  RefreshCw,
  Search,
  Star,
  Trash2,
  Wifi,
} from "lucide-react";

import { JournalWorkspace } from "./JournalWorkspace";
import { MarketOverview } from "./MarketOverview";
import { RotationPanel } from "./RotationPanel";
import { ShortlinePanel } from "./ShortlinePanel";
import { StockDetailPanel } from "./StockDetailPanel";
import "./mobile.css";

type Tab = "market" | "shortline" | "rotation" | "watchlist" | "journal";

interface Evidence {
  field: string;
  value: number | string | null;
  unit: string;
}

interface Envelope {
  symbol: string;
  market: string;
  status: string;
  primary_source: string | null;
  evidence: Evidence[];
  extra?: Record<string, unknown>;
}

interface FetchResult {
  envelope: Envelope;
  duration_ms: number;
}

interface WatchItem {
  symbol: string;
  name: string;
  created_at: string;
  updated_at: string;
}

interface WatchListResult {
  items: WatchItem[];
}

interface SymbolSearchItem {
  symbol: string;
  name: string;
  market: string;
  pinyin: string;
  category: string;
  source: string;
}

interface SymbolSearchResult {
  query: string;
  items: SymbolSearchItem[];
  count: number;
  source: string;
  fetched_at: string;
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) {
    throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  }
  return body as T;
}

function evidenceNumber(env: Envelope | null, field: string): number | null {
  const value = env?.evidence.find((item) => item.field === field)?.value;
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function formatNumber(value: number | null, digits = 2): string {
  return value === null
    ? "—"
    : value.toLocaleString("zh-CN", {
        maximumFractionDigits: digits,
        minimumFractionDigits: digits,
      });
}

function formatMarketCap(value: number | null, source: string | null): string {
  if (value === null) return "—";
  const yi = source === "eastmoney" ? value / 100_000_000 : value;
  return `${yi.toLocaleString("zh-CN", { maximumFractionDigits: 2 })} 亿`;
}

function isDirectSymbol(value: string): boolean {
  const normalized = value.trim().toUpperCase().replace(/\s+/g, "");
  return /^\d{6}$/.test(normalized)
    || /^(SH|SZ)[.:]?\d{6}$/.test(normalized)
    || /^\d{6}\.(SH|SZ)$/.test(normalized);
}

function Sparkline({ rows }: { rows: unknown[][] }) {
  const points = useMemo(() => {
    const closes = rows
      .map((row) => Number(row[2]))
      .filter((value) => Number.isFinite(value));
    if (closes.length < 2) return "";
    const min = Math.min(...closes);
    const max = Math.max(...closes);
    const span = max - min || 1;
    return closes
      .map((value, index) => {
        const x = (index / (closes.length - 1)) * 100;
        const y = 34 - ((value - min) / span) * 30;
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      })
      .join(" ");
  }, [rows]);

  if (!points) return <div className="mobile-empty-chart">K 线数据不足</div>;
  return (
    <svg className="mobile-sparkline" viewBox="0 0 100 38" preserveAspectRatio="none" aria-label="近 60 日走势">
      <polyline points={points} fill="none" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

export function MobileApp() {
  const [tab, setTab] = useState<Tab>("market");
  const [query, setQuery] = useState("");
  const [activeSymbol, setActiveSymbol] = useState("");
  const [quote, setQuote] = useState<Envelope | null>(null);
  const [klineRows, setKlineRows] = useState<unknown[][]>([]);
  const [watchlist, setWatchlist] = useState<WatchItem[]>([]);
  const [suggestions, setSuggestions] = useState<SymbolSearchItem[]>([]);
  const [searching, setSearching] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [health, setHealth] = useState<{ ok: boolean; version: string } | null>(null);

  const currentName = String(quote?.extra?.name ?? "");
  const price = evidenceNumber(quote, "price");
  const changePct = evidenceNumber(quote, "change_pct");
  const pe = evidenceNumber(quote, "pe_ttm");
  const pb = evidenceNumber(quote, "pb");
  const turnover = evidenceNumber(quote, "turnover_pct");
  const marketCap = evidenceNumber(quote, "total_market_cap");
  const isWatched = Boolean(activeSymbol && watchlist.some((item) => item.symbol === activeSymbol));

  async function loadWatchlist() {
    const result = await api<WatchListResult>("/mobile/watchlist");
    setWatchlist(result.items);
  }

  async function searchSymbols(keyword: string): Promise<SymbolSearchItem[]> {
    const result = await api<SymbolSearchResult>("/mobile/symbol-search", {
      method: "POST",
      body: JSON.stringify({ query: keyword, limit: 8 }),
    });
    return result.items;
  }

  useEffect(() => {
    void Promise.all([
      loadWatchlist(),
      api<{ ok: boolean; version: string }>("/health").then(setHealth),
    ]).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  useEffect(() => {
    const keyword = query.trim();
    if (!keyword || keyword === activeSymbol || isDirectSymbol(keyword)) {
      setSuggestions([]);
      setSearching(false);
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(() => {
      setSearching(true);
      void searchSymbols(keyword)
        .then((items) => {
          if (!cancelled) setSuggestions(items);
        })
        .catch(() => {
          if (!cancelled) setSuggestions([]);
        })
        .finally(() => {
          if (!cancelled) setSearching(false);
        });
    }, 260);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, activeSymbol]);

  async function openSymbol(raw: string) {
    const symbol = raw.trim().toUpperCase();
    if (!symbol) return;
    setLoading(true);
    setError("");
    setSuggestions([]);
    try {
      const quoteResult = await api<FetchResult>("/fetch", {
        method: "POST",
        body: JSON.stringify({ endpoint: "tx_quote", symbol, refresh: true }),
      });
      setQuote(quoteResult.envelope);
      setActiveSymbol(quoteResult.envelope.symbol);
      setQuery(quoteResult.envelope.symbol);

      try {
        const klineResult = await api<FetchResult>("/fetch", {
          method: "POST",
          body: JSON.stringify({ endpoint: "kline", symbol, args: { bars: 60 }, refresh: true }),
        });
        const rows = klineResult.envelope.extra?.rows;
        setKlineRows(Array.isArray(rows) ? rows as unknown[][] : []);
      } catch {
        setKlineRows([]);
      }
    } catch (e) {
      setQuote(null);
      setKlineRows([]);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function submitSearch() {
    const keyword = query.trim();
    if (!keyword) return;
    if (isDirectSymbol(keyword)) {
      await openSymbol(keyword);
      return;
    }

    setSearching(true);
    setError("");
    try {
      const items = suggestions.length > 0 ? suggestions : await searchSymbols(keyword);
      setSuggestions(items);
      if (items.length === 0) {
        setError(`没有找到沪深 A 股：${keyword}`);
        return;
      }
      await openSymbol(items[0].symbol);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSearching(false);
    }
  }

  async function toggleWatch() {
    if (!activeSymbol) return;
    setError("");
    try {
      await api("/mobile/watchlist", {
        method: isWatched ? "DELETE" : "POST",
        body: JSON.stringify(
          isWatched
            ? { symbol: activeSymbol }
            : { symbol: activeSymbol, name: currentName },
        ),
      });
      await loadWatchlist();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function removeWatch(symbol: string) {
    try {
      await api("/mobile/watchlist", {
        method: "DELETE",
        body: JSON.stringify({ symbol }),
      });
      await loadWatchlist();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  function openFromList(symbol: string) {
    setTab("market");
    setQuery(symbol);
    void openSymbol(symbol);
  }

  return (
    <div className="mobile-app">
      <header className="mobile-header">
        <div>
          <div className="mobile-brand">Vibe AStock</div>
          <div className="mobile-subtitle">Android · Rust Core · v0.1.7</div>
        </div>
        <div className={`mobile-status ${health?.ok ? "ok" : ""}`}>
          <Wifi size={14} />
          {health?.ok ? "本地核心已就绪" : "正在连接"}
        </div>
      </header>

      <main className="mobile-main">
        {error && <div className="mobile-error">{error}</div>}

        {tab === "market" && (
          <section className="mobile-section">
            <MarketOverview />

            <div className="mobile-section-title" style={{ marginTop: 22 }}>
              <div>
                <h1>个股行情</h1>
                <p>支持代码、名称和拼音搜索；腾讯实时行情失败时自动降级东财。</p>
              </div>
            </div>

            <form
              className="mobile-search"
              onSubmit={(event) => {
                event.preventDefault();
                void submitSearch();
              }}
            >
              <Search size={18} />
              <input
                inputMode="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="代码 / 名称 / 拼音，如 300308、茅台、GZMT"
                aria-label="股票搜索"
                autoComplete="off"
              />
              <button type="submit" disabled={loading || searching || !query.trim()}>
                {loading || searching ? <RefreshCw className="spin" size={17} /> : "查询"}
              </button>
            </form>

            {suggestions.length > 0 && query.trim() !== activeSymbol && (
              <div className="mobile-watch-list" style={{ marginTop: 8 }} aria-label="股票搜索结果">
                {suggestions.map((item) => (
                  <div className="mobile-watch-row" key={`${item.market}-${item.symbol}`}>
                    <button className="mobile-watch-open" type="button" onClick={() => void openSymbol(item.symbol)}>
                      <div>
                        <strong>{item.name}</strong>
                        <span>{item.market} · {item.symbol}{item.pinyin ? ` · ${item.pinyin}` : ""}</span>
                      </div>
                      <ChevronRight size={18} />
                    </button>
                  </div>
                ))}
              </div>
            )}

            {!quote && !loading && (
              <div className="mobile-hero-card">
                <ChartNoAxesCombined size={34} />
                <h1>选一只股票继续</h1>
                <p>行情、搜索与 K 线都由手机内 Rust Core 直接访问公开数据源，不依赖电脑端服务。</p>
                <div className="mobile-chips">
                  <span>名称/拼音搜索</span><span>腾讯行情</span><span>东财降级</span><span>前复权 K 线</span><span>SQLite 自选</span>
                </div>
              </div>
            )}

            {quote && (
              <>
                <article className="mobile-quote-card">
                  <div className="mobile-quote-head">
                    <div>
                      <div className="mobile-symbol-line">
                        <strong>{currentName || activeSymbol}</strong>
                        <span>{quote.market} · {quote.symbol}</span>
                      </div>
                      <div className="mobile-price-row">
                        <span className="mobile-price">{formatNumber(price)}</span>
                        <span className={changePct !== null && changePct < 0 ? "down" : "up"}>
                          {changePct === null ? "—" : `${changePct >= 0 ? "+" : ""}${formatNumber(changePct)}%`}
                        </span>
                      </div>
                    </div>
                    <button className={`mobile-star ${isWatched ? "active" : ""}`} onClick={() => void toggleWatch()} aria-label="自选">
                      {isWatched ? <Star size={22} fill="currentColor" /> : <BookmarkPlus size={22} />}
                    </button>
                  </div>

                  <div className="mobile-chart-wrap">
                    <div className="mobile-card-label">近 60 日 · 前复权</div>
                    <Sparkline rows={klineRows} />
                  </div>

                  <div className="mobile-metrics">
                    <div><span>PE(TTM)</span><strong>{formatNumber(pe)}</strong></div>
                    <div><span>PB</span><strong>{formatNumber(pb)}</strong></div>
                    <div><span>换手率</span><strong>{turnover === null ? "—" : `${formatNumber(turnover)}%`}</strong></div>
                    <div><span>总市值</span><strong>{formatMarketCap(marketCap, quote.primary_source)}</strong></div>
                  </div>

                  <div className="mobile-source-line">
                    <Database size={14} />
                    数据源：{quote.primary_source ?? "—"} · 状态：{quote.status}
                    {quote.extra?.quote_date ? ` · ${String(quote.extra.quote_date)}` : ""}
                  </div>
                </article>

                <StockDetailPanel quote={quote} rows={klineRows} />
              </>
            )}
          </section>
        )}

        {tab === "shortline" && <ShortlinePanel onOpenSymbol={openFromList} />}
        {tab === "rotation" && <RotationPanel />}

        {tab === "watchlist" && (
          <section className="mobile-section">
            <div className="mobile-section-title">
              <div><h1>自选股</h1><p>数据保存在手机本地 SQLite。</p></div>
              <span>{watchlist.length} 只</span>
            </div>
            {watchlist.length === 0 ? (
              <div className="mobile-empty">查询股票后点右上角星标，即可加入自选。</div>
            ) : (
              <div className="mobile-watch-list">
                {watchlist.map((item) => (
                  <div className="mobile-watch-row" key={item.symbol}>
                    <button className="mobile-watch-open" onClick={() => openFromList(item.symbol)}>
                      <div><strong>{item.name || item.symbol}</strong><span>{item.symbol}</span></div>
                      <ChevronRight size={18} />
                    </button>
                    <button className="mobile-delete" onClick={() => void removeWatch(item.symbol)} aria-label={`删除 ${item.symbol}`}>
                      <Trash2 size={17} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </section>
        )}

        {tab === "journal" && <JournalWorkspace onOpenSymbol={openFromList} />}
      </main>

      <nav className="mobile-nav" style={{ gridTemplateColumns: "repeat(5, 1fr)" }} aria-label="主导航">
        <button className={tab === "market" ? "active" : ""} onClick={() => setTab("market")}>
          <ChartNoAxesCombined size={20} /><span>行情</span>
        </button>
        <button className={tab === "shortline" ? "active" : ""} onClick={() => setTab("shortline")}>
          <Flame size={20} /><span>短线</span>
        </button>
        <button className={tab === "rotation" ? "active" : ""} onClick={() => setTab("rotation")}>
          <Layers3 size={20} /><span>轮动</span>
        </button>
        <button className={tab === "watchlist" ? "active" : ""} onClick={() => setTab("watchlist")}>
          <Star size={20} /><span>自选</span>
        </button>
        <button className={tab === "journal" ? "active" : ""} onClick={() => setTab("journal")}>
          <NotebookPen size={20} /><span>日志</span>
        </button>
      </nav>
    </div>
  );
}
