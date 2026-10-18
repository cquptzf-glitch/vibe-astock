import { BarChart3, Loader2, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";

import { RowsShareCard, type CardRow } from "./ShareCard";
import "./backtest.css";

type Trade = { symbol?: string; entry_date: string; entry_price: number; exit_date: string; exit_price: number; shares: number; pnl: number; pnl_pct: number; fees: number; reason: string };
type PerSymbol = { symbol: string; total_return_pct: number; max_drawdown_pct: number; buy_hold_return_pct: number; closed_trades: number; win_rate_pct: number | null; open_position: boolean };
type Monthly = { month: string; return_pct: number; bench_pct: number | null };
type Drawdown = { peak_date: string; trough_date: string; recover_date: string | null; depth_pct: number; bars_to_trough: number; bars_total: number };
type OptimizeRow = { label: string; is_return_pct: number; oos_return_pct: number; is_trades: number; oos_trades: number; best_in_sample: boolean };
export type Optimize =
  | { available: false; reason: string }
  | {
    available: true; in_sample: { start: string; end: string; bars: number }; out_of_sample: { start: string; end: string; bars: number };
    rows: OptimizeRow[]; best_in_sample: { label: string; is_return_pct: number; oos_return_pct: number; oos_rank: number; of: number };
    median_oos_return_pct: number; oos_positive_share: number; note: string;
  };
type ExperimentRow = { label: string; is_current: boolean; total_return_pct: number; max_drawdown_pct: number; closed_trades: number; win_rate_pct: number | null };

export type Result = {
  status: string;
  trade_unit?: string;
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
  monthly?: Monthly[];
  drawdowns?: Drawdown[];
  optimize?: Optimize;
  sharpe_interval?: { available: boolean; reason?: string; low?: number; high?: number; valid_samples?: number; positive_share?: number; note?: string };
  cost_comparison?: { actual_return_pct: number; no_cost_return_pct: number; note: string };
  data_sources?: {symbol:string;source:string;status:string;fetched_at:string;fallback_reason?:string|null}[];
  market_rules?: {note:string};
  suspension?: { untradable_bars: number; note: string };
  benchmark?: { available: boolean; name?: string; return_pct?: number; excess_pct?: number; reason?: string; note?: string };
  walk_forward?: { available: boolean; reason?: string; total_return_pct?: number; note?: string; folds?: { train_start: string; train_end: string; test_start: string; test_end: string; selected: string; is_return_pct: number; oos_return_pct: number }[] };
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
  positionPct: string;
  optimize: boolean;
  walkForward?: boolean;
  benchmark?: boolean;
  rebalanceEvery?: string;
}

const splitSymbols = (text: string) => text.split(/[\s,，、;；]+/).filter(Boolean);

/** 表单 → 请求体。空的止损不传;代码按空格/逗号/顿号拆开;多只股票时不做参数实验 */
export function buildRequest(form: BacktestForm): Record<string, unknown> {
  const symbols = splitSymbols(form.symbols);
  const spec = specOf(form.strategy);
  const body: Record<string, unknown> = { strategy: form.strategy, bars: Number(form.bars), fees: form.fees, slippage_bps: Number(form.slippage || "0"), limit_check: form.limitCheck, position_pct: Number(form.positionPct || "100") };
  if (symbols.length > 1) body.symbols = symbols; else body.symbol = symbols[0] ?? "";
  for (const p of spec.params) body[p.key] = Number(form.params[p.key] ?? p.value);
  if (form.stopLoss.trim()) body.stop_loss_pct = Number(form.stopLoss);
  if (form.experiment && symbols.length <= 1) body.experiment = true;
  if (form.optimize && symbols.length <= 1) body.optimize = true;
  if (form.walkForward && symbols.length <= 1) body.walk_forward = true;
  if (form.benchmark) body.benchmark = true;
  if (form.rebalanceEvery && symbols.length > 1) body.rebalance_every = Number(form.rebalanceEvery);
  return body;
}

// ------------------------------------------------------------ 自然语言回测

export interface NlTurn { role: "user" | "assistant"; content: string }
export interface NlOk { status: "ok"; report_markdown: string; result: Result; params_used: Record<string, unknown>; ignored_params: string[]; resolved_symbols: string[]; saved_document_id: number | null; save_error?: string | null }
export type NlResponse = { status: "needs_info"; question: string } | { status: "tool_error"; message: string } | NlOk;
export interface NlState { history: NlTurn[]; question: string; error: string }
export const NL_EMPTY: NlState = { history: [], question: "", error: "" };

/** 一次回复之后的对话状态:补问就把这一轮问答记进历史(下次一起发);工具报错原样显示、历史不变;跑完清空。最多留 6 条 */
export function reduceNl(state: NlState, content: string, response: NlResponse): { state: NlState; done: NlOk | null } {
  if (response.status === "ok") return { state: NL_EMPTY, done: response };
  if (response.status === "needs_info") {
    const history = [...state.history, { role: "user" as const, content }, { role: "assistant" as const, content: response.question }].slice(-6);
    return { state: { history, question: response.question, error: "" }, done: null };
  }
  return { state: { ...state, question: "", error: response.message }, done: null };
}

async function askNlBacktest(content: string, history: NlTurn[]): Promise<NlResponse> {
  const response = await fetch("/api/mobile/ai/backtest", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ content, history }),
  });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & Partial<NlResponse>) | null;
  if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data as NlResponse;
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

