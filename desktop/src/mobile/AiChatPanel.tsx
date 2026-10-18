import { BrainCircuit, Eraser, MessageSquarePlus, Pencil, RefreshCw, Send, ShieldCheck, Square, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { takeQueuedAiPrompt, useAiPageContext } from "./aiPageContext";

import { askConfirm, WebSelect } from "./controls";
import "./ai-chat.css";

export type ChatMessage = {
  id: number;
  role: "user" | "assistant";
  content: string;
  created_at: string;
};

type ChatHistory = {
  thread: number;
  messages: ChatMessage[];
  count: number;
  storage: string;
  local_data_attached: boolean;
};

export type ThreadInfo = { id: number; title: string; updated_at: string; count: number };
type ThreadList = { threads: ThreadInfo[]; default_thread: number; thread?: number };

type ChatSendResult = {
  status: string;
  thread: number;
  model: string;
  base_url: string;
  sent_characters: number;
  local_data_attached: boolean;
  response: string;
  history: ChatMessage[];
  generated_at: string;
};

type AiConfig = {
  model: string;
  configured: boolean;
  key_present: boolean;
};

const THREAD_KEY = "vibe.ai.thread";
const DELTA_EVENT = "ai-chat-delta";

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

function timeLabel(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

type TauriEvents = { listen: (name: string, handler: (event: { payload: { request_id?: string; delta?: string } }) => void) => Promise<() => void> };
/** 有 Tauri 事件通道才能流式;浏览器预览里没有,走一次性回复 */
export function eventApi(): TauriEvents | null {
  if (typeof window === "undefined") return null;
  const tauri = window.__TAURI__ as unknown as { event?: TauriEvents } | undefined;
  return tauri?.event?.listen ? tauri.event : null;
}

export function newRequestId(): string {
  const random = typeof crypto !== "undefined" && crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2);
  return `chat-${Date.now().toString(36)}-${random.slice(0, 12)}`;
}

function savedThread(): number {
  try {
    const n = Number(window.localStorage.getItem(THREAD_KEY));
    return Number.isInteger(n) && n > 0 ? n : 1;
  } catch { return 1; }
}

export function ThreadBar({ threads, current, busy, onSelect, onCreate, onRename, onDelete }: {
  threads: ThreadInfo[]; current: number; busy: boolean;
  onSelect: (id: number) => void; onCreate: () => void; onRename: () => void; onDelete: () => void;
}) {
  const active = threads.find((t) => t.id === current);
  return <div className="ai-chat-threads" aria-label="对话列表">
    <WebSelect aria-label="选择对话" value={current} onChange={(e) => onSelect(Number(e.target.value))} disabled={busy}>
      {threads.map((t) => <option key={t.id} value={t.id}>{t.title}({t.count})</option>)}
    </WebSelect>
    <button type="button" onClick={onCreate} disabled={busy} aria-label="新对话"><MessageSquarePlus size={15} /></button>
    <button type="button" onClick={onRename} disabled={busy || !active} aria-label="重命名对话"><Pencil size={15} /></button>
    <button type="button" onClick={onDelete} disabled={busy || !active || current === 1} aria-label="删除对话" title={current === 1 ? "默认对话不能删除,可以清空" : undefined}><Trash2 size={15} /></button>
  </div>;
}

export function AiChatPanel() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [threads, setThreads] = useState<ThreadInfo[]>([]);
  const [thread, setThread] = useState(savedThread);
  const [config, setConfig] = useState<AiConfig | null>(null);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [documentsEnabled,setDocumentsEnabled]=useState(false);
  const [toolsEnabled, setToolsEnabled] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [pendingUser, setPendingUser] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const page = useAiPageContext();
  const [attach, setAttach] = useState(false);
  const endRef = useRef<HTMLDivElement | null>(null);
  const requestRef = useRef<string>("");

  async function load(target = thread) {
    setLoading(true);
    setError("");
    try {
      const [list, nextConfig] = await Promise.all([
        api<ThreadList>("/mobile/ai/threads"),
        api<AiConfig>("/mobile/ai/config"),
      ]);
      const exists = list.threads.some((t) => t.id === target);
      const id = exists ? target : list.default_thread;
      const history = await api<ChatHistory>(`/mobile/ai/chat?thread=${id}`);
      setThreads(list.threads);
      setThread(id);
      setMessages(history.messages);
      setConfig(nextConfig);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void load(); }, []);
  useEffect(() => { try { window.localStorage.setItem(THREAD_KEY, String(thread)); } catch { /* 可选 */ } }, [thread]);
  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [messages, sending, streamText]);
  // 从页面上的「问 AI」按钮跳过来:预填问题并附带那一页(用户还可以取消勾选、改问题)
  useEffect(() => { const q = takeQueuedAiPrompt(); if (q) { setDraft(q); setAttach(true); } }, []);

  async function send() {
    const content = draft.trim();
    if (!content || sending) return;
    setSending(true);
    setError("");
    setNotice("");
    setStreamText("");
    setPendingUser(content);
    const events = eventApi();
    const pageContext = attach && page ? { title: page.title, text: page.text } : undefined;
    const requestId = newRequestId();
    requestRef.current = requestId;
    let unlisten: (() => void) | null = null;
    try {
      let result: ChatSendResult;
      if (events && !toolsEnabled) {
        unlisten = await events.listen(DELTA_EVENT, (event) => {
          if (event.payload.request_id === requestId && event.payload.delta) setStreamText((t) => t + event.payload.delta);
        });
        result = await api<ChatSendResult>("/mobile/ai/chat/stream", { method: "POST", body: JSON.stringify({ content, thread_id: thread, request_id: requestId, page_context: pageContext }) });
      } else {
        result = await api<ChatSendResult>(toolsEnabled ? "/mobile/ai/chat/tools" : "/mobile/ai/chat", { method: "POST", body: JSON.stringify({ content, thread_id: thread, request_id: requestId, page_context: pageContext, tools_enabled: toolsEnabled, documents_enabled: toolsEnabled && documentsEnabled }) });
      }
      setMessages(result.history);
      setDraft("");
      setNotice(pageContext ? `已通过 ${result.model} 完成回复；按你的选择附带了页面「${pageContext.title}」的内容(只用于这一问);未附加自选、持仓、交易日志或日复盘数据。` : `已通过 ${result.model} 完成回复；未附加自选、持仓、交易日志或日复盘数据。`);
      if (toolsEnabled) setNotice(`已通过 ${result.model} 完成受控工具对话，实际执行记录在回复末尾；未执行的工具不算成功。`);
      void api<ThreadList>("/mobile/ai/threads").then((list) => setThreads(list.threads)).catch(() => undefined);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      if (message.includes("已取消")) setNotice("已停止,这一轮没有保存(你的问题还留在输入框里)。");
      else setError(message);
    } finally {
      unlisten?.();
      requestRef.current = "";
      setSending(false);
      setStreamText("");
      setPendingUser("");
    }
  }

  async function stop() {
    const id = requestRef.current;
    if (!id) return;
    try { await api("/mobile/ai/chat/cancel", { method: "POST", body: JSON.stringify({ request_id: id }) }); } catch { /* 取消失败就等它自己结束 */ }
  }

  async function select(id: number) {
    setNotice("");
    setStreamText("");
    await load(id);
  }

  async function createThread() {
    setError("");
    try {
      const out = await api<ThreadList>("/mobile/ai/threads", { method: "POST", body: JSON.stringify({ action: "create" }) });
      await load(out.thread ?? thread);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }

  async function renameThread(title: string) {
    try {
      await api("/mobile/ai/threads", { method: "POST", body: JSON.stringify({ action: "rename", id: thread, title }) });
      setRenaming(null);
      await load(thread);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }

  async function deleteThread() {
    if (!await askConfirm("删除这个对话和里面的全部消息？此操作不可恢复。")) return;
    try {
      await api("/mobile/ai/threads", { method: "POST", body: JSON.stringify({ action: "delete", id: thread }) });
      await load(1);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }

  async function clearHistory() {
    if (messages.length === 0) return;
    if (!await askConfirm("清空这个对话里的消息？此操作不可恢复。")) return;
    setLoading(true);
    setError("");
    setNotice("");
    try {
      await api(`/mobile/ai/chat?thread=${thread}`, { method: "DELETE" });
      setMessages([]);
      setNotice("这个对话的记录已清空。");
      void api<ThreadList>("/mobile/ai/threads").then((list) => setThreads(list.threads)).catch(() => undefined);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="ai-chat-panel">
      <div className="ai-chat-title">
        <div>
          <span><BrainCircuit size={15} /> AI 对话</span>
          <h1>独立聊天 · 默认不读取本地投资数据</h1>
          <p>仅发送此聊天窗口中的显式消息；聊天记录保存在手机 SQLite。</p>
        </div>
        <button onClick={() => void load()} disabled={loading || sending} aria-label="刷新 AI 对话">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="ai-chat-privacy">
        <ShieldCheck size={16} />
        <span>不会自动附加自选、持仓、成交明细、交易备注或日复盘。需要分析这些数据时请使用“交易复盘”功能，并按其发送范围确认。</span>
      </div>

      {error && <div className="ai-chat-error">{error}</div>}
      {notice && <div className="ai-chat-notice">{notice}</div>}

      {!config?.configured && !loading && (
        <div className="ai-chat-config-warning">
          <strong>还没有配置模型 API</strong>
          <span>请先到“设置/交易复盘”中填写 OpenAI-compatible Base URL、模型名称和可选 API Key。</span>
        </div>
      )}

      <ThreadBar threads={threads} current={thread} busy={loading || sending} onSelect={(id) => void select(id)} onCreate={() => void createThread()}
        onRename={() => setRenaming(threads.find((t) => t.id === thread)?.title ?? "")} onDelete={() => void deleteThread()} />
      {renaming !== null && <div className="ai-chat-rename">
        <input value={renaming} maxLength={30} aria-label="对话标题" onChange={(e) => setRenaming(e.target.value)} />
        <button type="button" disabled={!renaming.trim()} onClick={() => void renameThread(renaming)}>保存</button>
        <button type="button" onClick={() => setRenaming(null)}>取消</button>
      </div>}

      <div className="ai-chat-toolbar">
        <div>
          <span>{config?.configured ? `模型：${config.model}` : "模型未配置"}</span>
          <small>{messages.length} 条本地消息</small>
        </div>
        <button onClick={() => void clearHistory()} disabled={loading || sending || messages.length === 0}>
          <Eraser size={14} /> 清空
        </button>
      </div>

      <div className="ai-chat-thread" aria-live="polite">
        {loading && messages.length === 0 && (
          <div className="ai-chat-empty"><RefreshCw size={18} className="spin" /> 正在读取本地会话…</div>
        )}
        {!loading && messages.length === 0 && !sending && (
          <div className="ai-chat-empty">
            <BrainCircuit size={25} />
            <strong>开始一个独立投研对话</strong>
            <span>可以讨论概念、方法、你手动贴入的数据或研究思路；需要最新市场数据时请先从行情页面获取后再贴入。</span>
          </div>
        )}

        {messages.map((message) => (
          <article className={`ai-chat-message ${message.role}`} key={message.id}>
            <div className="ai-chat-message-meta">
              <strong>{message.role === "user" ? "你" : "AI"}</strong>
              <span>{timeLabel(message.created_at)}</span>
            </div>
            <div className="ai-chat-message-content">{message.content}</div>
          </article>
        ))}

        {sending && pendingUser && (
          <article className="ai-chat-message user pending">
            <div className="ai-chat-message-meta"><strong>你</strong><span>刚刚</span></div>
            <div className="ai-chat-message-content">{pendingUser}</div>
          </article>
        )}
        {sending && (
          <article className="ai-chat-message assistant pending">
            <div className="ai-chat-message-meta"><strong>AI</strong><span>{streamText ? "正在输出" : "正在生成"}</span></div>
            {streamText
              ? <div className="ai-chat-message-content" aria-label="流式回复">{streamText}</div>
              : <div className="ai-chat-thinking"><RefreshCw size={15} className="spin" /> 正在调用你配置的模型 API…</div>}
          </article>
        )}
        <div ref={endRef} />
      </div>

      {page && <label className="ai-chat-attach"><input type="checkbox" checked={attach} disabled={sending} onChange={(e) => setAttach(e.target.checked)} aria-label="附带当前页面" />
        <span>附带页面「{page.title}」的内容({page.text.length} 字)<small>只随这一问发给你配置的模型,不存进对话历史;个人成交、持仓、盈亏页面不会登记。</small></span></label>}
      <label className="ai-chat-attach"><input type="checkbox" checked={toolsEnabled} disabled={sending} onChange={(e) => setToolsEnabled(e.target.checked)} /><span>允许公开数据与计算工具<small>只允许目录内取数与四则运算；本轮最多八次调用，完成后统一显示回复。</small></span></label>
      {toolsEnabled && <label className="ai-chat-attach"><input type="checkbox" checked={documentsEnabled} disabled={sending} onChange={e=>setDocumentsEnabled(e.target.checked)} /><span>检索本机资料库<small>仅把匹配片段交给模型；引用须与本轮结果一致，新导入 PDF 包含原始页码。</small></span></label>}
      <div className="ai-chat-composer">
        <textarea
          value={draft}
          maxLength={8000}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void send();
            }
          }}
          placeholder="输入问题；Shift + Enter 换行"
          disabled={sending}
        />
        <div className="ai-chat-compose-foot">
          <span>{draft.length}/8000</span>
          {sending
            ? <button onClick={() => void stop()} aria-label="停止生成"><Square size={14} /> 停止</button>
            : <button onClick={() => void send()} disabled={!config?.configured || !draft.trim()}><Send size={15} /> 发送</button>}
        </div>
      </div>
    </section>
  );
}
