import { BarChart3, Loader2, Play } from "lucide-react";
import { useMemo, useState } from "react";

import "./backtest.css";

type Result = {
  status: string;
  symbol: string;
  market: string;
  strategy: { name: string; fast: number; slow: number; execution: string; fees: string; adjustment: string };
  period: { start: string | null; end: string | null; bars: number };
  metrics: {
    initial_capital: number;
    final_equity: number;
    total_return_pct: number;
    buy_hold_return_pct: number;
    excess_return_pct: number;
    max_drawdown_pct: number;
    closed_trades: number;
    win_rate_pct: number | null;
    open_position: boolean;
  };
  equity_curve: [string, number][];
  note: string;
};

type SummaryRow = {
  label: string;
  value: string;
  signed?: boolean;
};

async function runBacktest(body: Record<string, unknown>): Promise<Result> {
  const response = await fetch("/api/mobile/backtest", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & Result) | null;
  if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data as Result;
}

function pct(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : `${value >= 0 ? "+" : ""}${value.toFixed(2)}%`;
}

function EquityCurve({ points }: { points: [string, number][] }) {
  const values = points.map((point) => point[1]).filter(Number.isFinite);
  if (values.length < 2) return <div className="backtest-empty">权益曲线数据不足。</div>;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const width = 640;
  const height = 180;
  const coords = points.map(([, value], index) => {
    const x = (index / Math.max(1, points.length - 1)) * (width - 12) + 6;
    const y = height - 8 - ((value - min) / span) * (height - 20);
    return [x, y] as const;
  });
  return (
    <div className="backtest-chart-wrap">
      <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" className="backtest-chart" aria-label="策略权益曲线">
        <line x1="6" x2={width - 6} y1={height - 8} y2={height - 8} />
        <polyline points={coords.map(([x, y]) => `${x},${y}`).join(" ")} fill="none" vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="backtest-chart-labels"><span>{points[0]?.[0]}</span><span>{points[points.length - 1]?.[0]}</span></div>
    </div>
  );
}

export function BacktestPanel() {
  const [symbol, setSymbol] = useState("600519");
  const [fast, setFast] = useState("10");
  const [slow, setSlow] = useState("30");
  const [bars, setBars] = useState("500");
  const [result, setResult] = useState<Result | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function run() {
    if (loading) return;
    setLoading(true);
    setError("");
    try {
      setResult(await runBacktest({
        symbol: symbol.trim(),
        fast: Number(fast),
        slow: Number(slow),
        bars: Number(bars),
      }));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  const summary = useMemo<SummaryRow[]>(() => {
    if (!result) return [];
    return [
      { label: "策略收益", value: pct(result.metrics.total_return_pct), signed: true },
      { label: "买入持有", value: pct(result.metrics.buy_hold_return_pct), signed: true },
      { label: "超额收益", value: pct(result.metrics.excess_return_pct), signed: true },
      { label: "最大回撤", value: pct(result.metrics.max_drawdown_pct), signed: true },
      { label: "胜率", value: pct(result.metrics.win_rate_pct) },
      { label: "已平仓", value: `${result.metrics.closed_trades} 笔` },
    ];
  }, [result]);

  return (
    <section className="backtest-panel">
      <div className="backtest-title">
        <div><div className="backtest-kicker"><BarChart3 size={15} /> Lite 回测</div><h1>本地日线策略回测</h1><p>手机直接用 Rust 计算，不需要 Python。信号用前一交易日收盘，下一交易日开盘成交，避免同一根 K 线偷看未来。</p></div>
      </div>

      <form className="backtest-form" onSubmit={(event) => { event.preventDefault(); void run(); }}>
        <label>股票代码<input value={symbol} onChange={(e) => setSymbol(e.target.value)} placeholder="600519" maxLength={32} /></label>
        <label>快线<input inputMode="numeric" value={fast} onChange={(e) => setFast(e.target.value)} /></label>
        <label>慢线<input inputMode="numeric" value={slow} onChange={(e) => setSlow(e.target.value)} /></label>
        <label>日线根数<input inputMode="numeric" value={bars} onChange={(e) => setBars(e.target.value)} /></label>
        <button type="submit" disabled={loading || !symbol.trim()}><Play size={14} />{loading ? <><Loader2 size={14} className="spin" /> 计算中</> : "开始回测"}</button>
      </form>

      {error && <div className="backtest-error">{error}</div>}

      {result && (
        <>
          <div className="backtest-meta"><span>{result.symbol} · {result.market}</span><span>{result.period.start ?? "—"} → {result.period.end ?? "—"}</span><span>{result.period.bars} 根</span></div>
          <article className="backtest-result">
            <div className="backtest-result-head"><div><strong>{result.strategy.name}</strong><span>MA{result.strategy.fast} / MA{result.strategy.slow}</span></div><span className="backtest-badge">历史核对工具</span></div>
            <div className="backtest-metrics">{summary.map(({ label, value, signed }) => <div key={label}><span>{label}</span><strong className={signed ? (value.startsWith("-") ? "down" : value === "—" ? "" : "up") : ""}>{value}</strong></div>)}</div>
            <EquityCurve points={result.equity_curve} />
          </article>
          <p className="backtest-note">{result.note}。{result.strategy.fees}；{result.strategy.adjustment}。</p>
        </>
      )}
    </section>
  );
}
