import { useDiscoveryData, fmtNumber, fmtPercent } from "./discoveryData";
import "./industry-revenue.css";

type MonthPoint = [string, number];
interface RevenueCompany {
  ticker: string; name: string; period: string; revenue_twd: number;
  mom_pct: number | null; yoy_pct: number | null; cumulative_yoy_pct: number | null;
  stale: boolean; lag_months: number; series?: MonthPoint[];
}
interface Revenue {
  companies: RevenueCompany[]; errors: { ticker: string; message: string }[];
  ccl_pcb_mom_difference_pp: number | null; note: string; source: string;
}

/** Use only matching calendar months as comparison bases; gaps are never treated as zero. */
export function revenueHistory(series: MonthPoint[]) {
  const values = new Map<number, number>();
  for (const [period, amount] of series) {
    const match = /^(\d{4})-(0[1-9]|1[0-2])$/.exec(period);
    if (!match || Number(match[1]) < 2000 || Number(match[1]) > 2100 || !Number.isFinite(amount) || amount <= 0) continue;
    values.set(Number(match[1]) * 12 + Number(match[2]) - 1, amount);
  }
  const periods = [...values.keys()].sort((a, b) => a - b);
  if (!periods.length) return [];
  const first = periods[0]!;
  const last = periods[periods.length - 1]!;
  // The provider accepts 2000–2100; refuse pathological ranges from old/corrupt data.
  if (last - first > 1211) return [];
  const rows = [];
  for (let month = last; month >= first; month--) {
    const amount = values.get(month);
    const prior = values.get(month - 1);
    const yearAgo = values.get(month - 12);
    rows.push({ period: `${Math.floor(month / 12)}-${String(month % 12 + 1).padStart(2, "0")}`,
      amount: amount ?? null,
      mom: amount != null && prior != null ? (amount / prior - 1) * 100 : null,
      yoy: amount != null && yearAgo != null ? (amount / yearAgo - 1) * 100 : null });
  }
  return rows;
}

export function RevenueTrend({ company, source, note }: { company: RevenueCompany; source: string; note: string }) {
  const [months, setMonths] = useState(24);
  const chartRef = useRef<SVGSVGElement>(null);
  const identity = useMemo(() => ({ company, months, source, note }), [company, months, source, note]);
  const history = revenueHistory(company.series ?? []);
  const rows = (months ? history.slice(0, months) : history).reverse();
  const present = rows.filter((r) => r.amount != null);
  if (present.length < 2) return <p className="revenue-chart-note">有效月营收不足两个月，暂不绘制走势。</p>;
  const max = Math.max(...present.map((r) => r.amount! / 1e8));
  const x = (i: number) => 50 + i * 298 / Math.max(rows.length - 1, 1);
  const y = (amount: number) => 154 - amount / 1e8 / max * 126;
  const paths: string[] = [];
  let segment = "";
  rows.forEach((r, i) => {
    if (r.amount == null) { if (segment) paths.push(segment); segment = ""; }
    else segment += `${segment ? "L" : "M"}${x(i).toFixed(2)} ${y(r.amount).toFixed(2)} `;
  });
  if (segment) paths.push(segment);
  const first = rows[0]!.period, last = rows[rows.length - 1]!.period;
  const missing = rows.length - present.length;
  const labels = [...new Set([0, Math.floor((rows.length - 1) / 2), rows.length - 1])];
  const lastValue = present[present.length - 1]!;
  return <section className="revenue-chart" aria-label={`${company.name}月营收趋势`}>
    <div className="revenue-chart-heading"><h4>月营收走势</h4><div role="group" aria-label={`${company.name}走势区间`}>
      {[12, 24, 0].map((n) => <button type="button" key={n} aria-pressed={months === n} onClick={() => setMonths(n)}>{n ? `近 ${n} 月` : "全部"}</button>)}
    </div></div>
    <p className="revenue-chart-note">{first} 至 {last} · {present.length} 个月有值 · 缺失 {missing} 个月；缺月处断开，不按零补值。</p>
    <svg ref={chartRef} viewBox="0 0 360 184" role="img" aria-label={`${company.name}月营收走势图，单位亿新台币`}>
      <title>{`${company.name} ${first} 至 ${last} 月营收；缺月处断开`}</title>
      <text x="50" y="14" className="revenue-axis">亿新台币</text>
      {[0, 0.5, 1].map((ratio) => <g key={ratio}><line x1="50" x2="348" y1={154 - ratio * 126} y2={154 - ratio * 126} className="revenue-grid" /><text x="44" y={158 - ratio * 126} textAnchor="end" className="revenue-axis">{fmtNumber(max * ratio)}</text></g>)}
      {rows.map((r, i) => r.amount == null ? <line key={r.period} x1={x(i)} x2={x(i)} y1="28" y2="154" className="revenue-gap"><title>{`${r.period} 缺失`}</title></line> : <circle key={r.period} cx={x(i)} cy={y(r.amount)} r="2.5" className="revenue-point"><title>{`${r.period} ${fmtNumber(r.amount / 1e8)} 亿新台币`}</title></circle>)}
      {paths.map((d, i) => <path key={i} d={d} className="revenue-line" />)}
      {labels.map((i) => <text key={i} x={x(i)} y="176" textAnchor={i === 0 ? "start" : i === rows.length - 1 ? "end" : "middle"} className="revenue-axis">{rows[i]!.period}</text>)}
    </svg>
    <ChartShareCard chartRef={chartRef} identity={identity} slug={`monthly-revenue-${company.ticker}-${months || "all"}`} spec={{
      title: `${company.name} ${company.ticker} 月营收`, subtitle: `${first} 至 ${last} · 亿新台币`, source,
      details: [`末月 ${lastValue.period}：${fmtNumber(lastValue.amount! / 1e8)} 亿新台币`, `${present.length} 个月有值；缺失 ${missing} 个月，缺月处断开，不按零补值。`],
      notes: [company.stale ? `数据已滞后 ${company.lag_months} 个月，不能视为最新月份。` : `最新披露月份为 ${company.period}。`, "走势图范围随区间切换；完整数值见历史月营收表。", note],
    }} />
  </section>;
}

