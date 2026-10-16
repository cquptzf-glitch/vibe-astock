import { Bell, BellOff, RefreshCw, Save, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import "./reminders.css";

type ReminderKind = "price_above" | "price_below" | "stop_near" | "target_near" | "daily_review";

type ReminderRule = {
  id: number;
  kind: ReminderKind;
  symbol: string | null;
  name: string;
  threshold: number | null;
  tolerance_pct: number;
  enabled: boolean;
  cooldown_minutes: number;
  review_hour: number | null;
  quote_key: string | null;
  created_at: string;
  updated_at: string;
};

type ReminderList = { count: number; items: ReminderRule[] };

type Draft = {
  id: number | null;
  kind: ReminderKind;
  symbol: string;
  name: string;
  threshold: string;
  tolerance: string;
  cooldown: string;
  reviewHour: string;
  enabled: boolean;
};

const LABELS: Record<ReminderKind, string> = {
  price_above: "价格上穿",
  price_below: "价格下破",
  stop_near: "接近止损",
  target_near: "接近目标",
  daily_review: "收盘复盘",
};

const EMPTY: Draft = {
  id: null,
  kind: "price_above",
  symbol: "",
  name: "",
  threshold: "",
  tolerance: "1",
  cooldown: "60",
  reviewHour: "16",
  enabled: true,
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

export function ReminderPanel() {
  const [items, setItems] = useState<ReminderRule[]>([]);
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const stockRule = draft.kind !== "daily_review";
  const enabledCount = useMemo(() => items.filter((item) => item.enabled).length, [items]);

  async function load() {
    setLoading(true);
    setError("");
    try {
      const result = await api<ReminderList>("/mobile/reminders");
      setItems(result.items);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void load(); }, []);

  function edit(item: ReminderRule) {
    setDraft({
      id: item.id,
      kind: item.kind,
      symbol: item.symbol ?? "",
      name: item.name,
      threshold: item.threshold?.toString() ?? "",
      tolerance: item.tolerance_pct.toString(),
      cooldown: item.cooldown_minutes.toString(),
      reviewHour: item.review_hour?.toString() ?? "16",
      enabled: item.enabled,
    });
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  async function save() {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      await api("/mobile/reminders", {
        method: "POST",
        body: JSON.stringify({
          ...(draft.id ? { id: draft.id } : {}),
          kind: draft.kind,
          symbol: stockRule ? draft.symbol.trim() : null,
          name: draft.name.trim(),
          threshold: stockRule ? Number(draft.threshold) : null,
          tolerance_pct: Number(draft.tolerance || "1"),
          enabled: draft.enabled,
          cooldown_minutes: Number(draft.cooldown || "60"),
          review_hour: stockRule ? null : Number(draft.reviewHour || "16"),
        }),
      });
      setDraft(EMPTY);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  async function toggle(item: ReminderRule) {
    setError("");
    try {
      await api("/mobile/reminders", {
        method: "POST",
        body: JSON.stringify({
          id: item.id,
          kind: item.kind,
          symbol: item.symbol,
          name: item.name,
          threshold: item.threshold,
          tolerance_pct: item.tolerance_pct,
          enabled: !item.enabled,
          cooldown_minutes: item.cooldown_minutes,
          review_hour: item.review_hour,
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
      await api("/mobile/reminders", { method: "DELETE", body: JSON.stringify({ id }) });
      if (draft.id === id) setDraft(EMPTY);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const invalid = stockRule
    ? !draft.symbol.trim() || !Number.isFinite(Number(draft.threshold)) || Number(draft.threshold) <= 0
    : !Number.isFinite(Number(draft.reviewHour)) || Number(draft.reviewHour) < 0 || Number(draft.reviewHour) > 23;

  return (
    <section className="mobile-section reminder-panel">
      <div className="mobile-section-title">
        <div>
          <h1>提醒中心</h1>
          <p>价格、止损、目标价与收盘复盘规则保存在本机；Android 后台最短每 15 分钟检查一次。</p>
        </div>
        <button type="button" className="mobile-delete" onClick={() => void load()} aria-label="刷新提醒" disabled={loading}>
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <article className="mobile-quote-card reminder-editor">
        <div className="reminder-editor-head">
          <div className="mobile-card-label">{draft.id ? `编辑提醒 #${draft.id}` : "新建提醒"}</div>
          <label className="reminder-switch">
            <input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft((value) => ({ ...value, enabled: event.target.checked }))} />
            <span>{draft.enabled ? "启用" : "停用"}</span>
          </label>
        </div>

        <div className="reminder-kind-grid">
          {(Object.keys(LABELS) as ReminderKind[]).map((kind) => (
            <button
              key={kind}
              type="button"
              className={draft.kind === kind ? "active" : ""}
              onClick={() => setDraft((value) => ({ ...value, kind }))}
            >
              {LABELS[kind]}
            </button>
          ))}
        </div>

        {stockRule ? (
          <>
            <div className="reminder-form-grid">
              <input value={draft.symbol} onChange={(event) => setDraft((value) => ({ ...value, symbol: event.target.value }))} placeholder="股票代码，如 600519" maxLength={32} />
              <input value={draft.name} onChange={(event) => setDraft((value) => ({ ...value, name: event.target.value }))} placeholder="名称（可选）" maxLength={300} />
              <input inputMode="decimal" value={draft.threshold} onChange={(event) => setDraft((value) => ({ ...value, threshold: event.target.value }))} placeholder="触发价" />
              <input inputMode="decimal" value={draft.tolerance} onChange={(event) => setDraft((value) => ({ ...value, tolerance: event.target.value }))} placeholder="附近范围 %" />
            </div>
            <p className="reminder-hint">“接近止损/目标”按触发价 ± 附近范围判断；价格上穿/下破按阈值进入状态触发。</p>
          </>
        ) : (
          <div className="reminder-form-grid reminder-review-grid">
            <input value={draft.name} onChange={(event) => setDraft((value) => ({ ...value, name: event.target.value }))} placeholder="提醒名称，如 收盘复盘" maxLength={300} />
            <label><span>北京时间</span><input inputMode="numeric" value={draft.reviewHour} onChange={(event) => setDraft((value) => ({ ...value, reviewHour: event.target.value }))} placeholder="16" /></label>
          </div>
        )}

        <div className="reminder-footer-fields">
          <label><span>冷却时间</span><input inputMode="numeric" value={draft.cooldown} onChange={(event) => setDraft((value) => ({ ...value, cooldown: event.target.value }))} /> 分钟</label>
          <div>
            {draft.id && <button type="button" onClick={() => setDraft(EMPTY)}>取消编辑</button>}
            <button type="button" className="primary" onClick={() => void save()} disabled={invalid || saving}>
              <Save size={14} /> {saving ? "保存中…" : "保存规则"}
            </button>
          </div>
        </div>
      </article>

      {error && <div className="mobile-error reminder-error">{error}</div>}

      <div className="reminder-summary">
        <span>全部 {items.length}</span>
        <span>已启用 {enabledCount}</span>
      </div>

      <div className="mobile-watch-list reminder-list">
        {items.length === 0 && !loading ? (
          <div className="mobile-empty">还没有提醒规则。</div>
        ) : items.map((item) => (
          <div className={`reminder-row ${item.enabled ? "" : "disabled"}`} key={item.id}>
            <button type="button" className="reminder-open" onClick={() => edit(item)}>
              <div className="reminder-icon">{item.enabled ? <Bell size={16} /> : <BellOff size={16} />}</div>
              <div>
                <strong>{item.name || item.symbol || LABELS[item.kind]}</strong>
                <span>
                  {LABELS[item.kind]}
                  {item.symbol ? ` · ${item.symbol}` : ""}
                  {item.threshold !== null ? ` · ${item.threshold}` : ""}
                  {item.kind === "daily_review" ? ` · 北京时间 ${item.review_hour ?? 16}:00` : ""}
                </span>
                <small>冷却 {item.cooldown_minutes} 分钟{item.quote_key ? ` · ${item.quote_key}` : ""}</small>
              </div>
            </button>
            <div className="reminder-actions">
              <button type="button" onClick={() => void toggle(item)}>{item.enabled ? "停用" : "启用"}</button>
              <button type="button" className="mobile-delete" onClick={() => void remove(item.id)} aria-label={`删除 ${item.name || item.symbol || "提醒"}`}><Trash2 size={15} /></button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
