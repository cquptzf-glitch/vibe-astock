import { usePageActive } from "./PageActivity.ts";
import { useEffect, useState } from "react";
import { WebSelect } from "./controls";
import { ChevronDown, ChevronUp, Pencil, Plus, RefreshCw, Save, Share2, Trash2 } from "lucide-react";
import { shareFile, textToBase64 } from "./nativeShare";
import { ReportImport } from "./ReportImport";

/** 分享成 .md 文件的内容:标题、分类 / 股票 / 日期一行、正文;文件名只用 ASCII(原生那边会把别的字符换成下划线) */
export function documentMarkdown(item: Pick<DocumentItem, "title" | "category" | "symbol" | "updated_at" | "body">): string {
  const meta = [item.category, item.symbol, item.updated_at.slice(0, 10)].filter(Boolean).join(" · ");
  return `# ${item.title}

${meta ? `> ${meta}

` : ""}${item.body || "(无正文)"}
`;
}

export interface DocumentItem {
  id: number;
  kind: "note" | "report" | "inbox";
  category: string;
  title: string;
  body: string;
  status: string;
  source: string;
  symbol: string | null;
  created_at: string;
  updated_at: string;
}

interface DocumentList {
  kind: DocumentItem["kind"];
  count: number;
  items: DocumentItem[];
}

export type LibraryKind = DocumentItem["kind"];

const META: Record<LibraryKind, { label: string; title: string; hint: string }> = {
  note: { label: "研究记录", title: "研究记录", hint: "复盘、要点、问答和研究结论沉淀在本机 SQLite。" },
  report: { label: "报告", title: "我的研报", hint: "保存报告正文、研究摘要或外部材料提取文本，可按正文检索。" },
  inbox: { label: "线索", title: "收件箱", hint: "保存稍后处理的线索、事件和待验证事项；可按未读筛选。" },
};

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as T;
}

export interface LibraryFilter { keyword: string; category: string; unreadOnly: boolean; from?: string; to?: string; status?: string; symbol?: string }

/** 本地过滤:标题、正文、分类、股票代码都参与检索;多个词必须同时命中。 */
export function filterDocuments(items: DocumentItem[], filter: LibraryFilter): DocumentItem[] {
  const words = filter.keyword.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  return items.filter((item) => (!filter.category || item.category === filter.category)
    && (!filter.from || item.updated_at.slice(0,10) >= filter.from)
    && (!filter.to || item.updated_at.slice(0,10) <= filter.to)
    && (!filter.status || item.status === filter.status)
    && (!filter.symbol || (item.symbol ?? "").includes(filter.symbol.trim()))
    && (!filter.unreadOnly || item.status !== "read")
    && words.every((word) => [item.title, item.body, item.category, item.symbol ?? ""].join(" ").toLocaleLowerCase().includes(word)));
}

