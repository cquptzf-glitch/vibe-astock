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
    if (!match || !Number.isFinite(amount) || amount <= 0) continue;
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

export function IndustryRevenueView({ data }: { data: Revenue }) {
  return <div>{data.companies.map((c) => {
    const history = revenueHistory(c.series ?? []);
    return <article className="discovery-feature-card" key={c.ticker}>
      <h3>{c.name} {c.ticker}</h3>
      <p>{c.period}{c.stale ? ` · 已滞后 ${c.lag_months} 个月` : ""}</p>
      <p>营收 {fmtNumber(c.revenue_twd / 1e8)} 亿新台币</p>
      <p>环比 {fmtPercent(c.mom_pct)} · 同比 {fmtPercent(c.yoy_pct)} · 累计同比 {fmtPercent(c.cumulative_yoy_pct)}</p>
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
