import { usePageActive } from "./PageActivity.ts";
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import type { ProviderFailure } from "./discoveryData";

/** /mobile/shortline/emotion 的返回。比率都是 0~1，涨跌幅是百分数；缺值是 null，不补 0。 */
export interface Unavailable { available: false; reason: string }
export interface Coverage { sample: number; expected_sample: number; coverage_rate: number | null; partial: boolean }
export interface Tier { base: number; promoted: number; rate: number | null }
export interface EmotionStock {
  code: string; name: string; board: string; boards: number; label: string; rebound: boolean;
  ret: number | null; close: number | null; amount: number | null; float_cap: number | null; turnover: number | null;
  seal_fund: number | null; first_seal: string | null; last_seal: string | null; broken_times: number; sector: string;
  prev_boards?: number | null;
}
export interface PoolRow {
  code: string; name: string; board: string; ret: number | null; close: number | null; amount: number | null;
  turnover: number | null; limit_price: number | null; amplitude: number | null; seal_fund: number | null;
  first_seal: string | null; last_seal: string | null; broken_times: number; consec_dt: number; open_times: number;
  sector: string; prev_boards: number | null;
}
export interface FellStock { code: string; name: string; prev_boards: number; prev_label: string; outcome: string; ret: number | null; sector: string }
export interface LadderLevel { boards: number; stocks: EmotionStock[]; fell: FellStock[]; base: number | null; promoted: number | null; rate: number | null }
export interface LedgerEvent {
  tag: string; code: string; name: string; board: string; boards: number; sector: string; ret: number | null;
  first_seal: string | null; last_seal: string | null; broken_times: number; note: string; prev_boards: number | null;
}
export interface ThemeRow {
  sector: string; limit_up: number; first_boards: number; consec_boards: number; highest: number; broken: number;
  broken_rate: number | null; first_seal: string | null; last_seal: string | null;
  prev_limit_up: number | null; continued: number | null; continuation_rate: number | null;
  names: { code: string; name: string; boards: number; label: string; first_seal: string | null; broken_times: number }[];
}
export interface FeedbackRow { tier: string; boards: number; 合计: number; 晋级率: number | null; [result: string]: string | number | null }
export interface BoardStat {
  board: string; limit_up: number; highest: number; broken: number | null; limit_down: number | null;
  prev_base: number | null; promoted: number | null; promotion_rate: number | null;
}
type Maybe<T> = (T & { available: true }) | Unavailable;

export interface EmotionData {
  status: "ok" | "partial" | "failed"; message?: string; errors: ProviderFailure[];
  date: string; prev_date: string | null; settled: boolean; source: string; fetched_at: string; source_note: string;
  summary: {
    limit_up: number; broken: number | null; limit_down: number | null; highest: number; consec_count: number;
    first_count: number; broken_rate: number | null; seal_rate: number | null;
  };
  prev_summary: { limit_up: number; highest: number; consec_count: number } | null;
  money_effect: Maybe<Coverage & { prev_date: string | null; avg: number; median: number; positive_rate: number | null; limit_up_again_rate: number | null }>;
  promotion: Maybe<{ prev_date: string; tiers: Record<string, Tier>; overall: Tier; limit_up_count: number; prev_limit_up_count: number }>;
  consec_premium: Maybe<Coverage & { avg: number; median: number; positive_rate: number | null }>;
  ladder_gap: Maybe<{ highest: number; tiers: { boards: number; count: number }[]; gaps: number[]; continuous: boolean; note: string }>;
  seal_quality: Maybe<{
    total: number; never_broken: number; never_broken_rate: number | null; opening_seconds: number; late_seal: number;
    reopened: number; avg_broken_times: number; broken_pool: number | null; broken_rate: number | null; ever_opened_rate: number | null; note: string;
  }>;
  loss_effect: Maybe<Coverage & {
    deep_loss_5_count: number; deep_loss_5_rate: number | null; deep_loss_7_count: number; limit_down_count: number;
    worst: number; market_limit_down: number | null; note: string;
  }>;
  feedback: Maybe<Coverage & { result_order: string[]; rows: FeedbackRow[]; details: { code: string; name: string; board: number; prev_tier: string; ret: number; result: string; sector: string }[]; note: string }>;
  themes: Maybe<{ themes: ThemeRow[]; theme_count: number; concentration: number | null; timeline: { slot: string; count: number }[]; faded: { sector: string; prev_limit_up: number }[]; prev_available: boolean }>;
  events: Maybe<{ count: number; prev_available: boolean; loss_events: LedgerEvent[]; gain_events: LedgerEvent[]; split_events: LedgerEvent[] }>;
  by_board: Maybe<{ prev_available: boolean; boards: BoardStat[]; note: string }>;
  ladder: Maybe<{ levels: LadderLevel[]; total_limit_up: number; highest: number; consec_count: number; prev_available: boolean; prev_note: string }>;
  first_board: Maybe<{ total_limit_up: number; first_count: number; rebound_count: number; stocks: EmotionStock[] }>;
  limit_up_rows: EmotionStock[];
  broken_rows: { available: boolean; reason?: string; rows: PoolRow[] };
  limit_down_rows: { available: boolean; reason?: string; rows: PoolRow[] };
}
export interface CycleDay { date: string; limit_up: number; highest_consec: number; broken_rate: number; score: number }
export interface CycleData {
  status: string; date: string; settled: boolean;
  cycle: Maybe<{ window: number; trough_date: string; trough_score: number; current_score: number; current_date: string; day_n: number; rising: boolean; trend: string; pctile: number | null; series: CycleDay[] }>;
}

