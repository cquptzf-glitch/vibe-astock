import { ExternalLink, Newspaper, RefreshCw } from "lucide-react";
import { WebSelect } from "./controls";
import { useState } from "react";
import { useDiscoveryData, type ProviderFailure } from "./discoveryData";
import { openPublicUrl } from "./externalLinks";
import "./discovery-features.css";

interface Catalog { industries: { key: string; name: string }[]; sources: { name: string; industry: string }[] }
export interface NewsItem { title: string; summary: string; link: string | null; published: string | null; source: string; redline: string[] }
export interface NewsData { status: string; items: NewsItem[]; sources_tried: number; sources_ok: number; failures: ProviderFailure[]; fetched_at: string }

export function NewsResults({ data, keyword = "", redlineOnly = false, onOpen }: {
  data: NewsData; keyword?: string; redlineOnly?: boolean; onOpen: (url: string) => void;
}) {
  const filter = keyword.trim().toLocaleLowerCase();
  const rows = data.items.filter((item) => (!redlineOnly || item.redline.length > 0)
    && (!filter || (item.title + " " + item.summary).toLocaleLowerCase().includes(filter)));
  return (
    <div className="discovery-results">
      <p className="discovery-footnote">来源成功 {data.sources_ok} / {data.sources_tried} · 共 {rows.length} 条</p>
      {data.status === "partial" && <div className="discovery-notice" role="status">部分来源获取失败：{data.failures.map((failure) => failure.source + "（" + failure.message + "）").join("；")}</div>}
      {rows.length === 0 && <div className="mobile-empty">所选条件下暂无资讯。{data.status === "partial" ? "部分来源尚未取到数据。" : ""}</div>}
      {rows.map((item, index) => (
        <article className="discovery-feature-card" key={item.link ?? item.source + index}>
          <div className="discovery-footnote">{item.source} · {item.published ? new Date(item.published).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" }) : "发布时间未提供"}</div>
          <h2>{item.title}</h2>
          {item.summary && <p>{item.summary}</p>}
          {item.redline.length > 0 && <div className="discovery-tags">{item.redline.map((hit) => <span key={hit}>关注词：{hit}</span>)}</div>}
          {item.link ? <button className="discovery-action" type="button" onClick={() => onOpen(item.link!)}><ExternalLink size={14} /> 阅读原文</button>
            : <p className="discovery-footnote">该条目未提供可用的网页地址</p>}
        </article>
      ))}
    </div>
  );
}

export function NewsPanel() {
  const catalog = useDiscoveryData<Catalog>("/mobile/news/catalog");
  const [industry, setIndustry] = useState("ai");
  const [source, setSource] = useState("");
  const [days, setDays] = useState(7);
  const [keyword, setKeyword] = useState("");
  const [redlineOnly, setRedlineOnly] = useState(false);
  const [openError, setOpenError] = useState("");
  const result = useDiscoveryData<NewsData>(`/mobile/news?industry=${encodeURIComponent(industry)}&days=${days}&source=${encodeURIComponent(source)}`);
  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title"><div><h1><Newspaper size={20} /> 产业资讯</h1><p>公开 RSS 资讯，保留来源与发布时间。</p></div>
        <button className="discovery-action" type="button" disabled={result.loading} onClick={result.refresh} aria-label="刷新产业资讯"><RefreshCw size={18} className={result.loading ? "spin" : ""} /></button></div>
      <div className="discovery-controls">
        <label>行业<WebSelect aria-label="资讯行业" value={industry} disabled={!catalog.data} onChange={(event) => { setIndustry(event.target.value); setSource(""); }}>
          {!catalog.data && <option value="ai">加载行业…</option>}
          {catalog.data?.industries.map((item) => <option key={item.key} value={item.key}>{item.name}</option>)}
        </WebSelect></label>
        <label>来源<WebSelect aria-label="资讯来源" value={source} onChange={(event) => setSource(event.target.value)}>
          <option value="">综合 · 前 4 个来源</option>
          {catalog.data?.sources.filter((item) => item.industry === industry).map((item) => <option key={item.name} value={item.name}>{item.name}</option>)}
        </WebSelect></label>
        <label>时间<WebSelect value={days} aria-label="资讯时间范围" onChange={(event) => setDays(Number(event.target.value))}><option value={3}>近 3 天</option><option value={7}>近 7 天</option><option value={30}>近 30 天</option></WebSelect></label>
        <label>关键词<input type="search" aria-label="筛选资讯关键词" placeholder="筛选已获取的资讯" value={keyword} onChange={(event) => setKeyword(event.target.value)} /></label>
      </div>
      <label className="discovery-check"><input type="checkbox" checked={redlineOnly} onChange={(event) => setRedlineOnly(event.target.checked)} /> 仅显示含关注词的资讯</label>
      {(result.error || catalog.error || openError) && <div className="mobile-error" role="alert">{result.error || catalog.error || openError}<button className="discovery-action" type="button" onClick={() => {result.refresh(); catalog.refresh(); setOpenError("");}}>重试</button></div>}
      {result.loading && <div className="mobile-empty" role="status">正在获取资讯…</div>}
      {result.data && <NewsResults data={result.data} keyword={keyword} redlineOnly={redlineOnly} onOpen={(url) => { void openPublicUrl(url).catch((error: unknown) => setOpenError(error instanceof Error ? error.message : "无法打开原文")); }} />}
    </section>
  );
}
