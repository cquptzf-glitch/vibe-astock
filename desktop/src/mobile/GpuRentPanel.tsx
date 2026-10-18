import { IndustryRevenue } from "./IndustryRevenue";
import { Activity, RefreshCw } from "lucide-react";
import { WebSelect } from "./controls";
import { useState } from "react";
import { fmtNumber, fmtPercent, formatSampleTime, useDiscoveryData, type ProviderFailure } from "./discoveryData";
import "./discovery-features.css";

export interface GpuSeries { gpu: string; status: string; points: [number, number][]; latest: number | null; asof_ts?: number; change_7d_pct?: number | null; stale?: boolean; dropped?: number; message: string; depreciation_reference_usd?: number | null }
export interface GpuData { status: string; items: GpuSeries[]; errors: ProviderFailure[]; source: string; method: string; guard: string; scope: string; forward?: { status: string; message?: string; contract_month?: string; lowest_strike?: number; p_below_lowest?: number; monotonic?: boolean; partial?: boolean; ladder?: { strike: number; p_above: number; price_type: string }[]; note?: string } }

export function TrendLine({ points, label }: { points: [number, number][]; label: string }) {
  if (points.length < 2) return <div className="discovery-footnote">历史采样不足，暂不能绘图。</div>;
  const low = Math.min(...points.map((point) => point[1]));
  const high = Math.max(...points.map((point) => point[1]));
  const start = points[0]![0];
  const end = points[points.length - 1]![0];
  const coordinates = points.map(([time, price]) => `${((time - start) / (end - start || 1) * 100).toFixed(2)},${(35 - (price - low) / (high - low || 1) * 30).toFixed(2)}`).join(" ");
  return <svg className="discovery-trend" viewBox="0 0 100 40" preserveAspectRatio="none" role="img" aria-label={label}><polyline points={coordinates} fill="none" vectorEffect="non-scaling-stroke" /></svg>;
}

export function GpuRentResults({ data }: { data: GpuData }) {
  return (
    <div className="discovery-results">
      {data.forward && <article className="discovery-feature-card"><h2>B200 月均租金 · 远期概率</h2>{data.forward.status === "ok" ? <><p>合约月 {data.forward.contract_month} · 月均低于 $ {data.forward.lowest_strike} 的市场概率 {((data.forward.p_below_lowest ?? 0) * 100).toFixed(1)}%</p>{data.forward.ladder?.map((r) => <p key={r.strike}>高于 $ {r.strike}：{(r.p_above * 100).toFixed(1)}% · {r.price_type === "bid_ask_mid" ? "买卖盘中点" : "最后成交"}</p>)}{data.forward.monotonic === false && <p>各档报价不满足单调关系，可能缺乏流动性。</p>}{data.forward.partial && <p>仅取到部分合约，阶梯可能不完整。</p>}<p className="discovery-footnote">{data.forward.note}</p></> : <p>{data.forward.message}</p>}</article>}
      {data.status === "partial" && <div className="discovery-notice" role="status">部分型号取数失败、采样缺失或已过期，请查看各型号说明。</div>}
      {data.items.map((item) => (
        <article className="discovery-feature-card" key={item.gpu}>
          <h2>{item.gpu}</h2>
          {item.status === "failed" || item.status === "unavailable" ? <p>{item.message}</p> : <>
            <div className="discovery-price"><strong>$ {fmtNumber(item.latest)}</strong><span>/ 卡·时</span></div>
            <p className="discovery-footnote">较约 7 日前 {fmtPercent(item.change_7d_pct)} · 采样 {formatSampleTime(item.asof_ts)}</p>
            {item.message && <div className="discovery-notice">{item.message}</div>}
            <TrendLine points={item.points} label={item.gpu + " 租金历史，" + item.points.length + " 个采样点"} />
            {item.depreciation_reference_usd != null && <p className="discovery-footnote">设备折旧参考 $ {fmtNumber(item.depreciation_reference_usd)} / 卡·时</p>}
          </>}
        </article>
      ))}
      {data.errors.map((failure) => <p className="discovery-footnote" key={failure.source}>{failure.source}：{failure.message}</p>)}
      <article className="discovery-feature-card"><h2>统计口径</h2><p>{data.method}</p><p>{data.guard}</p><p className="discovery-footnote">{data.source} · {data.scope}</p></article>
    </div>
  );
}

export function GpuRentPanel() {
  const [days, setDays] = useState(90);
  const [revenueOpen,setRevenueOpen]=useState(false);
  const result = useDiscoveryData<GpuData>("/mobile/industry/gpu-rent?days=" + days);
  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title"><div><h1><Activity size={20} /> 产业信号 · GPU 租金</h1><p>B200、H100 SXM、A100 SXM4 的公开租金采样。</p></div>
        <button type="button" className="discovery-action" disabled={result.loading} onClick={result.refresh} aria-label="刷新 GPU 租金"><RefreshCw size={18} className={result.loading ? "spin" : ""} /></button></div>
      <div className="discovery-controls"><label>历史范围<WebSelect aria-label="GPU 租金历史范围" value={days} onChange={(event) => setDays(Number(event.target.value))}><option value={30}>近 30 天</option><option value={90}>近 90 天</option><option value={365}>近一年</option></WebSelect></label></div>
      {result.error && <div className="mobile-error" role="alert">{result.error}<button className="discovery-action" type="button" onClick={result.refresh}>重试</button></div>}
      {result.loading && <div className="mobile-empty" role="status">正在获取 GPU 租金采样…</div>}
      {result.data && <GpuRentResults data={result.data} />}
      <details onToggle={e=>setRevenueOpen(e.currentTarget.open)}><summary>更多温度计：台系供应链月营收</summary>{revenueOpen&&<IndustryRevenue/>}</details>
    </section>
  );
}