export function fmtRate(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? "—" : Math.round(value * 1000) / 10 + "%";
}

// ------------------------------------------------------------ 复盘日:emotion 各页共用一个选择
const DATE_KEY = "vibe.emotion.date";
const dateListeners = new Set<() => void>();
let selectedDate = (() => {
  try {
    const saved = window.sessionStorage.getItem(DATE_KEY) ?? "";
    return /^\d{4}-\d{2}-\d{2}$/.test(saved) ? saved : "";
  } catch { return ""; }
})();
export function setEmotionDate(next: string) {
  selectedDate = /^\d{4}-\d{2}-\d{2}$/.test(next) ? next : "";
  try { window.sessionStorage.setItem(DATE_KEY, selectedDate); } catch { /* Optional session persistence. */ }
  for (const listener of dateListeners) listener();
}
export function useEmotionDate(): [string, (date: string) => void] {
  const date = useSyncExternalStore((listener) => { dateListeners.add(listener); return () => dateListeners.delete(listener); }, () => selectedDate, () => "");
  return [date, setEmotionDate];
}

// ------------------------------------------------------------ 同一复盘日在各页之间复用,5 分钟内不重复取
const CACHE_MS = 5 * 60_000;
const cache = new Map<string, { at: number; body: unknown }>();
export function clearEmotionCache() { cache.clear(); }

export function useCachedResource<T>(path: string) {
  const active = usePageActive();
  const cached = cache.get(path);
  const fresh = cached && Date.now() - cached.at < CACHE_MS ? cached.body as T : null;
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{ path: string; data: T | null; loading: boolean; error: string }>({ path, data: fresh, loading: !fresh, error: "" });
  useEffect(() => {
    if (!active) return;
    const hit = cache.get(path);
    if (revision === 0 && hit && Date.now() - hit.at < CACHE_MS) {
      setState({ path, data: hit.body as T, loading: false, error: "" });
      return;
    }
    let current = true;
    const controller = new AbortController();
    setState((previous) => ({ path, data: previous.path === path ? previous.data : null, loading: true, error: "" }));
    void fetch("/api" + path, { headers: { Accept: "application/json" }, signal: controller.signal })
      .then(async (response) => {
        const body = await response.json() as T & { message?: string; _cache?: { stale?: boolean; message?: string; saved_at?: string } };
        if (!response.ok) throw new Error(body.message || "数据源暂不可用，请重试");
        if (!body._cache?.stale) cache.set(path, { at: Date.now(), body });
        if (current) setState({ path, data: body, loading: false, error: body._cache?.stale ? `${body._cache.message ?? "旧缓存"} · 保存于 ${body._cache.saved_at ?? "时间未知"}` : "" });
      })
      .catch((error: unknown) => {
        if (current && !controller.signal.aborted) setState({ path, data: null, loading: false, error: error instanceof Error ? error.message : String(error) });
      });
    return () => { current = false; controller.abort(); };
  }, [path, revision, active]);
  const refresh = useCallback(() => { cache.delete(path); setRevision((value) => value + 1); }, [path]);
  const view = state.path === path ? state : { path, data: fresh, loading: !fresh, error: "" };
  return { ...view, refresh };
}

export function emotionPath(date: string) {
  return "/mobile/shortline/emotion" + (date ? "?date=" + date : "");
}

export function useEmotion() {
  const [date, setDate] = useEmotionDate();
  const result = useCachedResource<EmotionData>(emotionPath(date));
  return { ...result, date, setDate };
}

export function chinaToday(now = new Date()): string {
  return new Date(now.getTime() + 8 * 3600_000).toISOString().slice(0, 10);
}
