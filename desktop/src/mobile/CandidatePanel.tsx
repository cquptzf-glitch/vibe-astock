import { RefreshCw, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type Universe = "watch" | "position";

type SymbolRow = { symbol: string; name: string };
type WatchResult = { items: SymbolRow[] };
type ReviewResult = { positions: Array<SymbolRow & { open_shares: number }> };
type Evidence = { field: string; value: number | string | null; unit: string };
type QuoteResult = { envelope: { symbol: string; evidence: Evidence[]; extra?: Record<string, unknown> } };

type Candidate = SymbolRow & {
  price: number | null;
  pe: number | null;
  pb: number | null;
  marketCap: number | null;
  amount: number | null;
  turnover: number | null;
  changePct: number | null;
  missing: string[];
  met: boolean;
};

type Range = { enabled: boolean; min: string; max: string };

type Filters = {
  pe: Range;
  pb: Range;
  marketCap: Range;
  amount: Range;
  turnover: Range;
};

const DEFAULT_FILTERS: Filters = {
  pe: { enabled: true, min: "0", max: "40" },
  pb: { enabled: false, min: "0", max: "6" },
  marketCap: { enabled: false, min: "20", max: "3000" },
  amount: { enabled: false, min: "1", max: "" },
  turnover: { enabled: false, min: "", max: "20" },
};

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
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as T;
}

function number(envelope: QuoteResult["envelope"], field: string): number | null {
  const raw = envelope.evidence.find((item) => item.field === field)?.value;
  if (raw === null || raw === undefined || raw === "") return null;
  const parsed = typeof raw === "number" ? raw : Number(raw);
  return Number.isFinite(parsed) ? parsed : null;
}

function inRange(value: number | null, range: Range): { pass: boolean; missing: boolean } {
  if (!range.enabled) return { pass: true, missing: false };
  if (value === null) return { pass: false, missing: true };
  const min = range.min.trim() === "" ? null : Number(range.min);
  const max = range.max.trim() === "" ? null : Number(range.max);
  if (min !== null && Number.isFinite(min) && value < min) return { pass: false, missing: false };
  if (max !== null && Number.isFinite(max) && value > max) return { pass: false, missing: false };
  return { pass: true, missing: false };
}

async function mapLimited<T, R>(items: T[], limit: number, task: (item: T) => Promise<R>): Promise<R[]> {
  const result = new Array<R>(items.length);
  let cursor = 0;
  async function worker() {
    while (true) {
      const index = cursor++;
      if (index >= items.length) return;
      result[index] = await task(items[index]!);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, () => worker()));
  return result;
}

function fmt(value: number | null, digits = 2): string {
  return value === null ? "—" : value.toLocaleString("zh-CN", { maximumFractionDigits: digits });
}

