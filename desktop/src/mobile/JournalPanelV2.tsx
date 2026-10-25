import { useEffect, useMemo, useState } from "react";
import { WebSelect, DateField } from "./controls";
import { priv, usePrivacyMode } from "./privacy";
import { TradeImportCard } from "./TradeImport";
import {
  ChevronDown,
  ChevronUp,
  CircleDollarSign,
  ClipboardCheck,
  Edit3,
  Eye,
  EyeOff,
  NotebookPen,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  X,
} from "lucide-react";

import "./journal-v2.css";

type Mode = "record" | "manual";
type Planned = "" | "true" | "false";

type FillDraft = {
  type: "buy" | "sell" | "opening" | "split";
  date: string;
  price: string;
  shares: string;
  fee: string;
};

type JournalFill = {
  id: number;
  type: "buy" | "sell" | "opening" | "split";
  date: string;
  price: number;
  shares: number;
  fee: number;
  fee_estimated: boolean;
};

type JournalRow = {
  id: number;
  kind: "episode" | "manual";
  symbol: string;
  name: string;
  date: string;
  playbook: string | null;
  as_planned: boolean | null;
  planned_stop: number | null;
  planned_target: number | null;
  note: string;
  pnl_pct: number | null;
  realized_pnl: number | null;
  closed: boolean;
  open_shares: number;
  amount: number | null;
  fees: number | null;
  fees_are_estimated: boolean | null;
  fills: JournalFill[];
};

type JournalView = {
  available: boolean;
  playbooks: string[];
  summary: {
    entries: number;
    scored: number;
    closed: number;
    wins: number;
    win_rate: number | null;
    money_scored: number;
    net_pnl: number | null;
    planned_known: number;
    planned_count: number;
    execution_rate: number | null;
  };
  rows: JournalRow[];
};

type JournalResult = { result: Record<string, unknown>; view: JournalView };

async function api<T>(path = "/mobile/journal", init?: RequestInit): Promise<T> {
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

function chinaToday(): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
}

function emptyFill(type: FillDraft["type"], date = chinaToday()): FillDraft {
  return { type, date, price: "", shares: "", fee: "" };
}

