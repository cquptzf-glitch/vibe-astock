import { BrainCircuit, Eraser, RefreshCw, Send, ShieldCheck } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import "./ai-chat.css";

type ChatMessage = {
  id: number;
  role: "user" | "assistant";
  content: string;
  created_at: string;
};

type ChatHistory = {
  messages: ChatMessage[];
  count: number;
  storage: string;
  local_data_attached: boolean;
};

type ChatSendResult = {
  status: string;
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

export function AiChatPanel() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [config, setConfig] = useState<AiConfig | null>(null);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const endRef = useRef<HTMLDivElement | null>(null);

  async function load() {
    setLoading(true);
    setError("");
    try {
      const [history, nextConfig] = await Promise.all([
        api<ChatHistory>("/mobile/ai/chat"),
        api<AiConfig>("/mobile/ai/config"),
      ]);
      setMessages(history.messages);
      setConfig(nextConfig);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void load(); }, []);
  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [messages, sending]);

  async function send() {
    const content = draft.trim();
    if (!content || sending) return;
    setSending(true);
    setError("");
    setNotice("");
    try {
      const result = await api<ChatSendResult>("/mobile/ai/chat", {
        method: "POST",
        body: JSON.stringify({ content }),
      });
      setMessages(result.history);
      setDraft("");
      setNotice(`已通过 ${result.model} 完成回复；未附加自选、持仓、交易日志或日复盘数据。`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  }

  async function clearHistory() {
    if (messages.length === 0) return;
    if (!window.confirm("清空手机本地保存的 AI 对话记录？此操作不可恢复。")) return;
    setLoading(true);
    setError("");
    setNotice("");
    try {
      await api("/mobile/ai/chat", { method: "DELETE" });
      setMessages([]);
      setNotice("本地 AI 对话记录已清空。");
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
        {!loading && messages.length === 0 && (
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

        {sending && (
          <article className="ai-chat-message assistant pending">
            <div className="ai-chat-message-meta"><strong>AI</strong><span>正在生成</span></div>
            <div className="ai-chat-thinking"><RefreshCw size={15} className="spin" /> 正在调用你配置的模型 API…</div>
          </article>
        )}
        <div ref={endRef} />
      </div>

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
          <button onClick={() => void send()} disabled={sending || !config?.configured || !draft.trim()}>
            {sending ? <RefreshCw size={15} className="spin" /> : <Send size={15} />} 发送
          </button>
        </div>
      </div>
    </section>
  );
}
