import { useState } from "react";
import { RefreshCw } from "lucide-react";
import { fmtNumber, fmtPercent, fmtYuan, useDiscoveryData } from "./discoveryData";
import "./discovery-features.css";
import "./stock-insights.css";

export interface FlowRow {
  time: string; date: string; main_net: number | null; small_net: number | null;
  mid_net: number | null; large_net: number | null; super_net: number | null;
}
export interface FlowData {
  status: string; mode: "daily" | "minute"; rows: FlowRow[]; latest: FlowRow | null;
  count: number; stale: boolean; method: string; source: string; unit: string;
}
export interface StockBoardData {
  status: string; items: { code: string; name: string; change_pct: number | null }[];
  total: number; dropped: number; truncated: boolean; note: string; source: string;
}
export interface ObservationData {
  status: string; date: string; bars: number; close: number;
  ma5: number | null; ma10: number | null; ma20: number | null; ma60: number | null;
  return5: number | null; return20: number | null; return60: number | null;
  ma20_bias_pct: number | null; ma_cross: string | null; range_observation: string | null;
  prior20_high: number | null; prior20_low: number | null; volume_ratio_prev5: number | null;
  same_day: boolean; stale: boolean; note: string; source: string;
}

export function FlowChart({ rows }: { rows: FlowRow[] }) {
  const shown = rows.slice(-60);
  const max = Math.max(1, ...shown.map((row) => Math.abs(row.main_net ?? 0)));
  const missing = shown.filter((row) => row.main_net == null).length;
  const step = 320 / Math.max(1, shown.length);
  return <figure className="stock-flow-chart">
    <svg role="img" aria-label={`最近 ${shown.length} 个采样的主力净额，缺值 ${missing} 个`} viewBox="0 0 320 110">
      <line x1="0" x2="320" y1="55" y2="55" stroke="currentColor" opacity=".25" />
      {shown.map((row, index) => {
        const value = row.main_net;
        const x = index * step + step / 2;
        if (value == null || !Number.isFinite(value)) return <circle key={row.time} cx={x} cy="55" r="2" fill="currentColor"><title>{row.time} 缺值</title></circle>;
        const height = Math.abs(value) / max * 48;
        return <rect key={row.time} x={x - step * .35} width={step * .7} y={value >= 0 ? 55 - height : 55} height={Math.max(.5, height)}
          fill={value >= 0 ? "#e25a64" : "#36a887"}><title>{row.time}：{fmtYuan(value)} 元</title></rect>;
      })}
    </svg>
    <figcaption>{shown[0]?.time ?? "—"} → {shown.at(-1)?.time ?? "—"} · 缺值以零线圆点标记，不计为 0</figcaption>
  </figure>;
}

export function FundFlowResults({ data }: { data: FlowData }) {
  const latest = data.latest;
  if (!data.rows.length) return <div className="mobile-empty">来源未提供资金流记录；空序列不代表净额为零。</div>;
  return <div className="discovery-results">
    {data.stale && <div className="discovery-notice">最近记录为 {latest?.time ?? "未知时间"}，并非今日数据。</div>}
    {data.status === "partial" && <div className="discovery-notice">部分金额缺失，显示为 —；最新缺值不会由前一条记录代替。</div>}
    <p className="discovery-footnote">{data.mode === "minute" ? "当日累计净额" : "逐日净额"} · 最新采样 {latest?.time ?? "—"} · 金额单位：元</p>
    <div className="mobile-metrics">
      {([["主力", "main_net"], ["超大单", "super_net"], ["大单", "large_net"], ["中单", "mid_net"], ["小单", "small_net"]] as const).map(([name, field]) =>
        <div key={field}><span>{name}净额</span><strong>{fmtYuan(latest?.[field])}</strong></div>)}
    </div>
    <FlowChart rows={data.rows} />
    <div className="stock-flow-table" role="region" aria-label="最近十个资金流采样">
      <table><thead><tr><th>时间</th><th>主力净额（元）</th><th>超大单（元）</th></tr></thead>
        <tbody>{data.rows.slice(-10).reverse().map((row) => <tr key={row.time}><th>{row.time}</th><td>{fmtYuan(row.main_net)}</td><td>{fmtYuan(row.super_net)}</td></tr>)}</tbody>
      </table>
    </div>
    <p className="discovery-footnote">{data.method}。共 {data.count} 个采样 · 来源 {data.source}；资金流为来源按成交单分类统计。</p>
  </div>;
}

export function StockBoardResults({ data, onOpenBoard }: { data: StockBoardData; onOpenBoard: (code: string) => void }) {
  return <div className="discovery-results">
    {data.truncated && <div className="discovery-notice">仅获取 {data.items.length} / {data.total} 个关联，异常记录 {data.dropped} 条；不是完整名单。</div>}
    {data.items.length ? <div className="stock-board-links">{data.items.map((board) =>
      <button type="button" key={board.code} onClick={() => onOpenBoard(board.code)}><strong>{board.name}</strong><span>{board.code} · {fmtPercent(board.change_pct)}</span></button>
    )}</div> : <div className="mobile-empty">来源暂无所属板块。</div>}
    <p className="discovery-footnote">{data.note} · 来源 {data.source}</p>
  </div>;
}

