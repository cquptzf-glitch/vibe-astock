import { useEffect, useState } from "react";
import { Archive, Inbox, NotebookPen, Plus, RefreshCw, Save, Trash2 } from "lucide-react";

interface DocumentItem {
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

type Kind = DocumentItem["kind"];

const META: Record<Kind, { label: string; hint: string }> = {
  note: { label: "研究记录", hint: "复盘、要点、问答和研究结论沉淀在本机。" },
  report: { label: "报告", hint: "保存报告正文、研究摘要或外部材料提取文本。" },
  inbox: { label: "收件箱", hint: "保存稍后处理的线索、事件和待验证事项。" },
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

export function LibraryPanel() {
  const [kind, setKind] = useState<Kind>("note");
  const [items, setItems] = useState<DocumentItem[]>([]);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [category, setCategory] = useState("");
  const [symbol, setSymbol] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

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
    void load(kind);
  }, [kind]);

  function resetEditor() {
    setEditingId(null);
    setTitle("");
    setBody("");
    setCategory("");
    setSymbol("");
  }

  function edit(item: DocumentItem) {
    setEditingId(item.id);
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

  return (
    <section className="mobile-section">
      <div className="mobile-section-title">
        <div>
          <h1>本地资料库</h1>
          <p>研究记录、报告和收件箱统一保存在 Android SQLite，不依赖浏览器缓存。</p>
        </div>
        <button type="button" className="mobile-delete" onClick={() => void load(kind)} aria-label="刷新资料库">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="mobile-stock-tabs" role="tablist" aria-label="本地资料库">
        {(Object.keys(META) as Kind[]).map((key) => (
          <button key={key} role="tab" aria-selected={kind === key} className={kind === key ? "active" : ""} onClick={() => setKind(key)}>
            {key === "note" ? <NotebookPen size={13} /> : key === "report" ? <Archive size={13} /> : <Inbox size={13} />}
            {META[key].label}
          </button>
        ))}
      </div>

      <p className="mobile-source-line" style={{ marginTop: 8 }}>{META[kind].hint}</p>
      {error && <div className="mobile-error">{error}</div>}

      <article className="mobile-quote-card" style={{ marginTop: 12 }}>
        <div className="mobile-card-label">{editingId ? `编辑 #${editingId}` : `新建${META[kind].label}`}</div>
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          maxLength={500}
          placeholder="标题"
          className="mobile-library-input"
        />
        <div className="mobile-library-inline">
          <input value={category} onChange={(event) => setCategory(event.target.value)} maxLength={100} placeholder="分类（可选）" className="mobile-library-input" />
          <input value={symbol} onChange={(event) => setSymbol(event.target.value)} maxLength={32} placeholder="关联代码（可选）" className="mobile-library-input" />
        </div>
        <textarea
          value={body}
          onChange={(event) => setBody(event.target.value)}
          maxLength={250000}
          rows={7}
          placeholder="正文 / 摘要 / 待办内容"
          className="mobile-library-textarea"
        />
        <div className="mobile-library-actions">
          {editingId && <button type="button" onClick={resetEditor}>取消编辑</button>}
          <button type="button" onClick={() => void save()} disabled={!title.trim() || saving}>
            {editingId ? <Save size={15} /> : <Plus size={15} />}{saving ? "保存中…" : editingId ? "保存修改" : "添加"}
          </button>
        </div>
      </article>

      <div className="mobile-watch-list" style={{ marginTop: 12 }}>
        {loading && items.length === 0 ? (
          <div className="mobile-empty"><RefreshCw className="spin" size={17} /> 正在加载…</div>
        ) : items.length === 0 ? (
          <div className="mobile-empty">还没有{META[kind].label}。</div>
        ) : items.map((item) => (
          <div className="mobile-library-item" key={item.id}>
            <button type="button" className="mobile-library-main" onClick={() => edit(item)}>
              <strong>{item.title}</strong>
              <span>
                {item.category || META[item.kind].label}
                {item.symbol ? ` · ${item.symbol}` : ""}
                {item.kind === "inbox" ? ` · ${item.status === "read" ? "已读" : "未读"}` : ""}
              </span>
              {item.body && <p>{item.body.length > 140 ? `${item.body.slice(0, 140)}…` : item.body}</p>}
            </button>
            <div className="mobile-library-row-actions">
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
