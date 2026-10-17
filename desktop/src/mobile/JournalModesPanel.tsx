/**
 * 复盘 · 模式卡(移动版,对应桌面版交易日志的「模式卡」页):把自己的打法写下来、带版本,业绩按版本分段。
 * 改了打法或五段文字就开新版本、次日生效,旧版本不动;超长直接拒收不截断;样本不够不比较。
 * 卡片文字全由用户自己写,页面不预填任何规则。模式卡和成交一样只在本机、进备份、不进任何 AI,本页也不登记页面上下文。
 */
import { BookMarked, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useState } from "react";

import { fmtPercent, useDiscoveryData } from "./discoveryData";
import { fmtRate } from "./emotionData";
import "./emotion.css";
import "./monitor.css";
import "./journal-analysis.css";

type Maybe<T> = ({ available: true } & T) | { available: false; reason: string };
export interface ModeStats { trades: number; win_rate: number | null; median_pct: number | null; avg_pct: number | null; best: number | null; worst: number | null }
export interface ModeVersionPerf extends ModeStats {
  version: number; since: string; changes: string; playbook: string; setup: string; entry: string; exit: string; sizing: string; phase: string; saved_at: string; enough: boolean;
}
export interface ModeCardPerf {
  id: number; name: string; playbook: string; version_count: number; by_version: ModeVersionPerf[]; matched_trades: number;
  before_card: (ModeStats & { enough: boolean }) | null; compare_blocked: boolean;
  latest_vs_prev: { win_rate_delta: number; median_delta: number; from_version: number; to_version: number } | null;
}
export interface JournalModesData {
  available: true; playbooks: string[]; today: string; limits: { text: number; name: number; changes: number; versions: number };
  text_fields: { key: TextKey; label: string }[];
  modes: Maybe<{ cards: ModeCardPerf[]; min_per_version: number; truncated: boolean; note: string }>;
  excluded_non_stock: number; notes: string[];
}
type TextKey = "setup" | "entry" | "exit" | "sizing" | "phase";
interface Draft { card_id: number | null; name: string; playbook: string; changes: string; text: Record<TextKey, string> }

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const delta = (v: number) => (v > 0 ? "+" : "") + Math.round(v * 1000) / 10 + " 个点";
const blank = (playbook: string): Draft => ({ card_id: null, name: "", playbook, changes: "", text: { setup: "", entry: "", exit: "", sizing: "", phase: "" } });
const draftOf = (c: ModeCardPerf): Draft => {
  const v = c.by_version[c.by_version.length - 1];
  return { card_id: c.id, name: c.name, playbook: v?.playbook || c.playbook, changes: "", text: { setup: v?.setup ?? "", entry: v?.entry ?? "", exit: v?.exit ?? "", sizing: v?.sizing ?? "", phase: v?.phase ?? "" } };
};

async function post(body: unknown): Promise<{ view: JournalModesData; result: { new_version?: boolean; created?: boolean } }> {
  const response = await fetch("/api/mobile/journal/modes", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & Partial<{ view: JournalModesData; result: { new_version?: boolean; created?: boolean } }>) | null;
  if (!response.ok || !data?.view) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return { view: data.view, result: data.result ?? {} };
}

