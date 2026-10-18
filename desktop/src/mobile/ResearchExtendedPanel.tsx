import { useState } from "react";
import { fmtNumber, fmtPercent, useDiscoveryData } from "./discoveryData";
import { TrendLine } from "./GpuRentPanel";
type StatementRow = { period: string; items: { label: string; value: number | null; raw: unknown; unit: string | null }[]; yoy_pct: Record<string, number | null> };
export interface StatementsData { statements: Partial<Record<"income" | "balance" | "cashflow", StatementRow[]>>; errors: { statement: string; message: string }[]; note: string }
export interface ConsensusData { rows: { year: number; institutions: number; mean: number; min: number; max: number; thin_sample: boolean }[]; missing_years: number[]; note: string }
export interface ValuationData { series: Record<string, { rows: [string, number][]; latest: number; date: string; positive_samples: number; percentile_pct: number | null; min: number | null; max: number | null }>; errors: { metric: string; message: string }[]; note: string }
export function StatementsView({ data }: { data: StatementsData }) {
  return <div>{([['income', '利润表'], ['balance', '资产负债表'], ['cashflow', '现金流量表']] as const).map(([id, label]) => <section key={id}><h3>{label}</h3>{data.statements[id]?.map((r) => <details key={r.period}><summary>{r.period} · {r.items.length} 个科目</summary><div className="rw-statement-table"><table><thead><tr><th>科目</th><th>原值 / 单位</th><th>同比</th></tr></thead><tbody>{r.items.map((item, i) => <tr key={i}><td>{item.label}</td><td>{item.value == null ? String(item.raw ?? "—") : fmtNumber(item.value)} {item.unit ?? ""}</td><td>{fmtPercent(r.yoy_pct[item.label])}</td></tr>)}</tbody></table></div></details>) ?? <p>本表未取到</p>}</section>)}{data.errors.map((e) => <p role="status" key={e.statement}>{e.statement}：{e.message}</p>)}<p>{data.note}</p></div>;
}
export function ConsensusView({ data }: { data: ConsensusData }) {
  return <div><h3>机构一致预期 EPS</h3>{data.rows.map((r) => <article className="rw-stage" key={r.year}><strong>{r.year} · {r.institutions} 家机构</strong><p>均值 {fmtNumber(r.mean)} · 最低 {fmtNumber(r.min)} · 最高 {fmtNumber(r.max)}</p>{r.thin_sample && <p>机构不足三家，样本偏少。</p>}</article>)}{data.missing_years.length > 0 && <p>缺失年度：{data.missing_years.join("、")}</p>}<p>{data.note}</p></div>;
}
export function ValuationView({ data }: { data: ValuationData }) {
  return <div>{Object.entries(data.series).map(([id, s]) => <article className="rw-stage" key={id}><h3>{id === "pe_ttm" ? "市盈率 TTM" : "市净率"}</h3><p>{s.date} · {fmtNumber(s.latest)} 倍 · 正值样本 {s.positive_samples} 个</p><p>历史分位 {s.percentile_pct == null ? "样本不足或当前估值非正，未计算" : `${fmtNumber(s.percentile_pct)}%`} · 正值范围 {fmtNumber(s.min)} 至 {fmtNumber(s.max)}</p><TrendLine label={`${id} 历史估值`} points={s.rows.map(([d, v]) => [Date.parse(d), v])} /><small>{s.rows[0]?.[0]} 至 {s.rows.at(-1)?.[0]}</small></article>)}{data.errors.map((e) => <p key={e.metric}>{e.metric}：{e.message}</p>)}<p>{data.note}</p></div>;
}
function DataPanel({ tab, symbol }: { tab: string; symbol: string }) {
  const result = useDiscoveryData<StatementsData | ConsensusData | ValuationData>(`/mobile/research/${tab}?symbol=${encodeURIComponent(symbol)}`);
  return <>{result.loading && <p role="status">正在取数…</p>}{result.error && <p role="alert">{result.error}</p>}<button className="discovery-action" type="button" onClick={result.refresh} disabled={result.loading}>刷新本页数据</button>{result.data && (tab === "statements" ? <StatementsView data={result.data as StatementsData} /> : tab === "consensus" ? <ConsensusView data={result.data as ConsensusData} /> : <ValuationView data={result.data as ValuationData} />)}</>;
}
export function ResearchExtendedPanel({ symbol }: { symbol: string }) {
  const [tab, setTab] = useState("statements");
  return <section className="rw-panel"><div className="mobile-stock-tabs" role="tablist" aria-label="财务与估值"><button role="tab" aria-selected={tab === "statements"} onClick={() => setTab("statements")}>完整三表</button><button role="tab" aria-selected={tab === "valuation"} onClick={() => setTab("valuation")}>历史估值</button><button role="tab" aria-selected={tab === "consensus"} onClick={() => setTab("consensus")}>一致预期</button></div><DataPanel key={`${symbol}-${tab}`} tab={tab} symbol={symbol} /></section>;
}