/** 回测分享卡的行:只放这次回测的假设和结果读数,标明是历史假设;不含自选、持仓、成交 */
export function backtestCardRows(result: Result): CardRow[] {
  const m = result.metrics;
  const rows: CardRow[] = [
    { label: "标的", value: (result.symbols ?? [result.symbol]).join("、") },
    { label: "策略", value: result.strategy.label ?? result.strategy.name },
    { label: "区间", value: `${result.period.start ?? "—"} → ${result.period.end ?? "—"}` },
    { label: "策略收益", value: pct(m.total_return_pct) },
    { label: "买入持有", value: pct(m.buy_hold_return_pct) },
    { label: "最大回撤", value: pct(m.max_drawdown_pct) },
    { label: "已平仓 / 胜率", value: `${m.closed_trades} 笔 / ${pct(m.win_rate_pct)}` },
    { label: "手续费 / 滑点", value: `${yuan(m.total_fees)} / ${yuan(m.slippage_cost)}` },
  ];
  return rows;
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
    { label: result.trade_unit === "sell_fill" ? "卖出成交" : "已平仓", value: `${m.closed_trades} 笔` },
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
      {multi && (result.per_symbol?.length ?? 0) > 0 && <section className="backtest-block" aria-label="分标的结果">
        <h2>分标的结果({result.strategy.weighting})</h2>
        <table><thead><tr><th>代码</th><th>策略</th><th>买入持有</th><th>回撤</th><th>笔数</th></tr></thead>
          <tbody>{result.per_symbol!.map((s) => <tr key={s.symbol}><td>{s.symbol}{s.open_position ? "(持仓中)" : ""}</td><td className={sign(pct(s.total_return_pct))}>{pct(s.total_return_pct)}</td><td>{pct(s.buy_hold_return_pct)}</td><td>{pct(s.max_drawdown_pct)}</td><td>{s.closed_trades}</td></tr>)}</tbody></table>
      </section>}
      {shown.length > 0 && <section className="backtest-block" aria-label="成交明细">
        <h2>成交明细(最近 {shown.length} 笔,共 {trades.length} 笔)</h2>
        <table><thead><tr><th>{multi ? "代码 / " : ""}买入 → 卖出</th><th>价</th><th>盈亏(净)</th><th>原因</th></tr></thead>
          <tbody>{shown.map((t, i) => <tr key={`${t.entry_date}-${t.exit_date}-${i}`}><td>{multi ? `${t.symbol} ` : ""}{t.entry_date.slice(2)} → {t.exit_date.slice(2)}</td><td>{t.entry_price.toFixed(2)} → {t.exit_price.toFixed(2)}</td><td className={sign(pct(t.pnl_pct))}>{pct(t.pnl_pct)}<br /><small>{yuan(t.pnl)}</small></td><td>{t.reason}</td></tr>)}</tbody></table>
      </section>}
      {(result.drawdowns?.length ?? 0) > 0 && <section className="backtest-block" aria-label="最深的回撤">
        <h2>最深的 {result.drawdowns!.length} 段回撤</h2>
        <table><thead><tr><th>前高 → 谷底</th><th>深度</th><th>恢复</th></tr></thead>
          <tbody>{result.drawdowns!.map((d, i) => <tr key={i}><td>{d.peak_date} → {d.trough_date}<br /><small>{d.bars_to_trough} 个交易日到谷底</small></td><td className="down">{pct(d.depth_pct)}</td><td>{d.recover_date ? `${d.recover_date}(共 ${d.bars_total} 日)` : "到最后还没恢复"}</td></tr>)}</tbody></table>
      </section>}
      {(result.monthly?.length ?? 0) > 0 && <section className="backtest-block" aria-label="月度收益">
        <h2>月度收益(策略 vs 买入持有)</h2>
        <table><thead><tr><th>月份</th><th>策略</th><th>买入持有</th></tr></thead>
          <tbody>{result.monthly!.map((m) => <tr key={m.month}><td>{m.month}</td><td className={sign(pct(m.return_pct))}>{pct(m.return_pct)}</td><td className={m.bench_pct == null ? "" : sign(pct(m.bench_pct))}>{pct(m.bench_pct)}</td></tr>)}</tbody></table>
      </section>}
      {result.optimize && <section className="backtest-block" aria-label="样本外检验">
        <h2>样本内 / 样本外对照</h2>
        {result.optimize.available ? <>
          <p className="backtest-note">样本内 {result.optimize.in_sample.start} → {result.optimize.in_sample.end}({result.optimize.in_sample.bars} 根);样本外 {result.optimize.out_of_sample.start} → {result.optimize.out_of_sample.end}({result.optimize.out_of_sample.bars} 根)。样本内最好:{result.optimize.best_in_sample.label}(样本内 {pct(result.optimize.best_in_sample.is_return_pct)} → 样本外 {pct(result.optimize.best_in_sample.oos_return_pct)},样本外排第 {result.optimize.best_in_sample.oos_rank}/{result.optimize.best_in_sample.of});全部参数样本外收益中位数 {pct(result.optimize.median_oos_return_pct)},{Math.round(result.optimize.oos_positive_share * 100)}% 为正。</p>
          <table><thead><tr><th>参数</th><th>样本内</th><th>样本外</th><th>笔数(内/外)</th></tr></thead>
            <tbody>{result.optimize.rows.map((r) => <tr key={r.label} className={r.best_in_sample ? "current" : ""}><td>{r.label}{r.best_in_sample ? "(样本内最好)" : ""}</td><td className={sign(pct(r.is_return_pct))}>{pct(r.is_return_pct)}</td><td className={sign(pct(r.oos_return_pct))}>{pct(r.oos_return_pct)}</td><td>{r.is_trades} / {r.oos_trades}</td></tr>)}</tbody></table>
          <p className="backtest-note">{result.optimize.note}</p>
        </> : <p className="backtest-note">暂不可用:{result.optimize.reason}</p>}
      </section>}
      {result.experiment && <section className="backtest-block" aria-label="参数实验">
        <h2>参数实验({result.experiment.rows.length} 组{result.experiment.positive_share != null ? `,${Math.round(result.experiment.positive_share * 100)}% 为正收益` : ""})</h2>
        <table><thead><tr><th>参数</th><th>收益</th><th>回撤</th><th>笔数</th></tr></thead>
          <tbody>{result.experiment.rows.map((r) => <tr key={r.label} className={r.is_current ? "current" : ""}><td>{r.label}{r.is_current ? "(当前)" : ""}</td><td className={sign(pct(r.total_return_pct))}>{pct(r.total_return_pct)}</td><td>{pct(r.max_drawdown_pct)}</td><td>{r.closed_trades}</td></tr>)}</tbody></table>
        <p className="backtest-note">{result.experiment.note}</p>
        <div className="backtest-heatmap" role="img" aria-label="参数敏感度热力图，红色为正收益绿色为负收益，不代表最佳参数">{result.experiment.rows.map((r) => <div key={r.label} className={r.is_current ? "current" : ""} style={{ background: `rgba(${r.total_return_pct >= 0 ? "220,65,65" : "24,160,110"},${Math.min(0.65, Math.max(0.08, Math.abs(r.total_return_pct) / 40))})` }}><small>{r.label}</small><strong>{pct(r.total_return_pct)}</strong></div>)}</div>
      </section>}
      <AdvancedResults result={result} />
      <RowsShareCard title="回测结果(历史假设)" subtitle={`${result.period.start ?? "—"} → ${result.period.end ?? "—"} · 前复权日线`} rows={backtestCardRows(result)} slug="backtest" hint="只含这次回测的设定与读数,不含自选与持仓" />
      <p className="backtest-note">{result.note}。{result.strategy.fees};{result.strategy.adjustment}。</p>
    </>
  );
}