export function IndustryRevenueView({ data }: { data: Revenue }) {
  return <div>{data.companies.map((c) => {
    const history = revenueHistory(c.series ?? []);
    return <article className="discovery-feature-card" key={c.ticker}>
      <h3>{c.name} {c.ticker}</h3>
      <p>{c.period}{c.stale ? ` · 已滞后 ${c.lag_months} 个月` : ""}</p>
      <p>营收 {fmtNumber(c.revenue_twd / 1e8)} 亿新台币</p>
      <p>环比 {fmtPercent(c.mom_pct)} · 同比 {fmtPercent(c.yoy_pct)} · 累计同比 {fmtPercent(c.cumulative_yoy_pct)}</p>
      <RevenueTrend company={c} source={data.source} note={data.note} />
      {history.length > 0 && <details className="industry-revenue-history"><summary>历史月营收（{history.length} 个月）</summary>
        <p>按月份倒序；缺失月份和缺少比较基数显示为 —，金额单位为亿新台币。</p>
        <div className="industry-revenue-table"><table aria-label={`${c.name}历史月营收`}><thead><tr><th scope="col">月份</th><th scope="col">营收</th><th scope="col">环比</th><th scope="col">同比</th></tr></thead>
          <tbody>{history.map((row) => <tr key={row.period}><th scope="row">{row.period}</th><td>{row.amount == null ? "—" : fmtNumber(row.amount / 1e8)}</td><td>{fmtPercent(row.mom)}</td><td>{fmtPercent(row.yoy)}</td></tr>)}</tbody></table></div>
      </details>}
    </article>;
  })}{data.errors.map((e) => <p role="status" key={e.ticker}>{e.ticker}：{e.message}</p>)}
    <p>台光 − 金像电环比差：{data.ccl_pcb_mom_difference_pp == null ? "同期数据不足" : `${fmtNumber(data.ccl_pcb_mom_difference_pp)} 个百分点`}</p>
    <p>{data.note}</p><small>{data.source}</small>
  </div>;
}
export function IndustryRevenue() {
  const r = useDiscoveryData<Revenue>("/mobile/industry/revenue");
  return <section className="mobile-section"><h2>台系供应链月营收</h2><button type="button" className="discovery-action" disabled={r.loading} onClick={r.refresh}>刷新月营收</button>{r.loading && <p>正在读取月营收…</p>}{r.error && <p role="alert">{r.error}</p>}{r.data && <IndustryRevenueView data={r.data}/>}</section>;
}
import { useMemo, useRef, useState } from "react";
import { ChartShareCard } from "./ChartShareCard";
