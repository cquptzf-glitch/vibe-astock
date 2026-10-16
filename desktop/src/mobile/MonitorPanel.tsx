import { AlertTriangle, BriefcaseBusiness, ChevronRight, RefreshCw, Star } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import "./monitor.css";
import { usePresentation } from "./presentation";
import { useForegroundRefresh } from "./useForegroundRefresh";

type WatchRow = {
  symbol: string;
  name: string;
  price: number | null;
  change_pct: number | null;
  amount_yuan: number | null;
  limit_up: number | null;
  limit_down: number | null;
  quote_date: string | null;
  is_today: boolean;
  available: boolean;
};

type PositionRow = {
  id: number | null;
  symbol: string;
  name: string;
  open_shares: number;
  avg_cost: number | null;
  planned_stop: number | null;
  planned_target: number | null;
  price: number | null;
  change_pct: number | null;
  limit_up: number | null;
  limit_down: number | null;
  market_value: number | null;
  unrealized_pnl: number | null;
  unrealized_pct: number | null;
  quote_date: string | null;
  is_today: boolean;
  risk_state: "stop_reached" | "target_reached" | "stop_near" | "target_near" | "stale_quote" | "unquoted" | "normal";
};

type MonitorData = {
  status: "ok" | "partial";
  source: string;
  fetched_at: string;
  trading_date: string;
  watchlist: WatchRow[];
  positions: PositionRow[];
  missing: string[];
};

async function api<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, { headers: { Accept: "application/json" } });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as T;
}

function price(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : value.toLocaleString("zh-CN", { maximumFractionDigits: 3 });
}

function pct(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : `${value >= 0 ? "+" : ""}${value.toFixed(2)}%`;
}

function money(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  if (Math.abs(value) >= 100_000_000) return `${(value / 100_000_000).toFixed(2)} 亿`;
  if (Math.abs(value) >= 10_000) return `${(value / 10_000).toFixed(1)} 万`;
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 0 });
}

function amount(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : money(value);
}

const RISK_LABEL: Record<PositionRow["risk_state"], string> = {
  stop_reached: "已到止损",
  target_reached: "已到目标",
  stop_near: "接近止损",
  target_near: "接近目标",
  stale_quote: "行情非今日",
  unquoted: "暂无行情",
  normal: "正常",
};