export function ObservationResults({ data }: { data: ObservationData }) {
  return <div className="discovery-results">
    {data.same_day && <div className="discovery-notice">包含今日日线，交易时段内尚未定型；数值随采样变化。</div>}
    {data.stale && <div className="discovery-notice">最新日线距今超过 7 天，请核对停牌或数据源状态。</div>}
    {data.status === "partial" && <div className="discovery-notice">历史不足 61 根，缺少完整窗口的读数显示为 —。</div>}
    <p>{data.date} · 前复权 · {data.bars} 根日线</p>
    <div className="mobile-metrics">
      <div><span>近5根涨跌</span><strong>{fmtPercent(data.return5)}</strong></div>
      <div><span>近20根涨跌</span><strong>{fmtPercent(data.return20)}</strong></div>
      <div><span>近60根涨跌</span><strong>{fmtPercent(data.return60)}</strong></div>
      <div><span>MA5 / MA10</span><strong>{fmtNumber(data.ma5)} / {fmtNumber(data.ma10)}</strong></div>
      <div><span>MA20 / MA60</span><strong>{fmtNumber(data.ma20)} / {fmtNumber(data.ma60)}</strong></div>
      <div><span>距MA20</span><strong>{fmtPercent(data.ma20_bias_pct)}</strong></div>
      <div><span>成交量 / 前5根均量</span><strong>{fmtNumber(data.volume_ratio_prev5)} 倍</strong></div>
    </div>
    <p>{data.ma_cross ?? "均线交叉：数据不足"}</p><p>{data.range_observation ?? "此前20根区间：数据不足"}</p>
    <p className="discovery-footnote">此前20根价格区间 {fmtNumber(data.prior20_low)} ~ {fmtNumber(data.prior20_high)} 元，比较窗口不含本根。</p>
    <p className="discovery-footnote">{data.note} · 来源 {data.source}</p>
  </div>;
}

function FlowPanel({ symbol }: { symbol: string }) {
  const [mode, setMode] = useState<"daily" | "minute">("daily");
  const [limit, setLimit] = useState(60);
  const result = useDiscoveryData<FlowData>(`/mobile/stock/fund-flow?symbol=${encodeURIComponent(symbol)}&mode=${mode}&limit=${limit}`);
  return <>
    <div className="discovery-controls"><label>频率<select value={mode} onChange={(e) => setMode(e.target.value as "daily" | "minute")}><option value="daily">日线净额</option><option value="minute">分钟累计</option></select></label>
      {mode === "daily" && <label>交易日数<select value={limit} onChange={(e) => setLimit(Number(e.target.value))}><option value={20}>20</option><option value={60}>60</option><option value={120}>120</option></select></label>}
      <button className="discovery-action" disabled={result.loading} onClick={result.refresh}>刷新</button>
    </div>
    <RequestState {...result} />
    {result.data && <FundFlowResults data={result.data} />}
  </>;
}
function BoardsPanel({ symbol, onOpenBoard }: { symbol: string; onOpenBoard: (code: string) => void }) {
  const result = useDiscoveryData<StockBoardData>("/mobile/stock/boards?symbol=" + encodeURIComponent(symbol));
  return <><button className="discovery-action" disabled={result.loading} onClick={result.refresh}>刷新所属板块</button><RequestState {...result} />{result.data && <StockBoardResults data={result.data} onOpenBoard={onOpenBoard} />}</>;
}
function ObservationsPanel({ symbol }: { symbol: string }) {
  const result = useDiscoveryData<ObservationData>("/mobile/stock/observations?symbol=" + encodeURIComponent(symbol));
  return <><button className="discovery-action" disabled={result.loading} onClick={result.refresh}>刷新量价观察</button><RequestState {...result} />{result.data && <ObservationResults data={result.data} />}</>;
}
export function RequestState({ loading, error, refresh }: { loading: boolean; error: string; refresh: () => void }) {
  return <>{loading && <div className="mobile-empty"><RefreshCw size={16} className="spin" /> 正在获取公开数据…</div>}
    {error && <div className="mobile-error" role="alert">{error}<button className="discovery-action" onClick={refresh}>重试</button></div>}</>;
}
export function StockInsightsPanel({ symbol, onOpenBoard }: { symbol: string; onOpenBoard: (code: string) => void }) {
  const [tab, updateTab] = useState<"observations" | "funds" | "boards">(() => {
    try { const value = window.sessionStorage.getItem("vibe.stock.insights." + symbol); return value === "funds" || value === "boards" ? value : "observations"; }
    catch { return "observations"; }
  });
  const setTab = (value: "observations" | "funds" | "boards") => {
    updateTab(value);
    try { window.sessionStorage.setItem("vibe.stock.insights." + symbol, value); } catch { /* Optional session persistence. */ }
  };
  return <article className="mobile-quote-card stock-insights">
    <h2>量价与资金观察</h2>
    <div className="mobile-stock-tabs" role="tablist" aria-label="量价与资金观察">
      {([["observations", "量价"], ["funds", "资金流"], ["boards", "所属板块"]] as const).map(([key, label]) =>
        <button type="button" role="tab" key={key} aria-selected={tab === key} className={tab === key ? "active" : ""} onClick={() => setTab(key)}>{label}</button>)}
    </div>
    {tab === "observations" && <ObservationsPanel symbol={symbol} />}
    {tab === "funds" && <FlowPanel symbol={symbol} />}
    {tab === "boards" && <BoardsPanel symbol={symbol} onOpenBoard={onOpenBoard} />}
  </article>;
}
