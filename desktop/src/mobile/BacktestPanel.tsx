import { BarChart3, Loader2, Play } from "lucide-react";
import { useMemo, useState } from "react";

import { WebSelect } from "./controls";
import "./backtest.css";

type Trade = { symbol?: string; entry_date: string; entry_price: number; exit_date: string; exit_price: number; shares: number; pnl: number; pnl_pct: number; fees: number; reason: string };
type PerSymbol = { symbol: string; total_return_pct: number; max_drawdown_pct: number; buy_hold_return_pct: number; closed_trades: number; win_rate_pct: number | null; open_position: boolean };
type ExperimentRow = { label: string; is_current: boolean; total_return_pct: number; max_drawdown_pct: number; closed_trades: number; win_rate_pct: number | null };

export type Result = {
  status: string;
  symbol: string;
  market: string;
  symbols?: string[];
  strategy: { id?: string; name: string; label?: string; fast?: number | null; slow?: number | null; execution: string; fees: string; adjustment: string; stop_loss_pct?: number | null; weighting?: string };
  period: { start: string | null; end: string | null; bars: number };
  metrics: {
    initial_capital: number;
    final_equity: number;
    total_return_pct: number;
    buy_hold_return_pct: number;
    excess_return_pct: number;
    max_drawdown_pct: number;
    annualized_return_pct?: number | null;
    sharpe?: number | null;
    closed_trades: number;
    win_rate_pct: number | null;
    total_fees?: number;
    slippage_cost?: number;
    blocked_buys?: number;
    blocked_sells?: number;
    open_position: boolean;
  };
  per_symbol?: PerSymbol[];
  trades?: Trade[];
  experiment?: { rows: ExperimentRow[]; positive_share: number | null; note: string };
  equity_curve: [string, number][];
  note: string;
};

type SummaryRow = {
  label: string;
  value: string;
  signed?: boolean;
};

export type StrategyId = "sma_cross" | "donchian" | "rsi";
type StrategySpec = { id: StrategyId; label: string; hint: string; params: { key: string; label: string; value: string }[] };
const SMA_SPEC: StrategySpec = { id: "sma_cross", label: "均线交叉", hint: "前一收盘 > 快线 > 慢线进场;跌破快线或快线下穿慢线出场", params: [{ key: "fast", label: "快线", value: "10" }, { key: "slow", label: "慢线", value: "30" }] };
export const STRATEGIES: StrategySpec[] = [
  SMA_SPEC,
  { id: "donchian", label: "通道突破", hint: "收盘突破前 N 日最高收盘进场,跌破前 M 日最低收盘出场", params: [{ key: "entry", label: "突破天数", value: "20" }, { key: "exit", label: "跌破天数", value: "10" }] },
  { id: "rsi", label: "RSI 超卖反弹", hint: "RSI 低于买入线进场,高于卖出线出场", params: [{ key: "period", label: "周期", value: "14" }, { key: "buy", label: "买入线", value: "30" }, { key: "sell", label: "卖出线", value: "70" }] },
];

const specOf = (id: StrategyId): StrategySpec => STRATEGIES.find((s) => s.id === id) ?? SMA_SPEC;

export interface BacktestForm {
  symbols: string;
  strategy: StrategyId;
  params: Record<string, string>;
  bars: string;
  fees: "default" | "none";
  slippage: string;
  stopLoss: string;
  experiment: boolean;
  limitCheck: boolean;
}

const splitSymbols = (text: string) => text.split(/[\s,，、;；]+/).filter(Boolean);

/** 表单 → 请求体。空的止损不传;代码按空格/逗号/顿号拆开;多只股票时不做参数实验 */
export function buildRequest(form: BacktestForm): Record<string, unknown> {
  const symbols = splitSymbols(form.symbols);
  const spec = specOf(form.strategy);
  const body: Record<string, unknown> = { strategy: form.strategy, bars: Number(form.bars), fees: form.fees, slippage_bps: Number(form.slippage || "0"), limit_check: form.limitCheck };
  if (symbols.length > 1) body.symbols = symbols; else body.symbol = symbols[0] ?? "";
  for (const p of spec.params) body[p.key] = Number(form.params[p.key] ?? p.value);
  if (form.stopLoss.trim()) body.stop_loss_pct = Number(form.stopLoss);
  if (form.experiment && symbols.length <= 1) body.experiment = true;
  return body;
}

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

function pct(value: number | null | undefined): string {
  return value === null || value === undefined || !Number.isFinite(value) ? "—" : `${value >= 0 ? "+" : ""}${value.toFixed(2)}%`;
}

