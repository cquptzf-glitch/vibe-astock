import { CalendarDays, CheckCircle2, Circle, Plus, RefreshCw, Trash2 } from "lucide-react";
import { WebSelect, DateField } from "./controls";
import { useEffect, useMemo, useState } from "react";
import { ExchangeCalendar } from "./ExchangeCalendar";
import { chinaToday } from "./emotionData";
import "./research-workflow.css";

type EventKind = "company" | "ledger" | "macro" | "reminder";
type EventStatus = "open" | "done";

type CalendarEvent = {
  id: number;
  date: string;
  kind: EventKind;
  title: string;
  detail: string;
  symbol: string | null;
  status: EventStatus;
  source: string;
  created_at: string;
  updated_at: string;
  overdue?: boolean;
};

type CalendarResult = {
  from: string;
  to: string;
  today: string;
  count: number;
  items: CalendarEvent[];
};

const KIND_LABEL: Record<EventKind, string> = {
  company: "公司",
  ledger: "交易",
  macro: "宏观",
  reminder: "提醒",
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

export function calendarDate(offsetDays = 0, now = new Date()): string {
  return chinaToday(new Date(now.getTime() + offsetDays * 86_400_000));
}

type SyncBatch = { count: number; gaps: { source: string; message: string }[]; note: string; processed_symbols: number; total_symbols: number; next_offset: number | null; watchlist_revision: string };
export async function syncCalendarBatches(symbol: string, request: (body: Record<string, unknown>) => Promise<SyncBatch>, progress: (text: string) => void) {
  let offset = 0; let revision = ""; let count = 0; const gaps: SyncBatch["gaps"] = [];
  do {
    const result = await request({ symbol, offset, ...(revision ? { watchlist_revision: revision } : {}) });
    if (!Number.isSafeInteger(result.processed_symbols) || result.processed_symbols <= 0 || !Number.isSafeInteger(result.total_symbols) || result.total_symbols < offset + result.processed_symbols || typeof result.watchlist_revision !== "string" || !Array.isArray(result.gaps)) throw new Error("同步接口未返回完整批次信息，已同步的事件保留，请重试。");
    if (revision && revision !== result.watchlist_revision) throw new Error("自选名单在同步期间发生变化，请重新同步。");
    revision = result.watchlist_revision; count += result.count; gaps.push(...result.gaps);
    const processed = offset + result.processed_symbols;
    progress(`已检查 ${processed} / ${result.total_symbols} 只，保存 ${count} 项。${gaps.length ? `来源缺口：${gaps.map(g => `${g.source}：${g.message}`).join("；")}` : ""}`);
    if (result.next_offset === null && processed === result.total_symbols) break;
    if (result.next_offset !== processed || processed >= result.total_symbols) throw new Error("同步批次不连续，已同步的事件保留，请重试。");
    offset = processed;
  } while (true);
  return { count, gaps };
}

export function CalendarPanel() {
  const [windowDays, setWindowDays] = useState<30 | 90>(30);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [today, setToday] = useState(calendarDate());
  const [date, setDate] = useState(calendarDate());
  const [kind, setKind] = useState<EventKind>("reminder");
  const [title, setTitle] = useState("");
  const [detail, setDetail] = useState("");
  const [symbol, setSymbol] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [syncing, setSyncing] = useState(false);
  const [syncNote, setSyncNote] = useState("");

  async function syncSources() {
    setSyncing(true); setError(""); setSyncNote("");
    try {
      const result = await syncCalendarBatches(symbol.trim(), body => api<SyncBatch>("/mobile/calendar/sync", { method: "POST", body: JSON.stringify(body) }), setSyncNote);
      setSyncNote(`已同步 ${result.count} 项。${result.gaps.length ? `部分来源未成功：${result.gaps.map(g => `${g.source}：${g.message}`).join("；")}` : symbol.trim() ? "已检查指定股票。" : "全部自选已检查。"}`);
      await load();
    } catch (e) { await load(); setError(e instanceof Error ? e.message : String(e)); }
    finally { setSyncing(false); }
  }
  async function remind(item: CalendarEvent, enabled: boolean) {
    try {
      const result = await api<{ scheduler?: { supported?: boolean } }>("/mobile/calendar/reminder", { method: "POST", body: JSON.stringify({ id: item.id, enabled, hour: 9 }) });
      setSyncNote(enabled ? `已设置「${item.title}」当天北京时间 09:00 后提醒。${result.scheduler?.supported === false ? "当前环境不支持原生调度。" : "需要允许系统通知，后台任务可能受省电影响而延迟。"}` : `已关闭「${item.title}」的提醒。`);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }

  async function load() {
    setLoading(true);
    setError("");
    try {
      const from = calendarDate(-7);
      const to = calendarDate(windowDays);
      const result = await api<CalendarResult>(`/mobile/calendar?from=${from}&to=${to}`);
      setEvents(result.items);
      setToday(result.today);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void load(); }, [windowDays]);

  async function save() {
    if (!title.trim() || saving) return;
    setSaving(true);
    setError("");
    try {
      await api("/mobile/calendar", {
        method: "POST",
        body: JSON.stringify({
          date,
          kind,
          title: title.trim(),
          detail: detail.trim(),
          symbol: symbol.trim() || null,
          status: "open",
          source: "manual",
        }),
      });
      setTitle("");
      setDetail("");
      setSymbol("");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  async function toggle(item: CalendarEvent) {
    setError("");
    try {
      await api("/mobile/calendar", {
        method: "POST",
        body: JSON.stringify({
          ...item,
          status: item.status === "done" ? "open" : "done",
        }),
      });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function remove(id: number) {
    setError("");
    try {
      await api("/mobile/calendar", { method: "DELETE", body: JSON.stringify({ id }) });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const groups = useMemo(() => {
    const map = new Map<string, CalendarEvent[]>();
    events.forEach((item) => map.set(item.date, [...(map.get(item.date) ?? []), item]));
    return Array.from(map.entries());
  }, [events]);

  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title">
        <div><h1>事件日历</h1><p>公司、交易、宏观和待验证事项统一落本机 SQLite；过期未完成事件自动标记。</p></div>
        <button type="button" className="mobile-delete" onClick={() => void load()} disabled={loading} aria-label="刷新事件日历">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="discovery-tabs calendar-window-tabs">
        <button type="button" className={windowDays === 30 ? "active" : ""} onClick={() => setWindowDays(30)}>近 30 天</button>
        <button type="button" className={windowDays === 90 ? "active" : ""} onClick={() => setWindowDays(90)}>近 90 天</button>
      </div>
      <ExchangeCalendar />
      <button type="button" className="discovery-action" disabled={syncing} onClick={() => void syncSources()}>{syncing ? "正在同步公告与预约披露…" : symbol.trim() ? `同步 ${symbol.trim()} 公告与披露日期` : "同步自选公告与披露日期"}</button>
      {syncNote && <p role="status" className="discovery-footnote">{syncNote}</p>}

      <details className="mobile-quote-card calendar-editor">
        <summary><CalendarDays size={16} /> 新增日历事件</summary>
        <div className="calendar-editor-grid">
          <DateField value={date} onChange={(event) => setDate(event.target.value)} />
          <WebSelect value={kind} onChange={(event) => setKind(event.target.value as EventKind)}>
            {(Object.keys(KIND_LABEL) as EventKind[]).map((key) => <option key={key} value={key}>{KIND_LABEL[key]}</option>)}
          </WebSelect>
          <input value={symbol} onChange={(event) => setSymbol(event.target.value)} placeholder="股票代码（可选）" maxLength={32} />
        </div>
        <input className="calendar-title-input" value={title} onChange={(event) => setTitle(event.target.value)} placeholder="事件标题" maxLength={500} />
        <textarea value={detail} onChange={(event) => setDetail(event.target.value)} placeholder="验证条件、要点或备注（可选）" rows={3} maxLength={20000} />
        <button type="button" className="candidate-run" onClick={() => void save()} disabled={!title.trim() || saving}>
          <Plus size={14} /> {saving ? "保存中…" : "加入日历"}
        </button>
      </details>

      {error && <div className="mobile-error discovery-message">{error}</div>}
      {!loading && events.length === 0 && <div className="mobile-empty">当前时间窗没有事件。</div>}

      <div className="calendar-groups">
        {groups.map(([groupDate, rows]) => (
          <article className="mobile-quote-card calendar-day" key={groupDate}>
            <div className="calendar-day-head">
              <strong>{groupDate}</strong>
              <span>{groupDate === today ? "今天" : rows.some((item) => item.overdue) ? "有逾期" : `${rows.length} 项`}</span>
            </div>
            {rows.map((item) => (
              <div className={`calendar-event ${item.status === "done" ? "done" : ""} ${item.overdue ? "overdue" : ""}`} key={item.id}>
                <button type="button" className="calendar-status" onClick={() => void toggle(item)} aria-label={item.status === "done" ? "恢复未完成" : "标记完成"}>
                  {item.status === "done" ? <CheckCircle2 size={18} /> : <Circle size={18} />}
                </button>
                <div>
                  <strong>{item.title}</strong>
                  <span>{KIND_LABEL[item.kind]}{item.symbol ? ` · ${item.symbol}` : ""}{item.overdue ? " · 已逾期" : ""}</span>
                  {item.detail && <p>{item.detail}</p>}
                </div>
                {item.status === "open" && <><button type="button" className="discovery-action" onClick={() => void remind(item, true)}>当天提醒</button><button type="button" className="discovery-action" onClick={() => void remind(item, false)}>取消提醒</button></>}
                <button type="button" className="mobile-delete" onClick={() => void remove(item.id)} aria-label={`删除 ${item.title}`}><Trash2 size={15} /></button>
              </div>
            ))}
          </article>
        ))}
      </div>
    </section>
  );
}
