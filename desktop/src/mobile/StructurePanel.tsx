/**
 * 复盘 · 结构变化(移动版,对应桌面版「结构变化」页):图上这个断点,是市场变了、数据源变了,还是规则变了。
 * 三类分开看:① 数据源 —— 四张池子的上游字段清单变没变;② 市场 —— 最近 10 个交易日 vs 之前 20 个交易日的涨停结构(逐项比中位数);
 * ③ 制度 —— 只收人工登记的,系统不从数据反推「这天大概改了规则」。只报「这里不连续」,不报「所以该怎么改打法」。
 */
import { Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";

import { fmtNumber, fmtPercent } from "./discoveryData";
import { fmtRate, useCachedResource, useEmotionDate, clearEmotionCache } from "./emotionData";
import { ReviewDateBar } from "./HistoryPanels";
import "./emotion.css";
import "./monitor.css";

export interface MetricShift { metric: string; label: string; kind: "count" | "share"; recent: number; prior: number; delta: number; rel_change: number | null; shifted: boolean; recent_days: number; prior_days: number }
export interface PoolDrift {
  pool: string; label: string; days: number; changed: boolean; added: string[] | null; removed: string[] | null; detectable: boolean; note: string;
  versions: { since: string; until: string; count: number; fields: string }[];
}
export interface RegimeEvent { id: number; date: string; title: string; note: string; upcoming: boolean }
interface WindowInfo { days: number; used: number; from: string | null; to: string | null }
export interface StructureData {
  date: string; available: boolean; recent_window: WindowInfo; prior_window: WindowInfo;
  structure: { available: boolean; reason: string | null; metrics: MetricShift[]; shifted: MetricShift[]; skipped: string[] };
  thresholds: { share_abs: number; count_rel: number; min_days: number };
  field_drift: PoolDrift[]; field_changed: string[]; regime_events: RegimeEvent[]; regime_note: string;
  missing_days: string[]; unsettled_days: string[]; summary: string; note: string; gaps: string[]; source: string;
}

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const windowText = (w: WindowInfo) => (w.from && w.to ? `${w.from} ~ ${w.to}` : "—");
const valueText = (m: MetricShift, v: number) => (m.kind === "share" ? fmtRate(v) : fmtNumber(v, 1));
const deltaText = (m: MetricShift) => (m.kind === "share" ? `${m.delta > 0 ? "+" : ""}${fmtNumber(m.delta * 100, 1)} 个点` : m.rel_change == null ? "—" : fmtPercent(m.rel_change * 100));

export function EventEditor({ event, onDone, onCancel }: { event: RegimeEvent | null; onDone: () => void; onCancel: () => void }) {
  const [date, setDate] = useState(event?.date ?? "");
  const [title, setTitle] = useState(event?.title ?? "");
  const [note, setNote] = useState(event?.note ?? "");
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (body: unknown) => {
    setBusy(true);
    setMsg("");
    try {
      const response = await fetch("/api/mobile/review/structure", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const data = await response.json().catch(() => null) as { message?: string; error?: string } | null;
      if (!response.ok) setMsg(data?.message ?? data?.error ?? `HTTP ${response.status}`);
      else { clearEmotionCache(); onDone(); }
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return <div className="jr-form" aria-label="制度事件编辑">
    <label><span>施行日期(可以是未来)</span><input type="date" value={date} onChange={(e) => setDate(e.target.value)} aria-label="施行日期" /></label>
    <label><span>标题(最多 60 字)</span><input value={title} onChange={(e) => setTitle(e.target.value)} aria-label="事件标题" /></label>
    <label><span>说明(选填,最多 300 字)</span><input value={note} onChange={(e) => setNote(e.target.value)} aria-label="事件说明" /></label>
    <div className="jr-actions">
      <button type="button" className="discovery-action" disabled={busy} onClick={() => void submit({ action: "save", id: event?.id, date, title, note })}>保存</button>
      {event && <button type="button" disabled={busy} onClick={() => void submit({ action: "delete", id: event.id })}><Trash2 size={13} /> 删除</button>}
      <button type="button" disabled={busy} onClick={onCancel}>取消</button>
    </div>
    {msg && <p className="intra-warn" role="alert">{msg}</p>}
  </div>;
}

export function StructureView({ data: d, onChanged }: { data: StructureData; onChanged: () => void }) {
  const [editing, setEditing] = useState<RegimeEvent | "new" | null>(null);
  const s = d.structure;
  return <div className="watch-live">
    <section className="watch-card" aria-label="结构变化汇总"><div className="watch-card-head"><h2>结论</h2></div>
      <p className="smp-line">{d.summary}</p><p className="watch-hint">{d.note}</p></section>

    <section className="watch-card" aria-label="市场结构比较"><div className="watch-card-head"><h2>② 市场:涨停结构</h2></div>
      <p className="watch-hint">最近 {d.recent_window.days} 个交易日({windowText(d.recent_window)},取到 {d.recent_window.used} 天)vs 之前 {d.prior_window.days} 个交易日({windowText(d.prior_window)},取到 {d.prior_window.used} 天);逐项比中位数。移位门槛:计数类相对变化 ≥ {fmtRate(d.thresholds.count_rel)},占比类绝对变化 ≥ {fmtRate(d.thresholds.share_abs)};每个窗口至少 {d.thresholds.min_days} 天。</p>
      {s.available ? <div className="intra-table-wrap"><table className="intra-table" aria-label="结构比较">
        <thead><tr><th>读数</th><th>最近</th><th>之前</th><th>变化</th><th>移位?</th></tr></thead>
        <tbody>{s.metrics.map((m) => <tr key={m.metric}><td><b>{m.label}</b></td><td>{valueText(m, m.recent)}</td><td>{valueText(m, m.prior)}</td><td className={tone(m.delta)}>{deltaText(m)}</td><td>{m.shifted ? "是" : "否"}</td></tr>)}</tbody></table></div>
        : <p className="watch-empty" role="status">暂不可用:{s.reason}。东财池子只留约一个月,本机缓存会随使用慢慢积累,过几天再看。</p>}
      {s.skipped.length > 0 && <p className="watch-hint">天数不够没比的项:{s.skipped.join("、")}</p>}
    </section>

    <section className="watch-card" aria-label="上游字段清单"><div className="watch-card-head"><h2>① 数据源:上游字段清单</h2></div>
      <p className="watch-hint">字段清单只说明上游这天回了哪些键,不说明值的口径有没有变。变了 = 数据源改了,跨那一天的统计要小心,断点可能跟市场无关。</p>
      <ul className="jr-rules" aria-label="各池子字段变化">{d.field_drift.map((p) => <li key={p.pool} className={p.changed ? "no" : "yes"}>
        <b>{p.label}</b><span>{p.days} 天</span><em>{p.note}{p.changed && p.added && p.removed ? `(新增:${p.added.join("、") || "无"};消失:${p.removed.join("、") || "无"})` : ""}</em></li>)}</ul>
    </section>

    <section className="watch-card" aria-label="制度事件"><div className="watch-card-head"><h2>③ 制度:人工登记</h2>
      <button type="button" className="jr-toggle" onClick={() => setEditing(editing === "new" ? null : "new")}><Plus size={13} /> 登记</button></div>
      <p className="watch-hint">{d.regime_note}</p>
      {editing && <EventEditor key={editing === "new" ? "new" : editing.id} event={editing === "new" ? null : editing} onCancel={() => setEditing(null)} onDone={() => { setEditing(null); onChanged(); }} />}
      {d.regime_events.length === 0 ? <p className="watch-empty">还没登记制度事件(比如「2026-07-06 主板 ST 涨跌幅放宽到 10%」),要人工登记,系统不自动推断。</p>
        : <ul className="jr-violations" aria-label="已登记的制度事件">{d.regime_events.map((e) => <li key={e.id}><time>{e.date}</time>
          <b>{e.title}{e.upcoming && <em className="mp-stale">还没施行</em>}</b>{e.note && <span>{e.note}</span>}
          <small><button type="button" className="jr-toggle" aria-label={`修改 ${e.title}`} onClick={() => setEditing(e)}><Pencil size={12} /> 修改</button></small></li>)}</ul>}
    </section>

    {d.gaps.length > 0 && <ul className="watch-notes" aria-label="缺口说明">{d.gaps.map((g) => <li key={g}>{g}</li>)}</ul>}
    {d.unsettled_days.length > 0 && <p className="emo-warn" role="status">还没收盘、没算进去的:{d.unsettled_days.join("、")}</p>}
    <p className="monitor-note">{d.source}。</p>
  </div>;
}

export function StructurePanel() {
  const [date] = useEmotionDate();
  const result = useCachedResource<StructureData>(`/mobile/review/structure${date ? "?date=" + date : ""}`);
  return <section className="mobile-section emo-page">
    <ReviewDateBar title="结构变化" detail="断点是市场变了、数据源变了,还是规则变了" loading={result.loading} error={result.error} onRefresh={result.refresh} shownDate={result.data?.date} />
    {result.data && <StructureView data={result.data} onChanged={result.refresh} />}
  </section>;
}
