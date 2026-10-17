import { ArrowDownUp, RefreshCw, Save, SlidersHorizontal, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { WebSelect } from "./controls";
import {
  DEFAULT_FILTERS, evaluate, FILTER_KEYS, FILTER_LABELS, parsePresets, PRESETS_MAX, SORT_LABELS, sortCandidates, upsertPreset,
  type Filters, type FilterKey, type Metrics, type Mode, type Preset, type Range, type SortKey,
} from "./candidateView";

type Universe = "watch" | "position";

type SymbolRow = { symbol: string; name: string; group?: string };
type WatchResult = { items: SymbolRow[] };
type ReviewResult = { positions: Array<SymbolRow & { open_shares: number }> };
type Evidence = { field: string; value: number | string | null; unit: string };
type QuoteResult = { envelope: { symbol: string; evidence: Evidence[]; extra?: Record<string, unknown> } };

type Candidate = SymbolRow & { price: number | null; metrics: Metrics; quoteFailed: boolean };

const PRESET_KEY = "vibe.candidate.presets";
const NO_GROUP = "未分组";

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

const EMPTY_METRICS: Metrics = { pe: null, pb: null, marketCap: null, amount: null, turnover: null, changePct: null };

function readPresets(): Preset[] {
  try { return parsePresets(window.localStorage.getItem(PRESET_KEY)); } catch { return []; }
}
function writePresets(list: Preset[]) {
  try { window.localStorage.setItem(PRESET_KEY, JSON.stringify(list)); } catch { /* 存不了就只在本次有效 */ }
}

export function CandidatePanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [universe, setUniverse] = useState<Universe>("watch");
  const [group, setGroup] = useState("");
  const [filters, setFilters] = useState<Filters>(DEFAULT_FILTERS);
  const [mode, setMode] = useState<Mode>("all");
  const [sort, setSort] = useState<{ key: SortKey; dir: "asc" | "desc" }>({ key: "hits", dir: "desc" });
  const [items, setItems] = useState<Candidate[]>([]);
  const [presets, setPresets] = useState<Preset[]>(readPresets);
  const [presetName, setPresetName] = useState("");
  const [message, setMessage] = useState("");
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
          const capRaw = number(quote.envelope, "total_market_cap");
          const amountRaw = number(quote.envelope, "amount");
          const sourceCapUnit = quote.envelope.evidence.find((field) => field.field === "total_market_cap")?.unit;
          const amountUnit = quote.envelope.evidence.find((field) => field.field === "amount")?.unit;
          return {
            ...item,
            name: item.name || String(quote.envelope.extra?.name ?? item.symbol),
            price: number(quote.envelope, "price"),
            quoteFailed: false,
            metrics: {
              pe: number(quote.envelope, "pe_ttm"),
              pb: number(quote.envelope, "pb"),
              marketCap: capRaw === null ? null : sourceCapUnit === "亿元" ? capRaw : capRaw / 100_000_000,
              amount: amountRaw === null ? null : amountUnit === "万元" ? amountRaw / 10_000 : amountRaw / 100_000_000,
              turnover: number(quote.envelope, "turnover_pct"),
              changePct: number(quote.envelope, "change_pct"),
            },
          };
        } catch {
          return { ...item, price: null, metrics: EMPTY_METRICS, quoteFailed: true };
        }
      });
      setItems(enriched);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { setGroup(""); void load(); }, [universe]);

  const groups = useMemo(() => Array.from(new Set(items.map((i) => i.group ?? "").filter(Boolean))).sort((a, b) => a.localeCompare(b, "zh-CN")), [items]);
  const hasUngrouped = groups.length > 0 && items.some((i) => !i.group);
  const pool = useMemo(() => items.filter((i) => universe !== "watch" || group === "" || (group === NO_GROUP ? !i.group : i.group === group)), [items, universe, group]);

  // 条件改动即时重新判断,不用重新取行情
  const grouped = useMemo(() => {
    const rows = pool.map((item) => ({ ...item, evaluation: evaluate(item.metrics, filters, mode) }));
    const sorted = sortCandidates(rows, sort.key, sort.dir);
    return {
      met: sorted.filter((r) => r.evaluation.status === "met"),
      missing: sorted.filter((r) => r.evaluation.status === "missing"),
      unmet: sorted.filter((r) => r.evaluation.status === "unmet"),
    };
  }, [pool, filters, mode, sort]);

  function patchFilter(key: FilterKey, patch: Partial<Range>) {
    setFilters((current) => ({ ...current, [key]: { ...current[key], ...patch } }));
  }

  function savePreset() {
    const next = upsertPreset(presets, presetName, mode, filters);
    if (!next) {
      setMessage(presetName.trim() ? `最多保存 ${PRESETS_MAX} 组条件(名称 ≤ 20 字),先删掉不用的` : "先给这组条件起个名字");
      return;
    }
    setPresets(next);
    writePresets(next);
    setMessage(`已保存「${presetName.trim()}」`);
    setPresetName("");
  }

  function applyPreset(name: string) {
    const p = presets.find((x) => x.name === name);
    if (!p) return;
    setFilters(p.filters);
    setMode(p.mode);
    setMessage(`已套用「${p.name}」`);
  }

  function deletePreset(name: string) {
    const next = presets.filter((x) => x.name !== name);
    setPresets(next);
    writePresets(next);
    setMessage(`已删除「${name}」`);
  }

  const enabledCount = FILTER_KEYS.filter((k) => filters[k].enabled).length;
  const noteFor = (r: { evaluation: ReturnType<typeof evaluate>; quoteFailed: boolean }) => {
    if (r.quoteFailed) return " · 行情没取到";
    const e = r.evaluation;
    const parts = [e.enabled ? `满足 ${e.hits}/${e.enabled}` : ""];
    if (e.failed.length) parts.push("不符合 " + e.failed.map((k) => FILTER_LABELS[k].label).join("/"));
    if (e.missing.length) parts.push("缺 " + e.missing.map((k) => FILTER_LABELS[k].label).join("/"));
    return parts.filter(Boolean).length ? " · " + parts.filter(Boolean).join(",") : "";
  };

  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title">
        <div><h1>候选池</h1><p>覆盖本机自选(可按分组)与真实持仓；缺失数据单独列出，不把“未知”误判成“不符合”。条件改动立即重新判断，不用重新取行情。</p></div>
        <button type="button" className="mobile-delete" onClick={() => void load()} disabled={loading} aria-label="刷新候选池">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="discovery-tabs candidate-universe-tabs">
        <button type="button" className={universe === "watch" ? "active" : ""} onClick={() => setUniverse("watch")}>自选池</button>
        <button type="button" className={universe === "position" ? "active" : ""} onClick={() => setUniverse("position")}>持仓池</button>
      </div>

      {universe === "watch" && groups.length > 0 && (
        <div className="candidate-groups" role="tablist" aria-label="自选分组">
          <button type="button" role="tab" aria-selected={group === ""} className={group === "" ? "active" : ""} onClick={() => setGroup("")}>全部 {items.length}</button>
          {groups.map((g) => <button type="button" role="tab" key={g} aria-selected={group === g} className={group === g ? "active" : ""} onClick={() => setGroup(g)}>{g} {items.filter((i) => i.group === g).length}</button>)}
          {hasUngrouped && <button type="button" role="tab" aria-selected={group === NO_GROUP} className={group === NO_GROUP ? "active" : ""} onClick={() => setGroup(NO_GROUP)}>{NO_GROUP} {items.filter((i) => !i.group).length}</button>}
        </div>
      )}

      <article className="mobile-quote-card candidate-filter-card">
        <div className="mobile-card-label"><SlidersHorizontal size={13} /> 筛选条件({enabledCount} 项启用)</div>
        <div className="candidate-mode" role="radiogroup" aria-label="条件组合方式">
          <button type="button" role="radio" aria-checked={mode === "all"} className={mode === "all" ? "active" : ""} onClick={() => setMode("all")}>满足全部</button>
          <button type="button" role="radio" aria-checked={mode === "any"} className={mode === "any" ? "active" : ""} onClick={() => setMode("any")}>满足任一</button>
        </div>
        <div className="candidate-filter-grid">
          {FILTER_KEYS.map((key) => {
            const range = filters[key];
            const { label, unit } = FILTER_LABELS[key];
            return (
              <label key={key} className={range.enabled ? "enabled" : ""}>
                <span><input type="checkbox" checked={range.enabled} onChange={(event) => patchFilter(key, { enabled: event.target.checked })} />{label}</span>
                <div>
                  <input inputMode="decimal" value={range.min} onChange={(event) => patchFilter(key, { min: event.target.value })} placeholder="最小" disabled={!range.enabled} aria-label={`${label}最小`} />
                  <em>~</em>
                  <input inputMode="decimal" value={range.max} onChange={(event) => patchFilter(key, { max: event.target.value })} placeholder="最大" disabled={!range.enabled} aria-label={`${label}最大`} />
                  <small>{unit}</small>
                </div>
              </label>
            );
          })}
        </div>
        <div className="candidate-presets">
          {presets.length > 0 && (
            <div className="candidate-preset-list" aria-label="已保存的条件组合">
              {presets.map((p) => <span key={p.name}><button type="button" onClick={() => applyPreset(p.name)}>{p.name}</button><button type="button" aria-label={`删除组合 ${p.name}`} onClick={() => deletePreset(p.name)}><Trash2 size={12} /></button></span>)}
            </div>
          )}
          <div className="candidate-preset-save">
            <input value={presetName} maxLength={20} placeholder="给当前条件起个名字" aria-label="条件组合名称" onChange={(e) => setPresetName(e.target.value)} />
            <button type="button" onClick={savePreset}><Save size={13} /> 保存组合</button>
          </div>
        </div>
        {message && <p className="candidate-message" role="status">{message}</p>}
        <button type="button" className="candidate-run" onClick={() => void load()} disabled={loading}>{loading ? "取行情中…" : "重新取行情"}</button>
      </article>

      <div className="candidate-sort">
        <WebSelect aria-label="排序" value={sort.key} onChange={(e) => setSort({ key: e.target.value as SortKey, dir: sort.dir })}>
          {(Object.keys(SORT_LABELS) as SortKey[]).map((k) => <option key={k} value={k}>按{SORT_LABELS[k]}</option>)}
        </WebSelect>
        <button type="button" aria-label={sort.dir === "desc" ? "当前从高到低,点一下改为从低到高" : "当前从低到高,点一下改为从高到低"} onClick={() => setSort({ key: sort.key, dir: sort.dir === "desc" ? "asc" : "desc" })}>
          <ArrowDownUp size={14} />{sort.dir === "desc" ? "高→低" : "低→高"}
        </button>
        <small>空值排在最后</small>
      </div>

      {error && <div className="mobile-error discovery-message">{error}</div>}
      {!loading && pool.length === 0 && <div className="mobile-empty">当前股票池为空。</div>}

      {(["met", "missing", "unmet"] as const).map((key) => {
        const rows = grouped[key];
        if (rows.length === 0) return null;
        const title = key === "met" ? "符合" : key === "missing" ? "待补数据" : "不符合";
        return (
          <article className="mobile-quote-card candidate-group" key={key}>
            <div className="mobile-card-label">{title} · {rows.length}</div>
            <div className="mobile-watch-list">
              {rows.map((item) => (
                <button type="button" className="candidate-row" key={item.symbol} onClick={() => onOpenSymbol(item.symbol)}>
                  <div><strong>{item.name || item.symbol}</strong><span>{item.symbol}{item.group ? ` · ${item.group}` : ""}{noteFor(item)}</span></div>
                  <div><strong>{fmt(item.price)}</strong><span>PE {fmt(item.metrics.pe)} · PB {fmt(item.metrics.pb)} · 市值 {fmt(item.metrics.marketCap)}亿 · 换手 {fmt(item.metrics.turnover)}% · 涨跌 {fmt(item.metrics.changePct)}%</span></div>
                </button>
              ))}
            </div>
          </article>
        );
      })}
    </section>
  );
}
