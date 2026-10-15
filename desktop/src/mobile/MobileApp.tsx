import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  BookmarkPlus,
  ChartNoAxesCombined,
  ChevronRight,
  Database,
  Flame,
  RefreshCw,
  Search,
  Settings,
  Star,
  Trash2,
  Wifi,
} from "lucide-react";

import { MarketOverview } from "./MarketOverview";
import { ShortlinePanel } from "./ShortlinePanel";
import "./mobile.css";

type Tab = "market" | "shortline" | "watchlist" | "settings";

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

  useEffect(() => {
    void Promise.all([
      loadWatchlist(),
      api<{ ok: boolean; version: string }>("/health").then(setHealth),
    ]).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  async function openSymbol(raw: string) {
    const symbol = raw.trim().toUpperCase();
    if (!symbol) return;
    setLoading(true);
    setError("");
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
          <div className="mobile-subtitle">Android · Rust Core</div>
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
                <p>腾讯实时行情，失败自动降级东财；K 线为前复权。</p>
              </div>
            </div>

            <form
              className="mobile-search"
              onSubmit={(event) => {
                event.preventDefault();
                void openSymbol(query);
              }}
            >
              <Search size={18} />
              <input
                inputMode="numeric"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="输入 A 股代码，如 300308"
                aria-label="股票代码"
              />
              <button type="submit" disabled={loading || !query.trim()}>
                {loading ? <RefreshCw className="spin" size={17} /> : "查询"}
              </button>
            </form>

            {!quote && !loading && (
              <div className="mobile-hero-card">
                <ChartNoAxesCombined size={34} />
                <h1>选一只股票继续</h1>
                <p>行情与 K 线都由手机内 Rust Core 直接访问公开数据源，不依赖电脑端服务。</p>
                <div className="mobile-chips">
                  <span>腾讯行情</span><span>东财降级</span><span>前复权 K 线</span><span>SQLite 自选</span>
                </div>
              </div>
            )}

            {quote && (
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
            )}
          </section>
        )}

        {tab === "shortline" && <ShortlinePanel onOpenSymbol={openFromList} />}

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

        {tab === "settings" && (
          <section className="mobile-section">
            <div className="mobile-section-title">
              <div><h1>移动端状态</h1><p>Android MVP v0.1.1。</p></div>
            </div>
            <div className="mobile-info-card">
              <div><Activity size={18} /><span>Rust Core</span><strong>{health?.version ?? "检查中"}</strong></div>
              <div><Database size={18} /><span>本地数据</span><strong>SQLite</strong></div>
              <div><Wifi size={18} /><span>行情</span><strong>腾讯 + 东财</strong></div>
            </div>
            <div className="mobile-roadmap-card">
              <h2>已迁移</h2>
              <p>大盘指数、涨跌家数、涨跌分布、涨停/跌停/炸板、涨停池、简版连板天梯、单股行情、K 线、本地自选。</p>
            </div>
            <div className="mobile-roadmap-card">
              <h2>下一批</h2>
              <p>题材轮动、交易日志、个股详情增强、AI API 直连与 Android 通知。</p>
            </div>
          </section>
        )}
      </main>

      <nav className="mobile-nav" style={{ gridTemplateColumns: "repeat(4, 1fr)" }} aria-label="主导航">
        <button className={tab === "market" ? "active" : ""} onClick={() => setTab("market")}>
          <ChartNoAxesCombined size={20} /><span>行情</span>
        </button>
        <button className={tab === "shortline" ? "active" : ""} onClick={() => setTab("shortline")}>
          <Flame size={20} /><span>短线</span>
        </button>
        <button className={tab === "watchlist" ? "active" : ""} onClick={() => setTab("watchlist")}>
          <Star size={20} /><span>自选</span>
        </button>
        <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}>
          <Settings size={20} /><span>状态</span>
        </button>
      </nav>
    </div>
  );
}