export function CandidatePanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [universe, setUniverse] = useState<Universe>("watch");
  const [filters, setFilters] = useState<Filters>(DEFAULT_FILTERS);
  const [items, setItems] = useState<Candidate[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    setLoading(true);
    setError("");
    try {
      const symbols: SymbolRow[] = universe === "watch"
        ? (await api<WatchResult>("/mobile/watchlist")).items
        : (await api<ReviewResult>("/mobile/journal/review")).positions.map((item) => ({ symbol: item.symbol, name: item.name }));
      const unique = Array.from(new Map(symbols.map((item) => [item.symbol, item])).values());
      const enriched = await mapLimited(unique, 6, async (item): Promise<Candidate> => {
        try {
          const quote = await api<QuoteResult>("/fetch", {
            method: "POST",
            body: JSON.stringify({ endpoint: "tx_quote", symbol: item.symbol, refresh: true }),
          });
          const pe = number(quote.envelope, "pe_ttm");
          const pb = number(quote.envelope, "pb");
          const capRaw = number(quote.envelope, "total_market_cap");
          const amountRaw = number(quote.envelope, "amount");
          const sourceCapUnit = quote.envelope.evidence.find((field) => field.field === "total_market_cap")?.unit;
          const amountUnit = quote.envelope.evidence.find((field) => field.field === "amount")?.unit;
          const marketCap = capRaw === null ? null : sourceCapUnit === "亿元" ? capRaw : capRaw / 100_000_000;
          const amount = amountRaw === null ? null : amountUnit === "万元" ? amountRaw / 10_000 : amountRaw / 100_000_000;
          const turnover = number(quote.envelope, "turnover_pct");
          const changePct = number(quote.envelope, "change_pct");
          const checks = [
            ["PE", inRange(pe, filters.pe)],
            ["PB", inRange(pb, filters.pb)],
            ["市值", inRange(marketCap, filters.marketCap)],
            ["成交额", inRange(amount, filters.amount)],
            ["换手", inRange(turnover, filters.turnover)],
          ] as const;
          return {
            ...item,
            name: item.name || String(quote.envelope.extra?.name ?? item.symbol),
            price: number(quote.envelope, "price"),
            pe,
            pb,
            marketCap,
            amount,
            turnover,
            changePct,
            missing: checks.filter(([, check]) => check.missing).map(([label]) => label),
            met: checks.every(([, check]) => check.pass || check.missing),
          };
        } catch {
          return { ...item, price: null, pe: null, pb: null, marketCap: null, amount: null, turnover: null, changePct: null, missing: ["行情"], met: true };
        }
      });
      setItems(enriched);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void load(); }, [universe]);

  const grouped = useMemo(() => ({
    met: items.filter((item) => item.met && item.missing.length === 0),
    missing: items.filter((item) => item.missing.length > 0),
    unmet: items.filter((item) => !item.met && item.missing.length === 0),
  }), [items]);

  function patchFilter(key: keyof Filters, patch: Partial<Range>) {
    setFilters((current) => ({ ...current, [key]: { ...current[key], ...patch } }));
  }

  const controls: Array<[keyof Filters, string, string]> = [
    ["pe", "PE(TTM)", "倍"],
    ["pb", "PB", "倍"],
    ["marketCap", "总市值", "亿元"],
    ["amount", "成交额", "亿元"],
    ["turnover", "换手率", "%"],
  ];

  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title">
        <div><h1>候选池</h1><p>先覆盖本机自选与真实持仓；缺失数据单独列出，不把“未知”误判成“不符合”。</p></div>
        <button type="button" className="mobile-delete" onClick={() => void load()} disabled={loading} aria-label="刷新候选池">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="discovery-tabs candidate-universe-tabs">
        <button type="button" className={universe === "watch" ? "active" : ""} onClick={() => setUniverse("watch")}>自选池</button>
        <button type="button" className={universe === "position" ? "active" : ""} onClick={() => setUniverse("position")}>持仓池</button>
      </div>

      <article className="mobile-quote-card candidate-filter-card">
        <div className="mobile-card-label"><SlidersHorizontal size={13} /> 筛选条件</div>
        <div className="candidate-filter-grid">
          {controls.map(([key, label, unit]) => {
            const range = filters[key];
            return (
              <label key={key} className={range.enabled ? "enabled" : ""}>
                <span><input type="checkbox" checked={range.enabled} onChange={(event) => patchFilter(key, { enabled: event.target.checked })} />{label}</span>
                <div>
                  <input inputMode="decimal" value={range.min} onChange={(event) => patchFilter(key, { min: event.target.value })} placeholder="最小" disabled={!range.enabled} />
                  <em>~</em>
                  <input inputMode="decimal" value={range.max} onChange={(event) => patchFilter(key, { max: event.target.value })} placeholder="最大" disabled={!range.enabled} />
                  <small>{unit}</small>
                </div>
              </label>
            );
          })}
        </div>
        <button type="button" className="candidate-run" onClick={() => void load()} disabled={loading}>{loading ? "筛选中…" : "按当前条件重新筛选"}</button>
      </article>

      {error && <div className="mobile-error discovery-message">{error}</div>}
      {!loading && items.length === 0 && <div className="mobile-empty">当前股票池为空。</div>}

      {(["met", "missing", "unmet"] as const).map((group) => {
        const rows = grouped[group];
        if (rows.length === 0) return null;
        const title = group === "met" ? "符合" : group === "missing" ? "待补数据" : "不符合";
        return (
          <article className="mobile-quote-card candidate-group" key={group}>
            <div className="mobile-card-label">{title} · {rows.length}</div>
            <div className="mobile-watch-list">
              {rows.map((item) => (
                <button type="button" className="candidate-row" key={item.symbol} onClick={() => onOpenSymbol(item.symbol)}>
                  <div><strong>{item.name || item.symbol}</strong><span>{item.symbol}{item.missing.length ? ` · 缺 ${item.missing.join("/")}` : ""}</span></div>
                  <div><strong>{fmt(item.price)}</strong><span>PE {fmt(item.pe)} · PB {fmt(item.pb)} · 市值 {fmt(item.marketCap)}亿 · 换手 {fmt(item.turnover)}%</span></div>
                </button>
              ))}
            </div>
          </article>
        );
      })}
    </section>
  );
}
