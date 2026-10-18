export interface RuleCoverage {
  note: string; mode?: string; status?: string;
  sources?: { symbol: string; status: string; reason?: string; source?: string; ipo_date?: string; delist_date?: string | null; covered_bars?: number; st_days?: number; ipo_rule_bars?: number; missing_dates?: string[]; scope?: string; fetched_at?: string }[];
}
export function BacktestRuleAudit({ audit }: { audit: RuleCoverage }) {
  return <section className="backtest-block" aria-label="历史交易制度覆盖"><h2>历史交易制度覆盖</h2>
    <p>{audit.status === "ok" ? "已取得本轮历史日状态" : audit.status === "disabled" ? "已关闭历史制度取数，结果使用近似规则" : "历史制度数据存在缺口"}{audit.mode ? ` · ${audit.mode}` : ""}</p>
    <p className="backtest-note">{audit.note}</p>
    {audit.sources?.map((s) => <details key={s.symbol}><summary>{s.symbol} · {s.status === "ok" ? "日状态已覆盖" : "未完整覆盖"} · {s.covered_bars ?? 0} 根</summary>
      {s.reason && <p role="status">{s.reason}</p>}
      {s.ipo_date && <p>上市日 {s.ipo_date}{s.delist_date ? ` · 退市日 ${s.delist_date}` : ""}</p>}
      {s.st_days !== undefined && <p>来源中的 ST 状态 {s.st_days} 日 · 应用新股特殊规则 {s.ipo_rule_bars ?? 0} 根</p>}
      {!!s.missing_dates?.length && <p>未覆盖日期：{s.missing_dates.join("、")}</p>}
      <p>{s.source} {s.fetched_at}</p><p>{s.scope}</p>
    </details>)}
  </section>;
}
