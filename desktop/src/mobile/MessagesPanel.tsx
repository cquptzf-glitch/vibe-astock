import { usePageActive } from "./PageActivity.ts";
/**
 * 消息(「我的」):盘前提醒、收盘摘要、自选异动。没有后台推送——打开 App 或点「刷新」时按当天行情生成;
 * 内容只列已经发生的读数,不含持仓、成本、盈亏,不给买卖建议。
 */
import { Bell, CheckCheck, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { askConfirm } from "./controls";
import type { MobilePage } from "./mobileRoutes";
import "./messages.css";

export interface Message { id: number; kind: string; day: string; title: string; body: string; route: string; read: boolean; created_at: string; updated_at: string }
export interface MessageList { messages: Message[]; unread: number }
export interface GenerateResult { created: string[]; skipped: { kind: string; why: string }[]; list: MessageList }

export const KIND_LABEL: Record<string, string> = { pre_open: "盘前提醒", close_summary: "收盘摘要", watch_move: "自选异动" };
const ROUTES = new Set<string>(["emotion", "verify", "monitor", "intraday", "watch"]);
export const routeOf = (m: Message): MobilePage | null => (ROUTES.has(m.route) ? (m.route as MobilePage) : null);

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch("/api" + path, { method, headers: { Accept: "application/json", "Content-Type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data as T;
}

/** 启动时与进入消息页时都跑一次:取列表、生成当天该有的消息。生成失败不影响列表 */
export async function refreshMessages(): Promise<{ list: MessageList; created: string[]; skipped: { kind: string; why: string }[] }> {
  const list = await call<MessageList>("GET", "/mobile/messages");
  try {
    const out = await call<GenerateResult>("POST", "/mobile/messages/generate");
    return { list: out.list, created: out.created, skipped: out.skipped };
  } catch {
    return { list, created: [], skipped: [{ kind: "all", why: "生成失败,先显示已有的消息" }] };
  }
}

export function MessageItem({ message, onOpen, onRead, onDelete }: { message: Message; onOpen: (page: MobilePage) => void; onRead: () => void; onDelete: () => void }) {
  const target = routeOf(message);
  return <li className={"msg" + (message.read ? "" : " unread")}>
    <div className="msg-head"><em>{KIND_LABEL[message.kind] ?? message.kind}</em><strong>{message.title}</strong>{!message.read && <i aria-label="未读" />}</div>
    <p className="msg-body">{message.body}</p>
    <div className="msg-actions">
      {target && <button type="button" onClick={() => { onRead(); onOpen(target); }}>去看看</button>}
      {!message.read && <button type="button" onClick={onRead}>标为已读</button>}
      <button type="button" aria-label={`删除 ${message.title}`} onClick={onDelete}><Trash2 size={13} /></button>
    </div>
  </li>;
}

export function MessagesView({ list, onOpen, onRead, onDelete }: { list: MessageList; onOpen: (page: MobilePage) => void; onRead: (id: number) => void; onDelete: (id: number) => void }) {
  if (list.messages.length === 0) return <p className="mobile-empty" role="status">还没有消息。交易日打开 App 或点「刷新」时,会按当天行情生成盘前提醒、收盘摘要和自选异动(不是实时推送)。</p>;
  return <ul className="msg-list" aria-label="消息列表">{list.messages.map((m) => <MessageItem key={m.id} message={m} onOpen={onOpen} onRead={() => onRead(m.id)} onDelete={() => onDelete(m.id)} />)}</ul>;
}

export function MessagesPanel({ onNavigate, onUnread }: { onNavigate: (page: MobilePage) => void; onUnread?: (count: number) => void }) {
  const active = usePageActive();
  const [list, setList] = useState<MessageList | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [note, setNote] = useState("");
  const apply = useCallback((next: MessageList) => { setList(next); onUnread?.(next.unread); }, [onUnread]);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const out = await refreshMessages();
      apply(out.list);
      setNote(out.created.length ? `刚生成:${out.created.map((k) => KIND_LABEL[k] ?? k).join("、")}` : out.skipped.length ? out.skipped.map((s) => `${KIND_LABEL[s.kind] ?? s.kind}:${s.why}`).join(";") : "已是最新");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setLoading(false); }
  }, [apply]);
  useEffect(() => { if (active) void refresh(); }, [refresh, active]);

  const act = async (body: unknown) => {
    try { apply(await call<MessageList>("POST", "/mobile/messages", body)); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  };

  return <section className="mobile-section msg-page">
    <div className="mobile-section-title"><div><h1><Bell size={18} /> 消息</h1><p>盘前提醒、收盘摘要、自选异动</p></div>
      <span className="msg-tools">
        <button type="button" aria-label="全部已读" disabled={!list || list.unread === 0} onClick={() => void act({ action: "read_all" })}><CheckCheck size={15} /></button>
        <button type="button" aria-label="清空消息" disabled={!list || list.messages.length === 0} onClick={async () => { if (await askConfirm("清空全部消息?")) void act({ action: "clear" }); }}><Trash2 size={15} /></button>
        <button type="button" aria-label="刷新消息" disabled={loading} onClick={() => void refresh()}><RefreshCw size={15} className={loading ? "spin" : ""} /></button>
      </span></div>
    {error && <div className="mobile-error" role="alert">{error}</div>}
    {note && <p className="msg-note">{note}</p>}
    {list && <MessagesView list={list} onOpen={onNavigate} onRead={(id) => void act({ action: "read", id })} onDelete={(id) => void act({ action: "delete", id })} />}
    <p className="msg-note">没有后台推送:只在打开 App 或点刷新时按当天行情生成;只列已经发生的读数,不含持仓与盈亏,不构成买卖建议。</p>
  </section>;
}
