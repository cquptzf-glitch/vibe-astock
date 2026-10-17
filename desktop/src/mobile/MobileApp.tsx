import { lazy, Suspense, useEffect, useState, type ComponentType, type LazyExoticComponent, type ReactNode } from "react";
import { ArrowLeft, BookmarkPlus, ChevronRight, RefreshCw, Search, Star } from "lucide-react";

import { clearAiPage, PERSONAL_PAGES, queueAiPrompt } from "./aiPageContext";
import { EmotionTodayPanel, FirstBoardPanel, HomeEmotionBrief, LadderPanel, LimitUpTablePanel, MarketFactsPanel, ThemesPanel } from "./EmotionPanels";
import { MarketOverview } from "./MarketOverview";
import { HomeShortcuts } from "./MobileHeader";
import { WatchlistBoard } from "./WatchlistPanel";
import { DialogHost } from "./controls";
import { MonitorPanel } from "./MonitorPanel";
import { WatchQuotesPanel } from "./WatchQuotesPanel";
import { MessagesPanel, refreshMessages } from "./MessagesPanel";
import { FunctionDirectory, GroupTabs, WorkspaceNav } from "./WorkspaceNavigation";
import { ResearchPanel } from "./ResearchPanel";
import { StockDetailPanel } from "./StockDetailPanel";
import { StockChart } from "./KlineChart";
import { QuoteBoard } from "./QuoteBoard";
import { StockInsightsPanel } from "./StockInsightsPanel";
import { useMobileNavigation } from "./useMobileNavigation";
import type { MobilePage } from "./mobileRoutes";
import "./mobile.css";
import "./mobile-presentation.css";
import "./compact.css";
import "./discovery.css";
import "./journal-workspace.css";
import "./library.css";
import "./shortline.css";

// 路由分包:首页和个股页以外的页面第一次进入时才加载(本机文件,几毫秒)。名字对应各文件里的具名导出
function lazyPanel<M, K extends keyof M>(load: () => Promise<M>, name: K): LazyExoticComponent<M[K] extends ComponentType<infer P> ? ComponentType<P> : never> {
  return lazy(() => load().then((m) => ({ default: m[name] as never })));
}
const AiWorkspace = lazyPanel(() => import("./AiWorkspace"), "AiWorkspace");
const BacktestPanel = lazyPanel(() => import("./BacktestPanel"), "BacktestPanel");
const BoardDetailPanel = lazyPanel(() => import("./BoardDetailPanel"), "BoardDetailPanel");
const CalendarPanel = lazyPanel(() => import("./CalendarPanel"), "CalendarPanel");
const CandidatePanel = lazyPanel(() => import("./CandidatePanel"), "CandidatePanel");
const DailyReviewPanel = lazyPanel(() => import("./DailyReviewPanel"), "DailyReviewPanel");
const GpuRentPanel = lazyPanel(() => import("./GpuRentPanel"), "GpuRentPanel");
const JournalPanelV2 = lazyPanel(() => import("./JournalPanelV2"), "JournalPanelV2");
const JournalReviewPanel = lazyPanel(() => import("./JournalReviewPanel"), "JournalReviewPanel");
const LibraryPanel = lazyPanel(() => import("./LibraryPanel"), "LibraryPanel");
const StructurePanel = lazyPanel(() => import("./StructurePanel"), "StructurePanel");
const JournalAttributionPanel = lazyPanel(() => import("./JournalAttributionPanel"), "JournalAttributionPanel");
const JournalEnvPanel = lazyPanel(() => import("./JournalEnvPanel"), "JournalEnvPanel");
const JournalExcursionPanel = lazyPanel(() => import("./JournalExcursionPanel"), "JournalExcursionPanel");
const JournalModesPanel = lazyPanel(() => import("./JournalModesPanel"), "JournalModesPanel");
const JournalProblemsPanel = lazyPanel(() => import("./JournalProblemsPanel"), "JournalProblemsPanel");
const JournalRiskPanel = lazyPanel(() => import("./JournalRiskPanel"), "JournalRiskPanel");
const SamplesPanel = lazyPanel(() => import("./SamplesPanel"), "SamplesPanel");
const NewsPanel = lazyPanel(() => import("./NewsPanel"), "NewsPanel");
const ReminderPanel = lazyPanel(() => import("./ReminderPanel"), "ReminderPanel");
const RiskPanel = lazyPanel(() => import("./RiskPanel"), "RiskPanel");
const RotationPanel = lazyPanel(() => import("./RotationPanel"), "RotationPanel");
const SectorPanel = lazyPanel(() => import("./SectorPanel"), "SectorPanel");
const SettingsPanel = lazyPanel(() => import("./SettingsPanel"), "SettingsPanel");
const PortfolioPanel = lazyPanel(() => import("./PortfolioPanel"), "PortfolioPanel");
const MarketPanel = lazyPanel(() => import("./MarketPanel"), "MarketPanel");
const IntradayPanel = lazyPanel(() => import("./IntradayPanel"), "IntradayPanel");
const HistoryPanel = lazyPanel(() => import("./HistoryPanels"), "HistoryPanel");
const VerifyPanel = lazyPanel(() => import("./HistoryPanels"), "VerifyPanel");
const WeeklyPanel = lazyPanel(() => import("./HistoryPanels"), "WeeklyPanel");

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

