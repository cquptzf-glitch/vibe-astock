import { useEffect, useState } from "react";
import { WebSelect } from "./controls";
import { fmtPercent, useDiscoveryData } from "./discoveryData";
import { RequestState } from "./StockInsightsPanel";
import "./stock-insights.css";
import "./discovery-features.css";

export interface RiskItem {
  symbol: string; name: string; change_pct?: number | null; deviation_pct?: number | null;
  days?: number | null; rule?: string; rule_code?: number; is_today?: boolean | null;
  start?: string; end?: string; state?: "active" | "upcoming";
}
export interface RiskData {
  status: string; kind: "anomaly" | "monitor"; date: string; stale: boolean;
  page: number; pages: number; has_more: boolean; dropped: number; items: RiskItem[];
  source: string; note: string;
}
export function RiskResults({ data, keyword = "", onOpenSymbol }: { data: RiskData; keyword?: string; onOpenSymbol: (symbol: string) => void }) {
  const query = keyword.trim().toLocaleLowerCase();
  const shown = data.items.filter((item) => (item.symbol + item.name).toLocaleLowerCase().includes(query));
  return <div className="discovery-results">
    {data.stale && <div className="discovery-notice">来源日期 {data.date}，并非今日名单。</div>}
    {data.dropped > 0 && <div className="discovery-notice">{data.dropped} 条异常记录已跳过，当前结果可能不完整。</div>}
    <p className="discovery-footnote">{data.kind === "monitor" ? "窗口校验日期" : "来源日期"} {data.date} · {data.source} · {data.kind === "anomaly" ? "异常波动记录" : "重点监控窗口"}</p>
    {shown.length ? <div className="risk-list">{shown.map((item, index) =>
      <button className="risk-row" type="button" key={item.symbol + ":" + index} onClick={() => onOpenSymbol(item.symbol)}>
        <strong>{item.name} · {item.symbol}</strong>
        {data.kind === "anomaly" ? <>
          <span>{item.rule} · {item.is_today == null ? "当日状态未提供" : item.is_today ? "来源标记当日" : "来源标记非当日"}</span>
          <span>涨跌 {fmtPercent(item.change_pct)} · 偏离 {fmtPercent(item.deviation_pct)} · 窗口 {item.days ?? "—"} 个交易日</span>
        </> : <>
          <span>{item.start} ~ {item.end}</span><small>{item.state === "active" ? "生效中" : "尚未生效"}</small>
        </>}
      </button>)}</div> : <div className="mobile-empty">{query ? "当前页没有匹配记录。" : "来源当前没有记录；不代表其他风险不存在。"}</div>}
    <p className="discovery-footnote">{data.note}</p>
  </div>;
}
type RiskView = { kind: "anomaly" | "monitor"; page: number };
function savedView(): RiskView {
  try {
    const saved = JSON.parse(window.sessionStorage.getItem("vibe.risk.view") ?? "null") as Partial<RiskView> | null;
    return { kind: saved?.kind === "monitor" ? "monitor" : "anomaly", page: typeof saved?.page === "number" && Number.isInteger(saved.page) && saved.page >= 1 && saved.page <= 100 ? saved.page : 1 };
  } catch { return { kind: "anomaly", page: 1 }; }
}
export function RiskPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [view, setView] = useState<RiskView>(savedView);
  const [keyword, setKeyword] = useState("");
  useEffect(() => { try { window.sessionStorage.setItem("vibe.risk.view", JSON.stringify(view)); } catch { /* Optional session persistence. */ } }, [view]);
  const result = useDiscoveryData<RiskData>(`/mobile/shortline/risk?kind=${view.kind}&page=${view.page}`);
  return <section className="discovery-section">
    <h1>异常波动与重点监控</h1>
    <p className="discovery-footnote">展示公开来源的异常偏离和监控窗口，不预测涨跌。</p>
    <div className="risk-toolbar discovery-controls">
      <label>名单<WebSelect value={view.kind} onChange={(event) => { setView({ kind: event.target.value as RiskView["kind"], page: 1 }); setKeyword(""); }}><option value="anomaly">异常波动</option><option value="monitor">重点监控</option></WebSelect></label>
      <label>筛选当前页<input type="search" value={keyword} maxLength={80} onChange={(event) => setKeyword(event.target.value)} placeholder="代码或名称" /></label>
      <button className="discovery-action" disabled={result.loading} onClick={result.refresh}>刷新</button>
    </div>
    <RequestState {...result} />
    {result.data && <RiskResults data={result.data} keyword={keyword} onOpenSymbol={onOpenSymbol} />}
    {view.kind === "anomaly" && <div className="risk-pagination">
      <button className="discovery-action" disabled={result.loading || view.page <= 1} onClick={() => setView((v) => ({ ...v, page: v.page - 1 }))}>上一页</button>
      <span>第 {view.page} 页{result.data ? ` / ${result.data.pages} 页` : ""}</span>
      <button className="discovery-action" disabled={result.loading || !result.data?.has_more || view.page >= 100} onClick={() => setView((v) => ({ ...v, page: v.page + 1 }))}>下一页</button>
    </div>}
  </section>;
}