export function AdvancedResults({ result }: { result: Result }) {
  const ci = result.sharpe_interval;
  const walk = result.walk_forward;
  const bench = result.benchmark;
  return <>
    {ci && <section className="backtest-block"><h2>夏普 95% 区间</h2><p>{ci.available ? `${ci.low?.toFixed(2)} 至 ${ci.high?.toFixed(2)} · 有效重抽样 ${ci.valid_samples} 次，夏普为正占 ${((ci.positive_share ?? 0) * 100).toFixed(1)}%` : ci.reason}</p><p className="backtest-note">{ci.note}</p></section>}
    {result.cost_comparison && <section className="backtest-block"><h2>扣费前后对照</h2><p>实际 {pct(result.cost_comparison.actual_return_pct)} · 不计费用及滑点 {pct(result.cost_comparison.no_cost_return_pct)}</p><p className="backtest-note">{result.cost_comparison.note}</p></section>}
    {bench && <section className="backtest-block"><h2>基准指数对照</h2>{bench.available ? <p>{bench.name} {pct(bench.return_pct)} · 策略超额 {pct(bench.excess_pct)}</p> : <p>{bench.reason}</p>}<p className="backtest-note">{bench.note}</p></section>}
    {walk && <section className="backtest-block"><h2>滚动样本外检验</h2>{walk.available ? <><p>各段样本外串联收益 {pct(walk.total_return_pct)}</p><table><thead><tr><th>训练期</th><th>检验期 / 参数</th><th>样本外</th></tr></thead><tbody>{walk.folds?.map((f) => <tr key={f.test_start}><td>{f.train_start} → {f.train_end}<br />{pct(f.is_return_pct)}</td><td>{f.test_start} → {f.test_end}<br />{f.selected}</td><td>{pct(f.oos_return_pct)}</td></tr>)}</tbody></table></> : <p>{walk.reason}</p>}<p className="backtest-note">{walk.note}</p></section>}
    {result.data_sources&&<details><summary>实际行情来源与回测边界</summary>{result.data_sources.map(s=><p key={s.symbol}>{s.symbol} · {s.source} · {s.status} · {s.fetched_at}{s.fallback_reason?` · ${s.fallback_reason}`:""}</p>)}{result.market_rules&&<p>{result.market_rules.note}</p>}</details>}
    {result.suspension && <p className="backtest-note">不可交易样本 {result.suspension.untradable_bars} 根。{result.suspension.note}</p>}
  </>;
}