function isDirectSymbol(value: string): boolean {
  const normalized = value.trim().toUpperCase().replace(/\s+/g, "");
  return /^\d{6}$/.test(normalized)
    || /^(SH|SZ|BJ)\d{6}$/.test(normalized)
    || /^\d{6}\.(SH|SZ|BJ)$/.test(normalized);
}

export function MobileApp() {
  const { route, navigate, back } = useMobileNavigation();
  const page = route.page;
  const isStock = page === "stock";
  const go = (next: MobilePage) => navigate({ page: next });
  const askAi = (question: string) => { queueAiPrompt(question); go("ai"); };
  // 个人页面(成交、持仓、盈亏)不给 AI:进入时清掉上一页留下的「附带当前页面」登记
  useEffect(() => { if (PERSONAL_PAGES.has(route.page)) clearAiPage(); }, [route.page]);
  const [query, setQuery] = useState("");
  const [activeSymbol, setActiveSymbol] = useState("");
  const [quote, setQuote] = useState<Envelope | null>(null);
  const [klineRows, setKlineRows] = useState<unknown[][]>([]);
  const [watchlist, setWatchlist] = useState<WatchItem[]>([]);
  const [suggestions, setSuggestions] = useState<SymbolSearchItem[]>([]);
  const [searching, setSearching] = useState(false);
  const [loading, setLoading] = useState(false);
  const [quoteRevision, setQuoteRevision] = useState(0);
  const [loadedSymbol, setLoadedSymbol] = useState("");
  const [error, setError] = useState("");
  const [, setHealth] = useState<{ ok: boolean; version: string } | null>(null);
  const [unreadMessages, setUnreadMessages] = useState(0);
  // 启动时生成一次当天该有的消息(盘前提醒 / 收盘摘要 / 自选异动),取不到就算了,不打扰
  useEffect(() => { void refreshMessages().then((out) => setUnreadMessages(out.list.unread)).catch(() => undefined); }, []);

  const currentName = String(quote?.extra?.name ?? "");
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

  useEffect(() => {
    const symbol = route.symbol;
    let cancelled = false;
    setQuote(null);
    setKlineRows([]);
    setError("");
    setLoading(false);
    setActiveSymbol(symbol ?? "");
    if (!symbol) return;
    setQuery(symbol);
    setSuggestions([]);
    setLoading(true);
    void (async () => {
      try {
        const quoteResult = await api<FetchResult>("/fetch", {
          method: "POST",
          body: JSON.stringify({ endpoint: "tx_quote", symbol, refresh: true }),
        });
        if (cancelled) return;
        setQuote(quoteResult.envelope);
        setLoadedSymbol(symbol);
        setActiveSymbol(quoteResult.envelope.symbol);
        setQuery(quoteResult.envelope.symbol);
        try {
          const klineResult = await api<FetchResult>("/fetch", {
            method: "POST",
            body: JSON.stringify({ endpoint: "kline", symbol, args: { bars: 100 }, refresh: true }),
          });
          if (cancelled) return;
          const rows = klineResult.envelope.extra?.rows;
          setKlineRows(Array.isArray(rows) ? rows as unknown[][] : []);
        } catch {
          if (!cancelled) setKlineRows([]);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [route.symbol, quoteRevision]);

  function openSymbol(raw: string) {
    navigate({ page: "stock", symbol: raw });
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
      const items = await searchSymbols(keyword);
      setSuggestions(items);
      const first = items[0];
      if (!first) {
        setError(`没有找到沪深北 A 股：${keyword}`);
        return;
      }
      await openSymbol(first.symbol);
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

  function openFromList(symbol: string) {
    openSymbol(symbol);
  }

  const searchBlock = <>
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
        placeholder="代码 / 名称 / 拼音，如 300308、430047、茅台"
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
  </>;

  const stockBlock = <section className="mobile-section">
    {searchBlock}
    {loading && <div className="mobile-empty" role="status">正在加载 {route.symbol} 行情…</div>}
    {!loading && error && (
      <button className="mobile-back" type="button" onClick={() => setQuoteRevision((value) => value + 1)}>重新加载行情</button>
    )}
    {quote && loadedSymbol === route.symbol && (
      <>
        <article className="mobile-quote-card">
          <div className="mobile-quote-head">
            <div>
              <div className="mobile-symbol-line">
                <strong>{currentName || activeSymbol}</strong>
                <span>{quote.market} · {quote.symbol}</span>
              </div>
            </div>
            <button className={`mobile-star ${isWatched ? "active" : ""}`} onClick={() => void toggleWatch()} aria-label="自选">
              {isWatched ? <Star size={22} fill="currentColor" /> : <BookmarkPlus size={22} />}
            </button>
          </div>
        </article>

        <QuoteBoard key={"qb-" + activeSymbol} symbol={activeSymbol} />
        <StockChart key={"kc-" + activeSymbol} symbol={activeSymbol} />
        <StockDetailPanel quote={quote} rows={klineRows} />
        <ResearchPanel symbol={activeSymbol} />
        <StockInsightsPanel key={activeSymbol} symbol={activeSymbol} onOpenBoard={(board) => navigate({ page: "board", board })} />
      </>
    )}
  </section>;

  const watchBlock = <section className="mobile-section">
    <div className="mobile-section-title">
      <div><h1>我的自选</h1><p>自选保存在手机本地；点星标加入，在这里查看报价与管理。</p></div>
      <span>{watchlist.length} 只</span>
    </div>
    <WatchlistBoard onOpenSymbol={openFromList} onChanged={() => void loadWatchlist()} />
  </section>;

  function renderPage(): ReactNode {
    switch (page) {
      case "market": return <section className="mobile-section">
        <MarketOverview />
        <HomeEmotionBrief onOpen={() => go("emotion")} />
        <HomeShortcuts onNavigate={go} />
        <WatchQuotesPanel watchlist={watchlist} limit={6} onOpenSymbol={openFromList} onOpenAll={() => go("watch")} />
        <div className="mobile-section-title mobile-stock-search-heading">
          <div><h1>个股行情</h1><p>支持沪深北代码、名称和拼音搜索；腾讯实时行情失败时自动降级东财。</p></div>
        </div>
        {searchBlock}
      </section>;
      case "functions": return <FunctionDirectory onNavigate={go} />;
      case "my": return <FunctionDirectory onNavigate={go} group="my" />;
      case "research": return <section className="mobile-section research-home">
        <div className="mobile-section-title"><div><h1>个股深挖</h1><p>行情、K 线结构、公司资料、财务、公告、研报、资金流、量价观察与所属板块。</p></div></div>
        {searchBlock}
        <WatchQuotesPanel watchlist={watchlist} limit={8} onOpenSymbol={openFromList} onOpenAll={() => go("watch")} />
      </section>;
      case "stock": return stockBlock;
      case "monitor": return <MonitorPanel onOpenSymbol={openFromList} />;
      case "intraday": return <IntradayPanel />;
      case "risk": return <RiskPanel onOpenSymbol={openFromList} />;
      case "watch": return watchBlock;
      case "hold": return <section className="mobile-section"><PortfolioPanel onOpenSymbol={openFromList} /></section>;
      case "emotion": return <EmotionTodayPanel onNavigate={go} onAskAi={askAi} />;
      case "ladder": return <LadderPanel onOpenSymbol={openFromList} onAskAi={askAi} />;
      case "limitUp": return <LimitUpTablePanel onOpenSymbol={openFromList} onAskAi={askAi} />;
      case "firstBoard": return <FirstBoardPanel onOpenSymbol={openFromList} onAskAi={askAi} />;
      case "seal": return <MarketFactsPanel onOpenSymbol={openFromList} onAskAi={askAi} />;
      case "themes": return <ThemesPanel onOpenSymbol={openFromList} onAskAi={askAi} />;
      case "marketReview": return <MarketPanel onOpenSymbol={openFromList} />;
      case "structure": return <StructurePanel />;
      case "samples": return <SamplesPanel />;
      case "weekly": return <WeeklyPanel onOpenSymbol={openFromList} onAskAi={askAi} />;
      case "history": return <HistoryPanel onAskAi={askAi} />;
      case "verify": return <VerifyPanel />;
      case "daily": return <DailyReviewPanel onOpenSymbol={openFromList} />;
      case "records": return <JournalPanelV2 onOpenSymbol={openFromList} />;
      case "journalRisk": return <JournalRiskPanel />;
      case "journalModes": return <JournalModesPanel />;
      case "journalExcursion": return <JournalExcursionPanel />;
      case "journalEnv": return <JournalEnvPanel />;
      case "journalAttribution": return <JournalAttributionPanel />;
      case "journalProblems": return <JournalProblemsPanel />;
      case "review": return <JournalReviewPanel onOpenSymbol={openFromList} />;
      case "rotation": return <RotationPanel onOpenBoard={(board) => navigate({ page: "board", board })} />;
      case "board": return <BoardDetailPanel key={route.board} code={route.board!} onOpenSymbol={openFromList} />;
      case "sector": return <SectorPanel />;
      case "news": return <NewsPanel />;
      case "signals": return <GpuRentPanel />;
      case "candidate": return <CandidatePanel onOpenSymbol={openFromList} />;
      case "backtest": return <BacktestPanel />;
      case "ai": return <AiWorkspace />;
      case "messages": return <MessagesPanel onNavigate={go} onUnread={setUnreadMessages} />;
      case "calendar": return <CalendarPanel />;
      case "reminders": return <ReminderPanel />;
      case "library":
      case "notes": return <LibraryPanel kind="note" />;
      case "reports": return <LibraryPanel kind="report" />;
      case "inbox": return <LibraryPanel kind="inbox" />;
      case "settings": return <SettingsPanel onOpenAi={() => go("ai")} onOpenReminders={() => go("reminders")} />;
    }
  }

  const showBack = isStock || page === "board" || page === "functions";
  return (
    <div className="mobile-app">
      <main className="mobile-main" data-page={page}>
        <GroupTabs page={page} onNavigate={go} />
        {showBack && (
          <button className="mobile-back" type="button" onClick={back}>
            <ArrowLeft size={18} /> 返回
          </button>
        )}
        {error && <div className="mobile-error">{error}</div>}
        <Suspense fallback={<div className="mobile-empty" role="status">加载中…</div>}>{renderPage()}</Suspense>
      </main>

      <WorkspaceNav page={page} onNavigate={go} unreadMessages={unreadMessages} />
      <DialogHost />
    </div>
  );
}