export function LibraryPanel({ kind }: { kind: LibraryKind }) {
  const active = usePageActive();
  const meta = META[kind];
  const [items, setItems] = useState<DocumentItem[]>([]);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [openId, setOpenId] = useState<number | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [category, setCategory] = useState("");
  const [symbol, setSymbol] = useState("");
  const [filter, setFilter] = useState<LibraryFilter>({ keyword: "", category: "", unreadOnly: false });
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [archiveDays,setArchiveDays]=useState(90);
  const [archiveNotice,setArchiveNotice]=useState("");
  async function archivePolicy(enabled:boolean){
    try{const result=await api<{changed:number}>("/mobile/documents/policy",{method:"POST",body:JSON.stringify({kind,days:archiveDays,category:filter.category,enabled})});setArchiveNotice(enabled?`已启用归档规则，本次归档 ${result.changed} 条；以后打开资料库时自动整理。` : "已停用归档规则。");await load(kind);}catch(e){setError(e instanceof Error?e.message:String(e));}
  }

  async function load(nextKind = kind) {
    setLoading(true);
    setError("");
    try {
      const result = await api<DocumentList>(`/mobile/documents?kind=${nextKind}`);
      setItems(result.items);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    resetEditor();
    setFilter({ keyword: "", category: "", unreadOnly: false });
  }, [kind]);
  useEffect(() => { if (active) void load(kind); }, [kind, active]);

  function resetEditor() {
    setEditingId(null);
    setEditorOpen(false);
    setTitle("");
    setBody("");
    setCategory("");
    setSymbol("");
  }

  function edit(item: DocumentItem) {
    setEditingId(item.id);
    setEditorOpen(true);
    setTitle(item.title);
    setBody(item.body);
    setCategory(item.category);
    setSymbol(item.symbol ?? "");
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  async function save() {
    if (!title.trim() || saving) return;
    setSaving(true);
    setError("");
    try {
      await api("/mobile/documents", {
        method: "POST",
        body: JSON.stringify({
          ...(editingId ? { id: editingId } : {}),
          kind,
          title: title.trim(),
          body,
          category: category.trim(),
          symbol: symbol.trim() || null,
          status: kind === "inbox" ? "unread" : "active",
          source: "manual",
        }),
      });
      resetEditor();
      await load(kind);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  async function shareAsFile(item: DocumentItem) {
    setError("");
    try {
      await shareFile({ name: `vibe-astock-${item.kind}-${item.id}.md`, mime: "text/markdown", base64: textToBase64(documentMarkdown(item)), title: item.title, chooserTitle: "分享为 Markdown 文件" });
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }

  async function remove(id: number) {
    setError("");
    try {
      await api("/mobile/documents", { method: "DELETE", body: JSON.stringify({ id }) });
      if (editingId === id) resetEditor();
      await load(kind);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function toggleRead(item: DocumentItem) {
    setError("");
    try {
      await api("/mobile/documents", {
        method: "POST",
        body: JSON.stringify({
          id: item.id,
          kind: item.kind,
          title: item.title,
          body: item.body,
          category: item.category,
          symbol: item.symbol,
          source: item.source,
          status: item.status === "read" ? "unread" : "read",
        }),
      });
      await load(kind);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const categories = [...new Set(items.map((item) => item.category).filter(Boolean))].sort();
  const shown = filterDocuments(items, filter);
  const unread = items.filter((item) => item.status !== "read").length;

  return (
    <section className="mobile-section library-page">
      <div className="mobile-section-title">
        <div>
          <h1>{meta.title}</h1>
          <p>{meta.hint}</p>
        </div>
        <button type="button" className="mobile-delete" onClick={() => void load(kind)} aria-label="刷新资料库">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>
      {error && <div className="mobile-error">{error}</div>}
      <div className="library-toolbar library-filters">
        <div className="mobile-filter-row">
          <input type="search" aria-label="检索资料" placeholder="检索标题、正文、代码；多个词用空格" value={filter.keyword} maxLength={80}
            onChange={(event) => setFilter({ ...filter, keyword: event.target.value })} />
          {categories.length > 0 && <WebSelect aria-label="分类筛选" value={filter.category} onChange={(event) => setFilter({ ...filter, category: event.target.value })}>
            <option value="">全部分类</option>{categories.map((value) => <option key={value} value={value}>{value}</option>)}
          </WebSelect>}
          {kind === "inbox" && <label className="discovery-check"><input type="checkbox" checked={filter.unreadOnly} onChange={(event) => setFilter({ ...filter, unreadOnly: event.target.checked })} />只看未读（{unread}）</label>}
        </div>
        <p className="mobile-filter-note">共 {items.length} 条，显示 {shown.length} 条</p>
      </div>

      <div className="library-toolbar">
        <button type="button" className="library-editor-toggle" aria-expanded={editorOpen} onClick={() => (editorOpen && !editingId ? setEditorOpen(false) : editingId ? resetEditor() : setEditorOpen(true))}>
          <span>{editingId ? `编辑 #${editingId}` : `新建${meta.label}`}</span>{editorOpen ? <ChevronUp size={16} /> : <Plus size={16} />}
        </button>
        {editorOpen && <div style={{ padding: 10 }}>
          <input value={title} onChange={(event) => setTitle(event.target.value)} maxLength={500} placeholder="标题" aria-label="标题" className="mobile-library-input" />
          <div className="mobile-library-inline">
            <input value={category} onChange={(event) => setCategory(event.target.value)} maxLength={100} placeholder="分类（可选）" aria-label="分类" className="mobile-library-input" list="library-categories" />
            <input value={symbol} onChange={(event) => setSymbol(event.target.value)} maxLength={32} placeholder="关联代码（可选）" aria-label="关联代码" className="mobile-library-input" />
          </div>
          <datalist id="library-categories">{categories.map((value) => <option key={value} value={value} />)}</datalist>
          <textarea value={body} onChange={(event) => setBody(event.target.value)} maxLength={250000} rows={8} placeholder="正文 / 摘要 / 待办内容" aria-label="正文" className="mobile-library-textarea" />
          <div className="mobile-library-actions">
            <button type="button" onClick={resetEditor}>取消</button>
            <button type="button" onClick={() => void save()} disabled={!title.trim() || saving}>
              {editingId ? <Save size={15} /> : <Plus size={15} />}{saving ? "保存中…" : editingId ? "保存修改" : "添加"}
            </button>
          </div>
        </div>}
      </div>

      {kind === "report" && <ReportImport onSaved={() => void load(kind)} />}
      <details className="library-advanced"><summary>日期、状态检索与自动归档</summary><div className="rw-form"><label>更新起日<input type="text" placeholder="YYYY-MM-DD" value={filter.from??""} onChange={(e)=>setFilter({...filter,from:e.target.value})} /></label><label>更新止日<input type="text" placeholder="YYYY-MM-DD" value={filter.to??""} onChange={(e)=>setFilter({...filter,to:e.target.value})} /></label><label>关联代码<input value={filter.symbol??""} onChange={(e)=>setFilter({...filter,symbol:e.target.value})} /></label><label>状态<WebSelect value={filter.status??""} onChange={(e)=>setFilter({...filter,status:e.target.value})}><option value="">所有状态</option><option value="active">使用中</option><option value="archived">已归档</option><option value="read">已读</option><option value="unread">未读</option></WebSelect></label><label>超过多少天未更新自动归档<input inputMode="numeric" value={archiveDays} onChange={(e)=>setArchiveDays(Number(e.target.value))} /></label><p>规则应用于当前资料类型{filter.category?`的「${filter.category}」分类`:"的所有分类"}；收件箱未读内容不自动归档。原文保留，可按「已归档」检索并编辑恢复。</p><button type="button" onClick={()=>void archivePolicy(true)}>启用并应用规则</button><button type="button" onClick={()=>void archivePolicy(false)}>停用规则</button>{archiveNotice&&<p role="status">{archiveNotice}</p>}</div></details>

      <div className="mobile-watch-list" style={{ marginTop: 12 }}>
        {loading && items.length === 0 ? (
          <div className="mobile-empty"><RefreshCw className="spin" size={17} /> 正在加载…</div>
        ) : shown.length === 0 ? (
          <div className="mobile-empty">{items.length ? "没有匹配的资料。" : `还没有${meta.label}。`}</div>
        ) : shown.map((item) => (
          <div className="mobile-library-item" key={item.id}>
            <button type="button" className="mobile-library-main" aria-expanded={openId === item.id} onClick={() => setOpenId(openId === item.id ? null : item.id)}>
              <strong>{item.title}</strong>
              <span>
                {item.category || meta.label}
                {item.symbol ? ` · ${item.symbol}` : ""}
                {item.kind === "inbox" ? ` · ${item.status === "read" ? "已读" : "未读"}` : ""}
                {` · ${item.updated_at.slice(0, 10)}`}
              </span>
              {openId !== item.id && item.body && <p>{item.body.length > 140 ? `${item.body.slice(0, 140)}…` : item.body}</p>}
            </button>
            {openId === item.id && <div className="library-document-body">{item.body || "（无正文）"}</div>}
            <div className="mobile-library-row-actions">
              <button type="button" onClick={() => setOpenId(openId === item.id ? null : item.id)}>{openId === item.id ? <><ChevronUp size={14} />收起</> : <><ChevronDown size={14} />全文</>}</button>
              <button type="button" onClick={() => edit(item)}><Pencil size={14} />编辑</button>
              <button type="button" onClick={() => void shareAsFile(item)} aria-label={`分享 ${item.title} 为文件`}><Share2 size={14} />分享</button>
              {item.kind === "inbox" && (
                <button type="button" onClick={() => void toggleRead(item)}>{item.status === "read" ? "标未读" : "标已读"}</button>
              )}
              <button type="button" className="mobile-delete" onClick={() => void remove(item.id)} aria-label={`删除 ${item.title}`}><Trash2 size={15} /></button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
