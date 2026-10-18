import { useEffect, useMemo, useRef, useState } from "react";
import { askConfirm, DateField, WebSelect } from "./controls";
import { chinaToday } from "./emotionData";
import "./industry-library.css";

export const INDUSTRY_METRICS = { market_size: "市场规模", capacity: "产能", production: "产量", yield: "良率", market_share: "份额", lead_time: "交付周期", price: "价格", other: "其他观测" };
export interface IndustryFact { id?: number; sector: string; metric: string; entity: string; label: string; value: number; unit: string; currency: string; period_end: string; published_date: string; source_name: string; source_url: string; source_excerpt: string }
type Pack = { records: IndustryFact[]; errors?: { id: number; reason: string }[]; conflicts?: { entity:string;label:string;period_end:string;reason:string }[] };
export function parseIndustryPack(text: string, sector: string): IndustryFact[] {
  if (new TextEncoder().encode(text).length > 32 * 1024 * 1024) throw new Error("文件超过 32 MiB 预算");
  const p = JSON.parse(text);
  if (p.schema !== 1 || !Array.isArray(p.records) || !p.records.length || p.records.length > 2000) throw new Error("需要 schema=1、records 为 1–2000 条的产业资料 JSON");
  for (const f of p.records) {
    if (!f || typeof f !== "object" || typeof f.value !== "number" || !Number.isFinite(f.value)) throw new Error("观测必须含真实数值，不能为空或字符串");
    for (const key of ["sector", "metric", "entity", "label", "unit", "currency", "period_end", "published_date", "source_name", "source_url", "source_excerpt"]) if (typeof f[key] !== "string" || !f[key].trim()) throw new Error(`缺少字段 ${key}`);
    if (f.sector !== sector) throw new Error("文件包含其他赛道，请切换到对应赛道后导入");
    const url = new URL(f.source_url); if (url.protocol !== "https:" || url.username || url.password) throw new Error("出处须为无凭据的 HTTPS 链接");
  }
  return p.records;
}
async function api<T>(sector: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/mobile/industry/library?sector=${encodeURIComponent(sector)}`, { method: body ? "POST" : "GET", ...(body ? { headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) } : {}) });
  const data = await response.json(); if (!response.ok) throw new Error(data.message ?? `HTTP ${response.status}`); return data;
}
function fileApi() {
  if (typeof window === "undefined") return null;
  return window.__TAURI__ as unknown as { dialog?: { save(options: { defaultPath: string; filters: { name: string; extensions: string[] }[] }): Promise<string | null>; open(options: { multiple: boolean; directory: boolean; filters: { name: string; extensions: string[] }[] }): Promise<string | string[] | null> }; fs?: { readTextFile(path: string): Promise<string>; writeTextFile(path: string, value: string): Promise<void> } } | undefined;
}
export function IndustryFactsView({ records, onRemove }: { records: IndustryFact[]; onRemove?: (fact: IndustryFact) => void }) {
  return <div className="il-facts">{records.map((f, i) => <article className="mobile-quote-card" key={f.id ?? i}>
    <h3>{f.entity} · {f.label}</h3><p><strong>{f.value.toLocaleString("zh-CN", { maximumSignificantDigits: 15 })} {f.unit}</strong>{f.currency !== "none" ? ` · ${f.currency}` : ""}</p>
    <p>统计期末 {f.period_end} · 发布 {f.published_date}</p><p>{INDUSTRY_METRICS[f.metric as keyof typeof INDUSTRY_METRICS] ?? f.metric} · 用户整理，未独立核验</p>
    <details><summary>出处与原文摘录</summary><a href={f.source_url} target="_blank" rel="noopener noreferrer">{f.source_name}</a><blockquote>{f.source_excerpt}</blockquote></details>
    {onRemove && <button type="button" onClick={() => onRemove(f)}>移除此条观测</button>}
  </article>)}</div>;
}
export function IndustryLibrary({ sector }: { sector: string }) {
  const [pack, setPack] = useState<Pack>({ records: [] }); const [error, setError] = useState(""); const [message, setMessage] = useState(""); const [busy, setBusy] = useState(false); const [pending, setPending] = useState<IndustryFact[]>([]);
  const [metric, setMetric] = useState("capacity"); const [entity, setEntity] = useState(""); const [label, setLabel] = useState(""); const [value, setValue] = useState(""); const [unit, setUnit] = useState(""); const [currency, setCurrency] = useState("none"); const [period, setPeriod] = useState(chinaToday); const [published, setPublished] = useState(chinaToday); const [source, setSource] = useState(""); const [url, setUrl] = useState(""); const [excerpt, setExcerpt] = useState("");
  const [query,setQuery]=useState("");const [filter,setFilter]=useState("");const [visible,setVisible]=useState(50);
  const selected=useMemo(()=>pack.records.filter(f=>(!filter||f.metric===filter)&&(!query||[f.entity,f.label,f.source_name].some(v=>v.includes(query)))),[pack,filter,query]);
  const file = useRef<HTMLInputElement>(null); const mounted = useRef(true);
  useEffect(() => { mounted.current = true; void api<Pack>(sector).then(v => { if (mounted.current) setPack(v); }).catch(e => { if (mounted.current) setError(String(e)); }); return () => { mounted.current = false; }; }, [sector]);
  async function refresh() { const data = await api<Pack>(sector); if (mounted.current) setPack(data); }
  async function save(records: IndustryFact[]) {
    if (busy) return; setBusy(true); setError(""); setMessage("");
    try { const r = await api<{ inserted: number; duplicates: number }>(sector, { action: "import", schema: 1, records }); if (mounted.current) { setMessage(`已保存 ${r.inserted} 条，跳过 ${r.duplicates} 条重复观测。`); setPending([]); await refresh(); } } catch (e) { if (mounted.current) setError(String(e)); } finally { if (mounted.current) setBusy(false); }
  }
  function preview(text: string) { setPending(parseIndustryPack(text,sector)); setError(""); }
  async function choose() { setError(""); try { const native = fileApi(); if (native?.dialog && native.fs) { const path = await native.dialog.open({ multiple: false, directory: false, filters: [{ name: "产业资料 JSON", extensions: ["json"] }] }); if (typeof path === "string") preview(await native.fs.readTextFile(path)); } else file.current?.click(); } catch (e) { setError(String(e)); } }
  async function exportPack() { setError(""); try { const text = JSON.stringify({ schema: 1, records: pack.records }, null, 2); const name = `industry-${sector}-${chinaToday()}.json`; const native = fileApi(); if (native?.dialog && native.fs) { const path = await native.dialog.save({ defaultPath: name, filters: [{ name: "产业资料 JSON", extensions: ["json"] }] }); if (path) await native.fs.writeTextFile(path, text); } else { const link = document.createElement("a"); const object = URL.createObjectURL(new Blob([text], { type: "application/json" })); link.href = object; link.download = name; link.click(); setTimeout(() => URL.revokeObjectURL(object), 1000); } } catch (e) { setError(String(e)); } }
  async function remove(f: IndustryFact) { if (busy || !await askConfirm("移除此条产业观测？已生成研究里的快照会保留。")) return; setBusy(true); try { await api(sector, { action: "delete", sector, id: f.id }); await refresh(); } catch (e) { setError(String(e)); } finally { setBusy(false); } }
  return <section className="industry-library" aria-label="产业数据资料库"><h2>产业数据资料库</h2><p>记录规模、产能、良率、份额和交付周期，保留统计日期、单位及原文出处。数据由你整理，程序未独立核验；研究时需明确选择附带。</p>
    <div className="il-actions"><button type="button" disabled={busy} onClick={() => void choose()}>导入产业资料 JSON</button><button type="button" disabled={!pack.records.length || busy} onClick={() => void exportPack()}>导出本赛道资料</button></div>
    <input ref={file} type="file" accept="application/json,.json" hidden onChange={e => { const f = e.target.files?.[0]; e.target.value = ""; if (!f) return; if (f.size > 32 * 1024 * 1024) { setError("文件超过 32 MiB"); return; } void f.text().then(preview).catch(e => setError(String(e))); }} />
    {!!pending.length && <section aria-label="产业资料导入预览"><p>已读入 {pending.length} 条，仅预览，尚未保存。</p><IndustryFactsView records={pending.slice(0, 3)} /><p>服务器会检查全部记录；任何一条无效都会拒绝本次导入。</p><button disabled={busy} onClick={() => void save(pending)}>确认保存全部 {pending.length} 条</button><button disabled={busy} onClick={() => setPending([])}>取消导入</button></section>}
    <details><summary>手动记录一条观测</summary><form className="il-form" onSubmit={e => { e.preventDefault(); if (!value.trim()) { setError("请输入真实数值，不能用空值代替零"); return; } void save([{ sector, metric, entity, label, value: Number(value), unit, currency, period_end: period, published_date: published, source_name: source, source_url: url, source_excerpt: excerpt }]); }}>
      <label>指标类型<WebSelect value={metric} onChange={e => setMetric(e.target.value)}>{Object.entries(INDUSTRY_METRICS).map(([k,v]) => <option key={k} value={k}>{v}</option>)}</WebSelect></label>
      <label>主体<input required maxLength={120} value={entity} onChange={e => setEntity(e.target.value)} /></label><label>指标名称<input required maxLength={120} value={label} onChange={e => setLabel(e.target.value)} /></label><label>实际观测数值<input required type="number" min="0" step="any" value={value} onChange={e => setValue(e.target.value)} /></label><label>单位<input required maxLength={40} value={unit} onChange={e => setUnit(e.target.value)} placeholder="例如：万片/年、%" /></label><label>币种<WebSelect value={currency} onChange={e => setCurrency(e.target.value)}>{["none","CNY","USD","TWD","HKD","EUR"].map(c => <option key={c} value={c}>{c === "none" ? "不涉及金额" : c}</option>)}</WebSelect></label>
      <label>统计期末<DateField value={period} onChange={e => setPeriod(e.target.value)} /></label><label>发布日期<DateField value={published} onChange={e => setPublished(e.target.value)} /></label><label>来源名称<input required maxLength={120} value={source} onChange={e => setSource(e.target.value)} /></label><label>公开出处 HTTPS 链接<input required type="url" maxLength={1500} value={url} onChange={e => setUrl(e.target.value)} /></label><label className="il-wide">原文摘录<textarea required maxLength={2000} value={excerpt} onChange={e => setExcerpt(e.target.value)} /></label><button disabled={busy} className="il-wide" type="submit">保存观测</button>
    </form></details>
    {error && <p role="alert">{error}</p>}{message && <p role="status">{message}</p>}{!!pack.errors?.length && <p role="alert">有 {pack.errors.length} 条损坏或不合规记录未作为有效观测显示。</p>}
    {!!pack.conflicts?.length&&<details><summary>发现 {pack.conflicts.length} 组不同数值，请核对出处</summary>{pack.conflicts.map((c,i)=><p key={i}>{c.entity} · {c.label} · {c.period_end}：{c.reason}</p>)}</details>}
    <p>本赛道 {pack.records.length} 条观测；完整记录随业务备份保存。</p>
    <div className="il-form"><label>筛选主体、指标或来源<input value={query} maxLength={120} onChange={e=>{setQuery(e.target.value);setVisible(50);}}/></label><label>筛选类型<WebSelect value={filter} onChange={e=>{setFilter(e.target.value);setVisible(50);}}><option value="">全部类型</option>{Object.entries(INDUSTRY_METRICS).map(([k,v])=><option value={k} key={k}>{v}</option>)}</WebSelect></label></div>
    <p>匹配 {selected.length} 条，当前显示 {Math.min(visible,selected.length)} 条。</p><IndustryFactsView records={selected.slice(0,visible)} onRemove={f => void remove(f)} />{selected.length>visible&&<button type="button" onClick={()=>setVisible(v=>v+50)}>继续显示 50 条</button>}
  </section>;
}
