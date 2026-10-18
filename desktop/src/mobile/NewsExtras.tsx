import { useState } from "react";
import { useDiscoveryData } from "./discoveryData";
import { openPublicUrl } from "./externalLinks";
interface ProbabilityData { items: { id: string; module: string; venue: string; title: string; leg: string; prob: number; price_type: string; volume_24h: number | null; close: string; url: string | null }[]; failures: { source: string; message: string }[]; sampled_at: string; scope: string; note: string }
export function ProbabilityView({ data }: { data: ProbabilityData }) {
  const [error, setError] = useState("");
  return <div><p>{data.scope}</p><p>{data.note}</p><small>采样时点 {data.sampled_at}</small>{data.failures.map((f,i) => <p role="status" key={`${f.source}-${i}`}>{f.source}：{f.message}</p>)}{data.items.map((m, i) => <article className="discovery-feature-card" key={`${m.venue}-${m.id}-${i}`}><h3>{m.title}</h3><p>{m.module} · {m.venue} · {m.leg}</p><p>市场定价概率 {(m.prob * 100).toFixed(1)}% · 结算 {m.close}</p><p>24h 成交量 {m.volume_24h ?? "未提供"} · 报价类型 {m.price_type}</p>{m.url && <button type="button" onClick={() => void openPublicUrl(m.url!).catch((e) => setError(String(e)))}>查看原始合约</button>}</article>)}{!data.items.length && <p>本次抽样没有可展示的合约，请结合来源缺口查看。</p>}{error && <p role="alert">{error}</p>}</div>;
}
export function ProbabilityPanel() {
  const result = useDiscoveryData<ProbabilityData>("/mobile/news/probability");
  return <section className="mobile-section"><h2>事件定价概率</h2><button className="discovery-action" disabled={result.loading} type="button" onClick={result.refresh}>刷新</button>{result.loading && <p>正在获取两源开放合约…</p>}{result.error && <p role="alert">{result.error}</p>}{result.data && <ProbabilityView data={result.data} />}</section>;
}
function Announcements({ code }: { code: string }) {
  const result = useDiscoveryData<{ items: { title: string; date: string; art_code: string; pdf_url: string }[] }>(`/mobile/research/announcements?symbol=${encodeURIComponent(code)}&limit=30`);
  const [error,setError]=useState("");
  return <>{result.loading && <p>正在获取公告…</p>}{(result.error||error) && <p role="alert">{result.error||error}</p>}{result.data?.items.map((r) => <article key={r.art_code} className="discovery-feature-card"><h3>{r.title}</h3><p>{r.date}</p>{r.pdf_url&&<button type="button" onClick={()=>void openPublicUrl(r.pdf_url).catch((e)=>setError(String(e)))}>查看公告原文</button>}</article>)}{result.data?.items.length===0&&<p>源站当前列表没有公告。</p>}</>;
}
export function AnnouncementRadar() {
  const [code, setCode] = useState(""); const [selected,setSelected]=useState("");
  return <section className="mobile-section"><h2>A 股公告雷达</h2><p>按股票查看公开公告，保留标题、日期与原文入口。</p><div className="rw-form"><input aria-label="公告股票代码" placeholder="六位股票代码" value={code} maxLength={16} onChange={(e)=>setCode(e.target.value)} /><button type="button" disabled={!code.trim()} onClick={()=>setSelected(code.trim())}>查询公告</button></div>{selected&&<Announcements key={selected} code={selected}/>}</section>;
}
