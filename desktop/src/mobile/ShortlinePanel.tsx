import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronRight, Flame, RefreshCw, TimerReset } from "lucide-react";

import { MonitorPanel } from "./MonitorPanel";
import "./shortline.css";

interface LimitUpItem {
  code: string;
  name: string;
  price: number | null;
  pct: number | null;
  amount_yuan: number | null;
  float_mcap_yuan: number | null;
  turnover_pct: number | null;
  limit_days: number | null;
  first_seal: string | null;
  last_seal: string | null;
  seal_fund_yuan: number | null;
  break_times: number | null;
  industry: string;
  zt_stat: string;
}

interface LimitUpResult {
  status: string;
  source: string;
  date: string;
  requested_date: string;
  stale: boolean;
  count: number;
  items: LimitUpItem[];
  fetched_at: string;
}

interface ShortlinePanelProps {
  onOpenSymbol: (symbol: string) => void;
  view: "monitor" | "limit-up";
  onViewChange: (view: "monitor" | "limit-up") => void;
}

async function mobileApi<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, {
    headers: { Accept: "application/json" },
  });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) {
    throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  }
  return body as T;
}

function formatAmount(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  if (Math.abs(value) >= 100_000_000) {
    return `${(value / 100_000_000).toLocaleString("zh-CN", { maximumFractionDigits: 2 })} 亿`;
  }
  if (Math.abs(value) >= 10_000) {
    return `${(value / 10_000).toLocaleString("zh-CN", { maximumFractionDigits: 1 })} 万`;
  }
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 0 });
}

function formatPrice(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : value.toFixed(2);
}

function boardLabel(days: number): string {
  if (days <= 1) return "首板";
  return `${days}板`;
}

export function ShortlinePanel({ onOpenSymbol, view, onViewChange: setView }: ShortlinePanelProps) {
  const [data, setData] = useState<LimitUpResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const result = await mobileApi<LimitUpResult>("/mobile/shortline/limit-up");
      setData(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (view === "limit-up") void load();
  }, [load, view]);

  const groups = useMemo(() => {
    const map = new Map<number, LimitUpItem[]>();
    for (const item of data?.items ?? []) {
      const days = Math.max(1, item.limit_days ?? 1);
      const list = map.get(days) ?? [];
      list.push(item);
      map.set(days, list);
    }
    return [...map.entries()].sort((a, b) => b[0] - a[0]);
  }, [data]);

  const maxBoard = groups[0]?.[0] ?? 0;
  const firstBoardCount = groups.find(([days]) => days === 1)?.[1].length ?? 0;
  const multiBoardCount = Math.max(0, (data?.count ?? 0) - firstBoardCount);

  if (view === "monitor") {
    return (
      <section className="shortline-panel">
        <div className="shortline-tabs" role="tablist" aria-label="短线">
          <button type="button" className="active" role="tab" aria-selected="true">盯盘</button>
          <button type="button" role="tab" aria-selected="false" onClick={() => setView("limit-up")}>涨停池</button>
        </div>
        <MonitorPanel onOpenSymbol={onOpenSymbol} />
      </section>
    );
  }

  return (
    <section className="shortline-panel">
      <div className="shortline-tabs" role="tablist" aria-label="短线">
        <button type="button" role="tab" aria-selected="false" onClick={() => setView("monitor")}>盯盘</button>
        <button type="button" className="active" role="tab" aria-selected="true">涨停池</button>
      </div>
      <div className="shortline-title-row">
        <div>
          <div className="shortline-kicker"><Flame size={15} /> 短线情绪</div>
          <h1>涨停池 · 连板天梯</h1>
          <p>按连板高度优先、首封时间排序，数据来自东财涨停池。</p>
        </div>
        <button className="shortline-refresh" onClick={() => void load()} disabled={loading} aria-label="刷新涨停池">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && (
        <div className="shortline-error">
          <strong>涨停池暂不可用</strong>
          <span>{error}</span>
          <button onClick={() => void load()}>重试</button>
        </div>
      )}

      {!error && !data && loading && (
        <div className="shortline-loading"><RefreshCw className="spin" size={20} /> 正在获取涨停池…</div>
      )}

      {data && (
        <>
          <div className="shortline-meta-row">
            <span>{data.date}</span>
            <span>{data.stale ? "最近交易日" : "今日"}</span>
            <span>{data.source}</span>
          </div>

          <div className="shortline-summary">
            <div><span>涨停</span><strong>{data.count}</strong><small>只</small></div>
            <div><span>最高</span><strong>{maxBoard || "—"}</strong><small>{maxBoard ? "板" : ""}</small></div>
            <div><span>连板</span><strong>{multiBoardCount}</strong><small>只</small></div>
            <div><span>首板</span><strong>{firstBoardCount}</strong><small>只</small></div>
          </div>

          {data.stale && (
            <div className="shortline-stale">
              <TimerReset size={15} /> 当前展示上游最近交易日数据：{data.date}
            </div>
          )}

          {data.items.length === 0 ? (
            <div className="shortline-empty">当前交易日暂无涨停池数据。</div>
          ) : (
            <div className="shortline-ladder">
              {groups.map(([days, items]) => (
                <section className="shortline-board-group" key={days}>
                  <div className="shortline-board-head">
                    <div>
                      <span className={days >= 3 ? "hot" : ""}>{boardLabel(days)}</span>
                      {days >= 3 && <em>高标</em>}
                    </div>
                    <small>{items.length} 只</small>
                  </div>

                  <div className="shortline-list">
                    {items.map((item) => (
                      <button className="shortline-stock" key={item.code} onClick={() => onOpenSymbol(item.code)}>
                        <div className="shortline-stock-main">
                          <div className="shortline-stock-name">
                            <strong>{item.name}</strong>
                            <span>{item.code}</span>
                          </div>
                          <div className="shortline-stock-price">
                            <strong>{formatPrice(item.price)}</strong>
                            <span>{item.pct === null ? "—" : `${item.pct >= 0 ? "+" : ""}${item.pct.toFixed(2)}%`}</span>
                          </div>
                        </div>

                        <div className="shortline-tags">
                          {item.industry && <span>{item.industry}</span>}
                          <span>{item.zt_stat || boardLabel(days)}</span>
                          {item.break_times !== null && item.break_times > 0 && <span>炸板 {item.break_times}</span>}
                        </div>

                        <div className="shortline-stock-extra">
                          <span>首封 <b>{item.first_seal ?? "—"}</b></span>
                          <span>末封 <b>{item.last_seal ?? "—"}</b></span>
                          <span>封单 <b>{formatAmount(item.seal_fund_yuan)}</b></span>
                          <ChevronRight size={16} />
                        </div>
                      </button>
                    ))}
                  </div>
                </section>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}
