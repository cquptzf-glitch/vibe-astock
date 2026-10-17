import { Layers3, RefreshCw } from "lucide-react";
import { WebSelect } from "./controls";
import { useEffect, useState } from "react";
import { fmtNumber, fmtPercent, fmtYuan, formatSampleTime, useDiscoveryData, type ProviderFailure } from "./discoveryData";
import { TrendLine } from "./GpuRentPanel";
import "./discovery-features.css";

interface BoardMember { symbol: string; name: string; price: number | null; change_pct: number | null; amount_yuan: number | null; turnover_pct: number | null; main_net_yuan: number | null }
export interface BoardData {
  status: string; code: string; source: string; errors: ProviderFailure[];
  quote: { name: string; price: number | null; change_pct: number | null; amount_yuan: number | null; asof_ts: number | null } | null;
  constituents: { items: BoardMember[]; total: number; page: number; has_more: boolean; dropped: number; page_breadth: {up: number; down: number; flat: number; unknown: number} } | null;
  history: { rows: { date: string; close: number }[] } | null;
}

export function BoardDetailResults({ data, onOpenSymbol }: { data: BoardData; onOpenSymbol: (symbol: string) => void }) {
  const members = data.constituents;
  const points: [number, number][] = (data.history?.rows ?? []).map((row) => [Date.parse(row.date + "T00:00:00Z") / 1000, row.close]);
  return (
    <div className="discovery-results">
      <article className="discovery-feature-card"><h2>{data.quote?.name ?? data.code}</h2>
        <div className="discovery-price"><strong>{fmtNumber(data.quote?.price)}</strong><span>{fmtPercent(data.quote?.change_pct)}</span></div>
        <p className="discovery-footnote">{data.code} · 成交额 {fmtYuan(data.quote?.amount_yuan)} · {formatSampleTime(data.quote?.asof_ts)}</p>
        {members && <p>成分股总数 {members.total} 只</p>}
      </article>
      {data.errors.length > 0 && <div className="discovery-notice" role="status">部分数据暂不可用：{data.errors.map((failure) => failure.source + "（" + failure.message + "）").join("；")}</div>}
      {data.history && <article className="discovery-feature-card"><h2>近 90 个交易日 · 板块指数</h2><TrendLine points={points} label={data.code + " 板块指数历史"} /><p className="discovery-footnote">板块指数日线，不复权；展示实际返回的 {points.length} 个采样点。</p></article>}
      {members && <>
        <p className="discovery-footnote">本页统计：上涨 {members.page_breadth.up} / 下跌 {members.page_breadth.down} / 平盘 {members.page_breadth.flat} / 缺数据 {members.page_breadth.unknown}。仅覆盖当前页，非全板块统计。</p>
        {members.dropped > 0 && <div className="discovery-notice">本页 {members.dropped} 条异常股票记录已跳过。</div>}
        {members.items.length === 0 ? <div className="mobile-empty">当前页暂无成分股。</div> : <div className="mobile-watch-list">
          {members.items.map((item) => <button className="discovery-member" type="button" key={item.symbol} onClick={() => onOpenSymbol(item.symbol)}>
            <div><strong>{item.name}</strong><span>{item.symbol}</span><small>主力净额 {fmtYuan(item.main_net_yuan)}</small></div>
            <div><strong>{fmtNumber(item.price)} · {fmtPercent(item.change_pct)}</strong><span>换手 {fmtPercent(item.turnover_pct)}</span><small>成交额 {fmtYuan(item.amount_yuan)}</small></div>
          </button>)}
        </div>}
      </>}
      <p className="discovery-footnote">来源：{data.source}。行情与资金读数是已发生的市场现象。</p>
    </div>
  );
}

function savedView(code: string): { page: number; sort: string } {
  try {
    const value = JSON.parse(window.sessionStorage.getItem("vibe.board.view." + code) ?? "null") as { page?: unknown; sort?: unknown } | null;
    const page = typeof value?.page === "number" && Number.isInteger(value.page) && value.page >= 1 && value.page <= 200 ? value.page : 1;
    const sort = typeof value?.sort === "string" && ["change:desc", "change:asc", "net:desc", "turnover:desc"].includes(value.sort) ? value.sort : "change:desc";
    return { page, sort };
  } catch { return { page: 1, sort: "change:desc" }; }
}

export function BoardDetailPanel({ code, onOpenSymbol }: { code: string; onOpenSymbol: (symbol: string) => void }) {
  const [view, setView] = useState(() => savedView(code));
  const { page, sort } = view;
  const setPage = (update: (page: number) => number) => setView((current) => ({ ...current, page: update(current.page) }));
  useEffect(() => {
    try { window.sessionStorage.setItem("vibe.board.view." + code, JSON.stringify(view)); } catch { /* Session persistence is optional. */ }
  }, [code, view]);
  const [field, order] = sort.split(":");
  const result = useDiscoveryData<BoardData>(`/mobile/boards/detail?code=${encodeURIComponent(code)}&page=${page}&sort=${field}&order=${order}`);
  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title"><div><h1><Layers3 size={20} /> 板块详情</h1><p>行情、日线与成分股，可点击个股继续查看。</p></div>
        <button className="discovery-action" type="button" disabled={result.loading} onClick={result.refresh} aria-label="刷新板块详情"><RefreshCw size={18} className={result.loading ? "spin" : ""} /></button></div>
      <div className="discovery-controls"><label>成分股排序<WebSelect aria-label="成分股排序" value={sort} onChange={(event) => setView({ sort: event.target.value, page: 1 })}>
        <option value="change:desc">涨幅优先</option><option value="change:asc">跌幅优先</option><option value="net:desc">主力净额优先</option><option value="turnover:desc">换手率优先</option>
      </WebSelect></label></div>
      {result.error && <div className="mobile-error" role="alert">{result.error}<button type="button" className="discovery-action" onClick={result.refresh}>重试</button></div>}
      {result.loading && <div className="mobile-empty" role="status">正在获取板块详情…</div>}
      {result.data && <BoardDetailResults data={result.data} onOpenSymbol={onOpenSymbol} />}
      <div className="discovery-pagination"><button type="button" disabled={page <= 1 || result.loading} onClick={() => setPage((value) => value - 1)}>上一页</button><span>第 {page} 页</span><button type="button" disabled={!result.data?.constituents?.has_more || result.loading} onClick={() => setPage((value) => value + 1)}>下一页</button></div>
    </section>
  );
}