function yuan(value: number | undefined): string {
  return value === undefined || !Number.isFinite(value) ? "—" : `${Math.round(value).toLocaleString("zh-CN")} 元`;
}

export function EquityCurve({ points }: { points: [string, number][] }) {
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

const sign = (value: string) => (value.startsWith("-") ? "down" : value === "—" ? "" : "up");

export function ResultView({ result }: { result: Result }) {
  const m = result.metrics;
  const summary = useMemo<SummaryRow[]>(() => [
    { label: "策略收益", value: pct(m.total_return_pct), signed: true },
    { label: "买入持有", value: pct(m.buy_hold_return_pct), signed: true },
    { label: "超额收益", value: pct(m.excess_return_pct), signed: true },
    { label: "最大回撤", value: pct(m.max_drawdown_pct), signed: true },
    { label: "年化收益", value: pct(m.annualized_return_pct) },
    { label: "夏普(日度)", value: m.sharpe == null ? "—" : m.sharpe.toFixed(2) },
    { label: "胜率", value: pct(m.win_rate_pct) },
    { label: "已平仓", value: `${m.closed_trades} 笔` },
    { label: "费用 / 滑点", value: `${yuan(m.total_fees)} / ${yuan(m.slippage_cost)}` },
    ...(m.blocked_buys != null ? [{ label: "涨跌停没成交", value: `买 ${m.blocked_buys} / 卖 ${m.blocked_sells ?? 0} 次` }] : []),
  ], [m]);
  const trades = result.trades ?? [];
  const shown = trades.slice(-20).reverse();
  const multi = (result.symbols?.length ?? 1) > 1;
  return (
    <>
      <div className="backtest-meta"><span>{(result.symbols ?? [result.symbol]).join("、")} · {result.market}</span><span>{result.period.start ?? "—"} → {result.period.end ?? "—"}</span><span>{result.period.bars} 根</span></div>
      <article className="backtest-result">
        <div className="backtest-result-head"><div><strong>{result.strategy.name}</strong><span>{result.strategy.label ?? `MA${result.strategy.fast} / MA${result.strategy.slow}`}{result.strategy.stop_loss_pct ? ` · 止损 ${result.strategy.stop_loss_pct}%` : ""}</span></div><span className="backtest-badge">历史核对工具</span></div>
        <div className="backtest-metrics">{summary.map(({ label, value, signed }) => <div key={label}><span>{label}</span><strong className={signed ? sign(value) : ""}>{value}</strong></div>)}</div>
        <EquityCurve points={result.equity_curve} />
      </article>
      {multi && result.per_symbol && <section className="backtest-block" aria-label="分标的结果">
        <h2>分标的结果({result.strategy.weighting})</h2>
        <table><thead><tr><th>代码</th><th>策略</th><th>买入持有</th><th>回撤</th><th>笔数</th></tr></thead>
          <tbody>{result.per_symbol.map((s) => <tr key={s.symbol}><td>{s.symbol}{s.open_position ? "(持仓中)" : ""}</td><td className={sign(pct(s.total_return_pct))}>{pct(s.total_return_pct)}</td><td>{pct(s.buy_hold_return_pct)}</td><td>{pct(s.max_drawdown_pct)}</td><td>{s.closed_trades}</td></tr>)}</tbody></table>
      </section>}
      {shown.length > 0 && <section className="backtest-block" aria-label="成交明细">
        <h2>成交明细(最近 {shown.length} 笔,共 {trades.length} 笔)</h2>
        <table><thead><tr><th>{multi ? "代码 / " : ""}买入 → 卖出</th><th>价</th><th>盈亏(净)</th><th>原因</th></tr></thead>
          <tbody>{shown.map((t, i) => <tr key={`${t.entry_date}-${t.exit_date}-${i}`}><td>{multi ? `${t.symbol} ` : ""}{t.entry_date.slice(2)} → {t.exit_date.slice(2)}</td><td>{t.entry_price.toFixed(2)} → {t.exit_price.toFixed(2)}</td><td className={sign(pct(t.pnl_pct))}>{pct(t.pnl_pct)}<br /><small>{yuan(t.pnl)}</small></td><td>{t.reason}</td></tr>)}</tbody></table>
      </section>}
      {result.experiment && <section className="backtest-block" aria-label="参数实验">
        <h2>参数实验({result.experiment.rows.length} 组{result.experiment.positive_share != null ? `,${Math.round(result.experiment.positive_share * 100)}% 为正收益` : ""})</h2>
        <table><thead><tr><th>参数</th><th>收益</th><th>回撤</th><th>笔数</th></tr></thead>
          <tbody>{result.experiment.rows.map((r) => <tr key={r.label} className={r.is_current ? "current" : ""}><td>{r.label}{r.is_current ? "(当前)" : ""}</td><td className={sign(pct(r.total_return_pct))}>{pct(r.total_return_pct)}</td><td>{pct(r.max_drawdown_pct)}</td><td>{r.closed_trades}</td></tr>)}</tbody></table>
        <p className="backtest-note">{result.experiment.note}</p>
      </section>}
      <p className="backtest-note">{result.note}。{result.strategy.fees};{result.strategy.adjustment}。</p>
    </>
  );
}

const defaultParams = (id: StrategyId): Record<string, string> => Object.fromEntries((specOf(id)).params.map((p) => [p.key, p.value]));

export function BacktestPanel() {
  const [form, setForm] = useState<BacktestForm>({ symbols: "600519", strategy: "sma_cross", params: defaultParams("sma_cross"), bars: "500", fees: "default", slippage: "0", stopLoss: "", experiment: false, limitCheck: true });
  const [result, setResult] = useState<Result | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const spec = specOf(form.strategy);
  const multi = splitSymbols(form.symbols).length > 1;

  async function run() {
    if (loading) return;
    setLoading(true);
    setError("");
    try {
      setResult(await runBacktest(buildRequest(form)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="backtest-panel">
      <div className="backtest-title">
        <div><div className="backtest-kicker"><BarChart3 size={15} /> Lite 回测</div><h1>本地日线策略回测</h1><p>手机直接用 Rust 计算,不需要 Python。信号用前一交易日收盘,下一交易日开盘成交,避免同一根 K 线偷看未来;按 100 股整手、全仓进出,默认计入佣金、过户费和卖出印花税。</p></div>
      </div>

      <form className="backtest-form" onSubmit={(event) => { event.preventDefault(); void run(); }}>
        <label className="wide">股票代码(最多 5 只,空格分隔;多只等权、各自独立)<input value={form.symbols} onChange={(e) => setForm({ ...form, symbols: e.target.value })} placeholder="600519 000001" maxLength={80} aria-label="股票代码" /></label>
        <label>策略<WebSelect aria-label="策略" value={form.strategy} onChange={(e) => { const id = e.target.value as StrategyId; setForm({ ...form, strategy: id, params: defaultParams(id) }); }}>{STRATEGIES.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}</WebSelect></label>
        {spec.params.map((p) => <label key={p.key}>{p.label}<input inputMode="decimal" value={form.params[p.key] ?? p.value} onChange={(e) => setForm({ ...form, params: { ...form.params, [p.key]: e.target.value } })} aria-label={p.label} /></label>)}
        <label>日线根数<input inputMode="numeric" value={form.bars} onChange={(e) => setForm({ ...form, bars: e.target.value })} aria-label="日线根数" /></label>
        <label>交易费用<WebSelect aria-label="交易费用" value={form.fees} onChange={(e) => setForm({ ...form, fees: e.target.value as BacktestForm["fees"] })}><option value="default">默认费率(佣金万 2.5、最低 5 元)</option><option value="none">不计费用</option></WebSelect></label>
        <label>滑点(bp,万分之几)<input inputMode="decimal" value={form.slippage} onChange={(e) => setForm({ ...form, slippage: e.target.value })} aria-label="滑点" /></label>
        <label>止损(%,选填)<input inputMode="decimal" value={form.stopLoss} placeholder="不填 = 不止损" onChange={(e) => setForm({ ...form, stopLoss: e.target.value })} aria-label="止损" /></label>
        <label className="check wide"><input type="checkbox" checked={form.limitCheck} onChange={(e) => setForm({ ...form, limitCheck: e.target.checked })} aria-label="模拟涨跌停" /> 模拟涨跌停(开盘贴着涨停买不进、贴着跌停卖不出;ST 股 5% 没单独识别)</label>
        <label className="check wide"><input type="checkbox" checked={form.experiment && !multi} disabled={multi} onChange={(e) => setForm({ ...form, experiment: e.target.checked })} /> 同时做参数实验(固定一小格组合,仅单只股票)</label>
        <p className="backtest-hint wide">{spec.hint}</p>
        <button type="submit" disabled={loading || !form.symbols.trim()}><Play size={14} />{loading ? <><Loader2 size={14} className="spin" /> 计算中</> : "开始回测"}</button>
      </form>

      {error && <div className="backtest-error">{error}</div>}
      {result && <ResultView result={result} />}
    </section>
  );
}