function numberOrNull(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function money(value: number | null): string {
  return value === null || !Number.isFinite(value)
    ? "—"
    : value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

/** 个人金额:隐私模式下遮成 ••••(盈亏百分比不遮) */
const pm = (value: number | null) => priv(money(value));

function pct(value: number | null, ratio = false): string {
  if (value === null || !Number.isFinite(value)) return "—";
  const actual = ratio ? value * 100 : value;
  return `${actual > 0 ? "+" : ""}${actual.toFixed(1)}%`;
}

function plannedValue(value: boolean | null): Planned {
  return value === null ? "" : value ? "true" : "false";
}

function fillPayload(fills: FillDraft[]): { payload?: Record<string, unknown>[]; error?: string } {
  if (!fills.length) return { error: "至少需要一笔成交。" };
  const payload: Record<string, unknown>[] = [];
  for (const fill of fills) {
    const price = numberOrNull(fill.price);
    const shares = fill.type === "split" ? 1 : numberOrNull(fill.shares);
    const fee = numberOrNull(fill.fee);
    if (!fill.date || price === null || price <= 0 || shares === null || shares <= 0) {
      return { error: "每笔成交都要填写日期、大于 0 的成交价和股数。" };
    }
    if (fill.fee.trim() && (fee === null || fee < 0)) return { error: "费用要是大于等于 0 的数字。" };
    payload.push({ type: fill.type, date: fill.date, price, shares, ...(fee !== null ? { fee } : {}) });
  }
  return { payload };
}

function FillEditor({ fills, onChange }: { fills: FillDraft[]; onChange: (fills: FillDraft[]) => void }) {
  const pwType = usePrivacyMode().on ? "password" : "text";
  const update = (index: number, patch: Partial<FillDraft>) => onChange(fills.map((fill, i) => i === index ? { ...fill, ...patch } : fill));
  return (
    <div className="journal-v2-fill-editor">
      {fills.map((fill, index) => (
        <div className="journal-v2-fill-line" key={`${index}-${fill.type}`}>
          <WebSelect aria-label="成交类型" value={fill.type} onChange={e=>{const type=e.target.value as FillDraft["type"];update(index,{type,...(type==="split"?{price:"",shares:"1",fee:"0"}:type==="opening"?{fee:"0"}:{})});}}><option value="buy">买入</option><option value="sell">卖出</option><option value="opening">期初</option><option value="split">送转</option></WebSelect>
          <DateField value={fill.date} onChange={(event) => update(index, { date: event.target.value })} />
          <input type={pwType} autoComplete="off" inputMode="decimal" placeholder={fill.type==="split"?"新股数/原股数":fill.type==="opening"?"每股期初成本":"成交价"} value={fill.price} onChange={(event) => update(index, { price: event.target.value })} />
          <input type={pwType} autoComplete="off" inputMode="numeric" placeholder="股数" disabled={fill.type==="split"} value={fill.type==="split"?"1":fill.shares} onChange={(event) => update(index, { shares: event.target.value })} />
          <input type={pwType} autoComplete="off" inputMode="decimal" disabled={fill.type==="split"||fill.type==="opening"} placeholder="费用(可空)" value={fill.fee} onChange={(event) => update(index, { fee: event.target.value })} />
          <button type="button" className="remove" onClick={() => onChange(fills.filter((_, i) => i !== index))}><X size={14} /></button>
        </div>
      ))}
      <div className="journal-v2-fill-actions">
        <button type="button" onClick={() => onChange([...fills, emptyFill("buy")])}>+ 买入</button>
        <button type="button" onClick={() => onChange([...fills, emptyFill("sell")])}>+ 卖出</button>
        <button type="button" onClick={() => onChange([...fills, emptyFill("opening")])}>+ 期初持仓</button>
        <button type="button" onClick={() => onChange([...fills, emptyFill("split")])}>+ 送转拆股</button>
        <small>送转比例填新股数 / 原股数，每十股转十股填 2；合股填 0.5。同日送转在买卖之前结转。</small>
      </div>
    </div>
  );
}

export function JournalPanelV2({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [data, setData] = useState<JournalView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const privacy = usePrivacyMode();
  const [message, setMessage] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [mode, setMode] = useState<Mode>("record");

  const [symbol, setSymbol] = useState("");
  const [name, setName] = useState("");
  const [date, setDate] = useState(chinaToday());
  const [playbook, setPlaybook] = useState("");
  const [planned, setPlanned] = useState<Planned>("");
  const [stop, setStop] = useState("");
  const [target, setTarget] = useState("");
  const [note, setNote] = useState("");
  const [manualPnl, setManualPnl] = useState("");
  const [fills, setFills] = useState<FillDraft[]>([emptyFill("buy")]);

  const [editing, setEditing] = useState<JournalRow | null>(null);
  const [editName, setEditName] = useState("");
  const [editDate, setEditDate] = useState("");
  const [editPlaybook, setEditPlaybook] = useState("");
  const [editPlanned, setEditPlanned] = useState<Planned>("");
  const [editStop, setEditStop] = useState("");
  const [editTarget, setEditTarget] = useState("");
  const [editNote, setEditNote] = useState("");
  const [editPnl, setEditPnl] = useState("");

  const [appendTarget, setAppendTarget] = useState<JournalRow | null>(null);
  const [appendFills, setAppendFills] = useState<FillDraft[]>([emptyFill("sell")]);

  const playbooks = data?.playbooks ?? ["打板", "低吸", "接力", "半路", "套利", "其它"];
  const openCount = useMemo(() => data?.rows.filter((row) => row.kind === "episode" && !row.closed).length ?? 0, [data]);

  async function load() {
    setLoading(true);
    setError("");
    try { setData(await api<JournalView>()); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  }

  useEffect(() => { void load(); }, []);

  function resetCreate() {
    const today = chinaToday();
    setSymbol(""); setName(""); setDate(today); setPlaybook(""); setPlanned(""); setStop(""); setTarget(""); setNote(""); setManualPnl(""); setFills([emptyFill("buy", today)]);
  }

  async function createEntry() {
    const code = symbol.trim();
    setError(""); setMessage("");
    if (!/^\d{6}$/.test(code)) { setError("代码要是 6 位数字，交易日志只记录 A 股。"); return; }
    const common = {
      symbol: code,
      ...(name.trim() ? { name: name.trim() } : {}),
      ...(playbook ? { playbook } : {}),
      ...(planned ? { as_planned: planned === "true" } : {}),
      ...(note.trim() ? { note: note.trim() } : {}),
    };
    let body: Record<string, unknown>;
    if (mode === "manual") {
      const pnl = numberOrNull(manualPnl);
      if (manualPnl.trim() && pnl === null) { setError("盈亏% 要是数字，例如 3.2 或 -5。"); return; }
      body = { action: "manual", ...common, date, ...(pnl !== null ? { pnl_pct: pnl } : {}) };
    } else {
      const parsed = fillPayload(fills);
      if (parsed.error) { setError(parsed.error); return; }
      const stopValue = numberOrNull(stop);
      const targetValue = numberOrNull(target);
      if (stop.trim() && (stopValue === null || stopValue <= 0)) { setError("计划止损价要大于 0。"); return; }
      if (target.trim() && (targetValue === null || targetValue <= 0)) { setError("计划目标价要大于 0。"); return; }
      body = { action: "record", ...common, fills: parsed.payload, ...(stopValue !== null ? { planned_stop: stopValue } : {}), ...(targetValue !== null ? { planned_target: targetValue } : {}) };
    }
    setLoading(true);
    try {
      const result = await api<JournalResult>("/mobile/journal", { method: "POST", body: JSON.stringify(body) });
      setData(result.view); resetCreate(); setShowCreate(false); setMessage("交易日志已保存到手机本地。");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  }

  function startEdit(row: JournalRow) {
    setEditing(row); setAppendTarget(null);
    setEditName(row.name); setEditDate(row.date); setEditPlaybook(row.playbook ?? ""); setEditPlanned(plannedValue(row.as_planned));
    setEditStop(row.planned_stop?.toString() ?? ""); setEditTarget(row.planned_target?.toString() ?? ""); setEditNote(row.note); setEditPnl(row.pnl_pct?.toString() ?? "");
  }

  async function saveEdit() {
    if (!editing) return;
    const stopValue = numberOrNull(editStop);
    const targetValue = numberOrNull(editTarget);
    const pnlValue = numberOrNull(editPnl);
    if (editStop.trim() && (stopValue === null || stopValue <= 0)) { setError("计划止损价要大于 0。"); return; }
    if (editTarget.trim() && (targetValue === null || targetValue <= 0)) { setError("计划目标价要大于 0。"); return; }
    if (editing.kind === "manual" && editPnl.trim() && pnlValue === null) { setError("盈亏% 要是数字。"); return; }
    const body = {
      id: editing.id,
      name: editName.trim(),
      playbook: editPlaybook || null,
      as_planned: editPlanned ? editPlanned === "true" : null,
      planned_stop: stopValue,
      planned_target: targetValue,
      note: editNote.trim(),
      ...(editing.kind === "manual" ? { date: editDate, pnl_pct: pnlValue } : {}),
    };
    setLoading(true); setError("");
    try {
      const result = await api<JournalResult>("/mobile/journal/update", { method: "POST", body: JSON.stringify(body) });
      setData(result.view); setEditing(null); setMessage("记录已更新。");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  }

  function startAppend(row: JournalRow) {
    setAppendTarget(row); setEditing(null); setAppendFills([emptyFill(row.open_shares > 0 ? "sell" : "buy")]);
  }

  async function append() {
    if (!appendTarget) return;
    const parsed = fillPayload(appendFills);
    if (parsed.error) { setError(parsed.error); return; }
    setLoading(true); setError("");
    try {
      const result = await api<JournalResult>("/mobile/journal/append", {
        method: "POST",
        body: JSON.stringify({ id: appendTarget.id, fills: parsed.payload }),
      });
      setData(result.view); setAppendTarget(null); setMessage("追加成交已并入原记录。");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  }

  async function remove(id: number) {
    setLoading(true); setError("");
    try {
      const result = await api<JournalResult>("/mobile/journal", { method: "POST", body: JSON.stringify({ action: "delete", id }) });
      setData(result.view); setMessage("记录已删除。");
      if (editing?.id === id) setEditing(null);
      if (appendTarget?.id === id) setAppendTarget(null);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  }

  return (
    <section className="journal-v2">
      <div className="journal-v2-title">
        <div><span><NotebookPen size={15} /> 本地交易日志</span><h1>记录 · 编辑 · 追加成交</h1><p>交易数据只保存在手机 SQLite；默认不发送给 AI。</p></div>
        <span className="journal-v2-title-tools">
          <button onClick={privacy.toggle} aria-pressed={privacy.on} aria-label={privacy.on ? "关闭隐私模式" : "开启隐私模式(遮住金额和股数)"}>{privacy.on ? <EyeOff size={17} /> : <Eye size={17} />}</button>
          <button onClick={() => void load()} disabled={loading} aria-label="刷新交易日志"><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
        </span>
      </div>

      {error && <div className="journal-v2-error">{error}</div>}
      {message && <div className="journal-v2-message">{message}</div>}

      <div className="journal-v2-summary">
        <div><span>记录</span><strong>{data?.summary.entries ?? 0}</strong></div>
        <div><span>已结算</span><strong>{data?.summary.closed ?? 0}</strong></div>
        <div><span>胜率</span><strong>{pct(data?.summary.win_rate ?? null, true)}</strong></div>
        <div><span>按计划</span><strong>{pct(data?.summary.execution_rate ?? null, true)}</strong></div>
      </div>

      <div className="journal-v2-money">
        <CircleDollarSign size={15} /><span>净盈亏</span>
        <strong className={(data?.summary.net_pnl ?? 0) >= 0 ? "up" : "down"}>{pm(data?.summary.net_pnl ?? null)} 元</strong>
        <small>未平仓 {openCount} 笔</small>
      </div>

      <button className="journal-v2-create-toggle" onClick={() => setShowCreate((value) => !value)}>
        <Plus size={16} /> 记一笔 {showCreate ? <ChevronUp size={15} /> : <ChevronDown size={15} />}
      </button>

      {showCreate && (
        <div className="journal-v2-editor">
          <div className="journal-v2-mode"><button className={mode === "record" ? "active" : ""} onClick={() => setMode("record")}>成交明细</button><button className={mode === "manual" ? "active" : ""} onClick={() => setMode("manual")}>只记盈亏%</button></div>
          <div className="journal-v2-fields">
            {mode === "manual" && <DateField value={date} onChange={(event) => setDate(event.target.value)} />}
            <input inputMode="numeric" placeholder="股票代码" value={symbol} onChange={(event) => setSymbol(event.target.value)} />
            <input placeholder="简称(可选)" value={name} onChange={(event) => setName(event.target.value)} />
            <WebSelect value={playbook} onChange={(event) => setPlaybook(event.target.value)}><option value="">打法(不填)</option>{playbooks.map((item) => <option key={item}>{item}</option>)}</WebSelect>
            <WebSelect value={planned} onChange={(event) => setPlanned(event.target.value as Planned)}><option value="">是否按计划</option><option value="true">按计划</option><option value="false">计划外</option></WebSelect>
            {mode === "manual" && <input inputMode="decimal" placeholder="盈亏% 如 -3.2" value={manualPnl} onChange={(event) => setManualPnl(event.target.value)} />}
            {mode === "record" && <><input type={privacy.on ? "password" : "text"} autoComplete="off" inputMode="decimal" placeholder="计划止损价" value={stop} onChange={(event) => setStop(event.target.value)} /><input type={privacy.on ? "password" : "text"} autoComplete="off" inputMode="decimal" placeholder="计划目标价" value={target} onChange={(event) => setTarget(event.target.value)} /></>}
          </div>
          <textarea placeholder="当时为什么做这笔，复盘时最有价值" value={note} onChange={(event) => setNote(event.target.value)} />
          {mode === "record" && <FillEditor fills={fills} onChange={setFills} />}
          <button className="journal-v2-primary" onClick={() => void createEntry()} disabled={loading}><Save size={15} />保存</button>
        </div>
      )}

      <TradeImportCard onImported={() => void load()} />

      <div className="journal-v2-list">
        {data?.rows.map((row) => (
          <article className="journal-v2-row" key={row.id}>
            <div className="journal-v2-row-head">
              <button className="symbol" onClick={() => onOpenSymbol(row.symbol)}><strong>{row.name || row.symbol}</strong><span>{row.symbol} · {row.date}</span></button>
              <div className="result"><strong className={(row.pnl_pct ?? 0) > 0 ? "up" : (row.pnl_pct ?? 0) < 0 ? "down" : ""}>{pct(row.pnl_pct)}</strong><span>{row.kind === "manual" ? "盈亏补记" : row.closed ? `${pm(row.realized_pnl)} 元` : `持有 ${priv(row.open_shares.toLocaleString("zh-CN"))} 股`}</span></div>
            </div>
            <div className="journal-v2-tags">
              {row.playbook && <span>{row.playbook}</span>}
              {row.as_planned === true && <span className="planned"><ClipboardCheck size={10} />按计划</span>}
              {row.as_planned === false && <span className="unplanned">计划外</span>}
              {row.planned_stop !== null && <span>止损 {priv(String(row.planned_stop))}</span>}
              {row.planned_target !== null && <span>目标 {priv(String(row.planned_target))}</span>}
            </div>
            {row.fills.length > 0 && <div className="journal-v2-fills">{row.fills.map((fill) => <div key={fill.id}><b className={fill.type === "buy" ? "buy" : "sell"}>{({buy:"买",sell:"卖",opening:"期初",split:"送转"})[fill.type]}</b><span>{fill.date}</span><strong>{priv(`${fill.type==="split"?"股数倍率 ":""}${fill.price.toLocaleString("zh-CN", { maximumFractionDigits: 3 })} × ${fill.shares.toLocaleString("zh-CN")}`)}</strong><small>费 {pm(fill.fee)}{fill.fee_estimated ? " 估" : ""}</small></div>)}</div>}
            {row.note && <p>{row.note}</p>}
            <div className="journal-v2-actions">
              <button onClick={() => startEdit(row)}><Edit3 size={14} />编辑</button>
              {row.kind === "episode" && !row.closed && <button onClick={() => startAppend(row)}><Plus size={14} />追加成交</button>}
              <button className="danger" onClick={() => void remove(row.id)} disabled={loading}><Trash2 size={14} />删除</button>
            </div>

            {editing?.id === row.id && (
              <div className="journal-v2-inline-editor">
                <div className="journal-v2-fields">
                  {row.kind === "manual" && <DateField value={editDate} onChange={(event) => setEditDate(event.target.value)} />}
                  <input placeholder="简称" value={editName} onChange={(event) => setEditName(event.target.value)} />
                  <WebSelect value={editPlaybook} onChange={(event) => setEditPlaybook(event.target.value)}><option value="">打法(不填)</option>{playbooks.map((item) => <option key={item}>{item}</option>)}</WebSelect>
                  <WebSelect value={editPlanned} onChange={(event) => setEditPlanned(event.target.value as Planned)}><option value="">是否按计划</option><option value="true">按计划</option><option value="false">计划外</option></WebSelect>
                  {row.kind === "manual" && <input inputMode="decimal" placeholder="盈亏%" value={editPnl} onChange={(event) => setEditPnl(event.target.value)} />}
                  {row.kind === "episode" && <><input type={privacy.on ? "password" : "text"} autoComplete="off" inputMode="decimal" placeholder="计划止损价" value={editStop} onChange={(event) => setEditStop(event.target.value)} /><input type={privacy.on ? "password" : "text"} autoComplete="off" inputMode="decimal" placeholder="计划目标价" value={editTarget} onChange={(event) => setEditTarget(event.target.value)} /></>}
                </div>
                <textarea placeholder="复盘备注" value={editNote} onChange={(event) => setEditNote(event.target.value)} />
                <div className="journal-v2-inline-actions"><button className="journal-v2-primary" onClick={() => void saveEdit()}><Save size={14} />保存修改</button><button onClick={() => setEditing(null)}><X size={14} />取消</button></div>
              </div>
            )}

            {appendTarget?.id === row.id && (
              <div className="journal-v2-inline-editor">
                <strong>追加到 {row.name || row.symbol} · 当前剩余 {row.open_shares.toLocaleString("zh-CN")} 股</strong>
                <FillEditor fills={appendFills} onChange={setAppendFills} />
                <div className="journal-v2-inline-actions"><button className="journal-v2-primary" onClick={() => void append()}><Plus size={14} />并入原记录</button><button onClick={() => setAppendTarget(null)}><X size={14} />取消</button></div>
              </div>
            )}
          </article>
        ))}
      </div>

      {data && data.rows.length === 0 && <div className="journal-v2-empty"><NotebookPen size={28} /><strong>还没有交易记录</strong><span>先记第一笔，后续可以直接编辑和追加成交。</span></div>}
    </section>
  );
}