export function NlBacktestBox({ onDone }: { onDone: (ok: NlOk) => void }) {
  const [text, setText] = useState("");
  const [state, setState] = useState<NlState>(NL_EMPTY);
  const [busy, setBusy] = useState(false);
  const [fatal, setFatal] = useState("");
  async function send() {
    const content = text.trim();
    if (!content || busy) return;
    setBusy(true);
    setFatal("");
    try {
      const out = reduceNl(state, content, await askNlBacktest(content, state.history));
      setState(out.state);
      if (out.done) { setText(""); onDone(out.done); }
      else if (out.state.question) setText("");
    } catch (e) {
      setFatal(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  return <section className="backtest-nl" aria-label="用一句话回测">
    <h2><Sparkles size={14} /> 用一句话回测</h2>
    <p className="backtest-hint">例如「用通道突破回测贵州茅台和平安银行最近两年,止损 8%」。AI 只负责把你的话翻成回测参数(要用你在设置里配好的模型);信息不够会先问你,股票按你写的代码或名称在搜索里确认,不采信模型自己编的代码;回测由程序运行,报告按真实结果生成并存进研究记录。</p>
    <textarea value={text} maxLength={1000} placeholder={state.question ? "回答上面的补问…" : "描述你想回测什么"} aria-label="回测描述" onChange={(e) => setText(e.target.value)} />
    {state.question && <p className="backtest-ask" role="status">AI 想先确认:{state.question}</p>}
    {state.error && <div className="backtest-error" role="alert">{state.error}</div>}
    {fatal && <div className="backtest-error" role="alert">{fatal}</div>}
    <div className="backtest-nl-actions">
      <button type="button" disabled={busy || !text.trim()} onClick={() => void send()}>{busy ? <><Loader2 size={14} className="spin" /> 处理中</> : "让 AI 配置并运行"}</button>
      {(state.history.length > 0 || state.error) && <button type="button" onClick={() => { setState(NL_EMPTY); setFatal(""); }}>重新开始</button>}
    </div>
  </section>;
}

export function BacktestPanel() {
  const [result, setResult] = useState<Result | null>(null);
  const [nlDone, setNlDone] = useState<NlOk | null>(null);
  return <section className="backtest-panel">
    <div className="backtest-title"><div><div className="backtest-kicker"><BarChart3 size={15} /> 研究回测</div><h1>用对话完成历史假设核对</h1><p>描述股票、策略、区间与想比较的问题。助手先确认参数，再调用程序计算；可要求成本对照、滚动样本外、参数敏感度或组合再平衡。实际来源、假设和缺口随报告展示。</p></div></div>
    <NlBacktestBox onDone={ok=>{setNlDone(ok);setResult(ok.result);}} />
    {nlDone && <section className="backtest-block" aria-label="AI 回测报告"><h2>回测报告(程序按真实结果生成)</h2><pre className="backtest-report">{nlDone.report_markdown}</pre><p className="backtest-note">{nlDone.saved_document_id?`已存进「研究记录」(分类:回测,编号 ${nlDone.saved_document_id})。`:`没能存进研究记录:${nlDone.save_error??"原因未知"}。`}{nlDone.ignored_params.length?`模型给出但被丢掉的参数:${nlDone.ignored_params.join("、")}。`:""}</p></section>}
    {result&&<ResultView result={result}/>}
  </section>;
}
