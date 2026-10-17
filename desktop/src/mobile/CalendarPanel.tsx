import { CalendarDays, CheckCircle2, Circle, Plus, RefreshCw, Trash2 } from "lucide-react";
import { WebSelect, DateField } from "./controls";
import { useEffect, useMemo, useState } from "react";

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

function localDate(offsetDays = 0): string {
  const date = new Date();
  date.setDate(date.getDate() + offsetDays);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function CalendarPanel() {
  const [windowDays, setWindowDays] = useState<30 | 90>(30);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [today, setToday] = useState(localDate());
  const [date, setDate] = useState(localDate());
  const [kind, setKind] = useState<EventKind>("reminder");
  const [title, setTitle] = useState("");
  const [detail, setDetail] = useState("");
  const [symbol, setSymbol] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    setLoading(true);
    setError("");
    try {
      const from = localDate(-7);
      const to = localDate(windowDays);
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

      <article className="mobile-quote-card calendar-editor">
        <div className="mobile-card-label"><CalendarDays size={13} /> 新增事件</div>
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
      </article>

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
                <button type="button" className="mobile-delete" onClick={() => void remove(item.id)} aria-label={`删除 ${item.title}`}><Trash2 size={15} /></button>
              </div>
            ))}
          </article>
        ))}
      </div>
    </section>
  );
}