export function ModeEditor({ draft, data, onSaved, onCancel }: { draft: Draft; data: JournalModesData; onSaved: (view: JournalModesData, note: string) => void; onCancel: () => void }) {
  const [d, setD] = useState(draft);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const save = async () => {
    if (!d.name.trim()) { setMsg("模式卡要有名字"); return; }
    setBusy(true);
    setMsg("");
    try {
      const { view, result } = await post({ action: "save", ...(d.card_id != null ? { card_id: d.card_id, changes: d.changes } : {}), name: d.name, playbook: d.playbook, ...d.text });
      onSaved(view, result.created ? "已建卡,第一版从今天起生效。" : result.new_version ? "已开新版本,从明天起生效,旧版本没动。" : "已保存(没有改打法和五段文字,不开新版本)。");
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return <div className="jm-editor" role="group" aria-label={d.card_id == null ? "新建模式卡" : "编辑模式卡"}>
    <label><span>模式名</span><input value={d.name} maxLength={data.limits.name} placeholder="如「首板早封」" onChange={(e) => setD({ ...d, name: e.target.value })} aria-label="模式名" /></label>
    <label><span>打法</span><select value={d.playbook} onChange={(e) => setD({ ...d, playbook: e.target.value })} aria-label="打法">{data.playbooks.map((p) => <option key={p} value={p}>{p}</option>)}</select></label>
    {data.text_fields.map((f) => <label key={f.key} className="wide"><span>{f.label}<small>{d.text[f.key].length}/{data.limits.text}</small></span>
      <textarea rows={2} value={d.text[f.key]} onChange={(e) => setD({ ...d, text: { ...d.text, [f.key]: e.target.value } })} aria-label={f.label} /></label>)}
    {d.card_id != null && <label className="wide"><span>这次改了什么(记进新版本)</span><input value={d.changes} onChange={(e) => setD({ ...d, changes: e.target.value })} aria-label="这次改了什么" /></label>}
    <p className="watch-hint">{d.card_id != null ? "打法或上面五段有改动就开一个新版本,从明天起生效;今天和以前的交易仍按原版本统计。只改名字不开新版本。" : "第一版从今天起生效,不回溯到更早的交易。"}每段最多 {data.limits.text} 字,超长会被拒收、不会被截断。</p>
    <div className="jr-actions">
      <button type="button" className="discovery-action" disabled={busy} onClick={() => void save()}>保存</button>
      <button type="button" disabled={busy} onClick={onCancel}>取消</button>
    </div>
    {msg && <p className="intra-warn" role="alert">{msg}</p>}
  </div>;
}

export function ModeCard({ c, minPer, onEdit, onDelete }: { c: ModeCardPerf; minPer: number; onEdit: () => void; onDelete: () => void }) {
  const cmp = c.latest_vs_prev;
  const [asking, setAsking] = useState(false);
  return <section className="watch-card" aria-label={`模式卡 ${c.name}`}>
    <div className="watch-card-head"><h2><BookMarked size={15} />{c.name}</h2><span className="jm-tag">{c.playbook}</span></div>
    <p className="watch-hint">{c.version_count} 个版本 · 对上 {c.matched_trades} 笔</p>
    <div className="intra-table-wrap"><table className="intra-table" aria-label={`${c.name} 各版本`}>
      <thead><tr><th>版本</th><th>生效</th><th>打法</th><th>笔数</th><th>胜率</th><th>中位</th></tr></thead>
      <tbody>{c.by_version.map((v) => <tr key={v.version} className={v.enough ? "" : "jr-short"}>
        <td><b>v{v.version}</b>{v.changes && <small className="jm-change">{v.changes}</small>}</td><td>{v.since.slice(5)}</td><td>{v.playbook}</td>
        <td>{v.trades}{!v.enough && "*"}</td><td>{fmtRate(v.win_rate)}</td><td className={tone(v.median_pct)}>{fmtPercent(v.median_pct)}</td></tr>)}</tbody>
    </table></div>
    {c.by_version.some((v) => !v.enough) && <p className="intra-warn">* 不到 {minPer} 笔(或历史没载全),这个版本的读数单看没意义,也不拿来比较。</p>}
    {c.compare_blocked && <p className="intra-warn">版本样本不够或历史不完整,暂不比较:3 笔对 12 笔的「变差了」没有意义。</p>}
    {cmp && <p className="smp-line"><span>v{cmp.from_version} → v{cmp.to_version}</span><span>胜率 <b className={tone(cmp.win_rate_delta)}>{delta(cmp.win_rate_delta)}</b></span>
      <span>中位收益 <b className={tone(cmp.median_delta)}>{cmp.median_delta > 0 ? "+" : ""}{cmp.median_delta}%</b></span><span>你自己的历史统计,不是对下一笔的预测</span></p>}
    {c.before_card && c.before_card.trades > 0 && <p className="watch-hint">另有 {c.before_card.trades} 笔发生在建卡之前(胜率 {fmtRate(c.before_card.win_rate)}、中位 {fmtPercent(c.before_card.median_pct)}),不进任何版本:否则等于拿现在的规则评价更早的操作。</p>}
    <div className="jr-actions">
      <button type="button" onClick={onEdit}>编辑</button>
      {asking
        ? <><button type="button" className="discovery-action" onClick={onDelete} aria-label={`确认删除模式卡 ${c.name}`}><Trash2 size={13} />确认删除({c.version_count} 个版本一起删)</button><button type="button" onClick={() => setAsking(false)}>不删了</button></>
        : <button type="button" onClick={() => setAsking(true)} aria-label={`删除模式卡 ${c.name}`}><Trash2 size={13} />删除</button>}
    </div>
  </section>;
}

export function JournalModesView({ data, onChanged }: { data: JournalModesData; onChanged: (view: JournalModesData) => void }) {
  const [editing, setEditing] = useState<Draft | null>(null);
  const [msg, setMsg] = useState("");
  const remove = async (c: ModeCardPerf) => {
    setMsg("");
    try {
      const { view } = await post({ action: "delete", card_id: c.id });
      if (editing?.card_id === c.id) setEditing(null);
      onChanged(view);
      setMsg(`已删除「${c.name}」。`);
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    }
  };
  const m = data.modes;
  return <div className="watch-live">
    <section className="watch-card" aria-label="我这套打法">
      <div className="watch-card-head"><h2><BookMarked size={15} />我这套打法</h2>
        {!editing && <button type="button" className="jr-toggle" onClick={() => { setMsg(""); setEditing(blank(data.playbooks[0] ?? "")); }}><Plus size={13} />新建模式卡</button>}</div>
      <p className="watch-hint">打法改过之后,改动前后的统计不能混在一起算:混了那个平均数谁都用不上。写下来并带版本,业绩就能按版本分段,回答「上次那个改动到底是好是坏」。胜率不算持平,和交易统计同一口径。</p>
      {editing && <ModeEditor key={editing.card_id ?? "new"} draft={editing} data={data} onCancel={() => setEditing(null)} onSaved={(view, note) => { setEditing(null); setMsg(note); onChanged(view); }} />}
      {msg && <p className="watch-hint" role="status">{msg}</p>}
    </section>
    {!m.available ? <p className="watch-empty" role="status">{m.reason}。</p> : <>
      <p className="watch-hint">{m.note}</p>
      {m.truncated && <p className="intra-warn" role="status">只载入了最近一部分交易,历史样本不完整,暂停版本比较。</p>}
      {m.cards.map((c) => <ModeCard key={c.id} c={c} minPer={m.min_per_version} onEdit={() => { setMsg(""); setEditing(draftOf(c)); }} onDelete={() => void remove(c)} />)}
    </>}
    {data.excluded_non_stock > 0 && <p className="watch-hint">另有 {data.excluded_non_stock} 条记录不是 A 股股票(ETF、基金、债券等),交易日志不算它们,详见「问题记录」。</p>}
    <ul className="watch-notes" aria-label="口径说明">{data.notes.map((n) => <li key={n}>{n}</li>)}</ul>
  </div>;
}

export function JournalModesPanel() {
  const { data, loading, error, refresh } = useDiscoveryData<JournalModesData>("/mobile/journal/modes");
  const [override, setOverride] = useState<JournalModesData | null>(null);
  const shown = override ?? data;
  return <section className="monitor-panel">
    <div className="monitor-title-row">
      <div>
        <div className="monitor-kicker"><BookMarked size={15} /> 交易日志</div>
        <h1>模式卡</h1>
        <p>把自己的打法写下来、带版本,业绩按版本分段。改动从次日生效,旧版本不动;模式卡只在本机、进备份,不进 AI。</p>
      </div>
      <button className="monitor-refresh" type="button" onClick={() => { setOverride(null); refresh(); }} disabled={loading} aria-label="刷新模式卡"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
    </div>
    {error && <div className="monitor-error" role="alert">模式卡这次没读出来:{error}。<button type="button" className="discovery-action" onClick={refresh}>重试</button></div>}
    {!shown && !error && <div className="monitor-empty" role="status"><RefreshCw size={18} className="spin" /> 正在读取本机模式卡…</div>}
    {shown && <JournalModesView data={shown} onChanged={setOverride} />}
  </section>;
}