export function MonitorPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const { refreshSeconds } = usePresentation();
  const [data, setData] = useState<MonitorData | null>(null);
  const [tab, setTab] = useState<"watch" | "position">("watch");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [samples, setSamples] = useState<Record<string, { at: number; price: number }[]>>({});

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const next = await api<MonitorData>("/mobile/monitor");
      const at = Date.now();
      const prices = [...next.watchlist, ...next.positions].filter((row) => row.is_today && row.price !== null);
      setSamples((previous) => {
        const nextSamples = { ...previous };
        for (const row of prices) {
          const history = [...(nextSamples[row.symbol] ?? []), { at, price: row.price! }]
            .filter((sample) => at - sample.at <= 5 * 60_000)
            .slice(-6);
          nextSamples[row.symbol] = history;
        }
        return nextSamples;
      });
      setData(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();

  }, [refresh]);
  useForegroundRefresh(refresh, refreshSeconds);

  const riskCount = useMemo(
    () => (data?.positions ?? []).filter((row) => ["stop_reached", "target_reached", "stop_near", "target_near"].includes(row.risk_state)).length,
    [data],
  );

  const rows = tab === "watch" ? data?.watchlist ?? [] : data?.positions ?? [];
  const move3m = (symbol: string, current: number | null): number | null => {
    if (current === null) return null;
    const history = samples[symbol] ?? [];
    const first = history.find((sample) => Date.now() - sample.at >= 150_000);
    if (!first || first.price <= 0) return null;
    return (current / first.price - 1) * 100;
  };

  return (
    <section className="monitor-panel">
      <div className="monitor-title-row">
        <div>
          <div className="monitor-kicker"><Star size={15} /> 盘中盯盘</div>
          <h1>自选 · 持仓动态</h1>
          <p>{refreshSeconds ? `前台每 ${refreshSeconds} 秒刷新；后台暂停。` : "仅手动刷新。"}价格类状态只使用今日行情。</p>
        </div>
        <button className="monitor-refresh" type="button" onClick={() => void refresh()} disabled={loading} aria-label="刷新盯盘">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && <div className="monitor-error">{error}</div>}

      <div className="monitor-tabs" role="tablist" aria-label="盯盘范围">
        <button type="button" role="tab" aria-selected={tab === "watch"} className={tab === "watch" ? "active" : ""} onClick={() => setTab("watch")}>
          <Star size={13} /> 自选 {data ? data.watchlist.length : ""}
        </button>
        <button type="button" role="tab" aria-selected={tab === "position"} className={tab === "position" ? "active" : ""} onClick={() => setTab("position")}>
          <BriefcaseBusiness size={13} /> 持仓 {data ? data.positions.length : ""}
        </button>
      </div>

      {data && (
        <div className="monitor-meta">
          <span>行情日 {data.trading_date}</span>
          <span>{data.source}</span>
          <span>更新 {new Date(data.fetched_at).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}</span>
          {data.status === "partial" && <span className="warning">部分股票暂无报价</span>}
        </div>
      )}

      {tab === "position" && riskCount > 0 && (
        <div className="monitor-risk-summary">
          <AlertTriangle size={15} />
          <span>{riskCount} 个持仓进入止损/目标附近或已触发状态，请打开持仓详情核对。</span>
        </div>
      )}

      {!data && loading && <div className="monitor-empty"><RefreshCw size={18} className="spin" /> 正在读取自选和持仓…</div>}
      {data && rows.length === 0 && (
        <div className="monitor-empty">{tab === "watch" ? "还没有自选股，先在行情页查询并加入自选。" : "暂无未平仓持仓，成交记录会自动汇总到这里。"}</div>
      )}

      {rows.length > 0 && (
        <div className="monitor-list">
          {tab === "watch"
            ? (rows as WatchRow[]).map((row) => (
              <button type="button" className="monitor-row" key={row.symbol} onClick={() => onOpenSymbol(row.symbol)}>
                <div className="monitor-main">
                  <div><strong>{row.name || row.symbol}</strong><span>{row.symbol}{row.quote_date ? ` · ${row.quote_date}` : ""}</span></div>
                  <div className="monitor-price"><strong>{price(row.price)}</strong><span className={row.change_pct !== null && row.change_pct < 0 ? "down" : "up"}>{pct(row.change_pct)}</span></div>
                </div>
                <div className="monitor-extra">
                  <span>成交额 <b>{amount(row.amount_yuan)}</b></span>
                  {row.is_today && row.price !== null && row.limit_up !== null && row.price >= row.limit_up ? <span className="warning">涨停</span> : null}
                  {row.is_today && row.price !== null && row.limit_down !== null && row.price <= row.limit_down ? <span className="down">跌停</span> : null}
                  <span>{(() => { const move = move3m(row.symbol, row.price); return move !== null && Math.abs(move) >= 1.5 ? `急${move > 0 ? "拉" : "跌"} ${move >= 0 ? "+" : ""}${move.toFixed(2)}%` : null; })()}</span>
                  <span>{row.available ? (row.is_today ? "今日行情" : "旧行情") : "暂无报价"}</span>
                  <ChevronRight size={16} />
                </div>
              </button>
            ))
            : (rows as PositionRow[]).map((row) => (
              <button type="button" className={`monitor-row ${row.risk_state !== "normal" ? "risk" : ""}`} key={row.id ?? row.symbol} onClick={() => onOpenSymbol(row.symbol)}>
                <div className="monitor-main">
                  <div><strong>{row.name || row.symbol}</strong><span>{row.symbol} · {row.open_shares.toLocaleString("zh-CN")} 股</span></div>
                  <div className="monitor-price"><strong>{price(row.price)}</strong><span className={row.change_pct !== null && row.change_pct < 0 ? "down" : "up"}>{pct(row.change_pct)}</span></div>
                </div>
                <div className="monitor-extra">
                  <span>成本 <b>{price(row.avg_cost)}</b></span>
                  {row.is_today && row.price !== null && row.limit_up !== null && row.price >= row.limit_up ? <span className="warning">涨停</span> : null}
                  {row.is_today && row.price !== null && row.limit_down !== null && row.price <= row.limit_down ? <span className="down">跌停</span> : null}
                  <span>浮盈亏 <b className={(row.unrealized_pnl ?? 0) >= 0 ? "up" : "down"}>{money(row.unrealized_pnl)}</b></span>
                  <span className={row.risk_state !== "normal" ? "warning" : ""}>{RISK_LABEL[row.risk_state]}</span>
                  <ChevronRight size={16} />
                </div>
              </button>
            ))}
        </div>
      )}

      <p className="monitor-note">数据源：腾讯批量快照；取不到的标的会显示“暂无报价”，不会当成 0。后台价格提醒仍由 Android WorkManager 独立运行。</p>
    </section>
  );
}
