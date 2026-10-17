/**
 * 我的持仓 · 已实现盈亏与现金分红(移动版)。
 * 已实现只计清仓的交易(净额);现金分红是手填的收入,单独列、不并进已实现、不改持仓成本。个人持仓、盈亏、分红只在本机,不进任何 AI。
 */
import { Pencil, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { askConfirm, DateField } from "./controls";
import { fmtNumber } from "./discoveryData";
import { priv, usePrivacyMode } from "./privacy";
import "./portfolio-income.css";

export interface IncomeHolding { symbol: string; name: string; open_shares: number; avg_cost: number; open_cost: number; realized_pnl: number; closed_trades: number; dividends: number }
export interface IncomeClosed { symbol: string; name: string; realized_pnl: number; closed_trades: number; dividends: number; last_date: string }
export interface DividendItem { id: number; symbol: string; name: string; date: string; amount: number; note: string }
export interface IncomeData {
  totals: { realized_pnl: number; dividends: number; combined: number; closed_trades: number; wins: number; losses: number; open_symbols: number; closed_symbols: number };
  holdings: IncomeHolding[]; closed: IncomeClosed[]; dividends: DividendItem[]; method: string; note: string; fees_default: boolean;
}

const tone = (v: number) => (v > 0 ? "up" : v < 0 ? "down" : "");
const signed = (v: number) => priv((v > 0 ? "+" : "") + fmtNumber(v));
const label = (x: { name: string; symbol: string }) => x.name || x.symbol;

export function DividendEditor({ item, today, busy, message, onSave, onDelete, onCancel }: {
  item: DividendItem | null; today: string; busy: boolean; message: string;
  onSave: (v: { symbol: string; date: string; amount: string; note: string }) => void; onDelete?: () => void; onCancel: () => void;
}) {
  const [symbol, setSymbol] = useState(item?.symbol ?? "");
  const [date, setDate] = useState(item?.date ?? today);
  const [amount, setAmount] = useState(item ? String(item.amount) : "");
  const [note, setNote] = useState(item?.note ?? "");
  const privacy = usePrivacyMode();
  return <div className="pi-editor" aria-label="分红编辑">
    <label><span>代码</span><input value={symbol} onChange={(e) => setSymbol(e.target.value)} aria-label="分红代码" inputMode="numeric" placeholder="如 600519" /></label>
    <label><span>到账日期(不能是未来)</span><DateField aria-label="分红日期" max={today} value={date} onChange={(e) => setDate(e.target.value)} /></label>
    <label><span>到手现金(元)</span><input type={privacy.on ? "password" : "text"} value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="分红金额" inputMode="decimal" autoComplete="off" /></label>
    <label><span>备注(选填,最多 100 字)</span><input value={note} onChange={(e) => setNote(e.target.value)} aria-label="分红备注" /></label>
    <div className="pi-actions">
      <button type="button" className="primary" disabled={busy} onClick={() => onSave({ symbol, date, amount, note })}>保存</button>
      <button type="button" disabled={busy} onClick={onCancel}>取消</button>
      {onDelete && <button type="button" className="danger" disabled={busy} onClick={onDelete}><Trash2 size={13} /> 删除</button>}
    </div>
    {message && <p className="intra-warn" role="alert">{message}</p>}
  </div>;
}

export function IncomeView({ data, onEdit, onAdd }: { data: IncomeData; onEdit: (item: DividendItem) => void; onAdd: () => void }) {
  const t = data.totals;
  return <div className="pi-body">
    <div className="pi-stats" aria-label="已实现与分红合计">
      <div><span>已实现盈亏</span><strong className={tone(t.realized_pnl)}>{signed(t.realized_pnl)}</strong><small>{t.closed_trades} 笔清仓 · 赚 {t.wins} 亏 {t.losses}</small></div>
      <div><span>现金分红</span><strong className={tone(t.dividends)}>{signed(t.dividends)}</strong><small>{data.dividends.length} 笔</small></div>
      <div><span>合计</span><strong className={tone(t.combined)}>{signed(t.combined)}</strong><small>元</small></div>
    </div>
    {data.holdings.length > 0 && <>
      <h3>在手</h3>
      <ul className="pi-list" aria-label="在手持仓的已实现与分红">{data.holdings.map((h) => <li key={h.symbol}>
        <b>{label(h)}</b><small>{h.symbol} · {priv(fmtNumber(h.open_shares, 0))} 股 · 成本 {priv(fmtNumber(h.avg_cost))}</small>
        <span>已实现 <i className={tone(h.realized_pnl)}>{signed(h.realized_pnl)}</i>({h.closed_trades} 笔)</span><span>分红 <i className={tone(h.dividends)}>{signed(h.dividends)}</i></span></li>)}</ul>
    </>}
    {data.closed.length > 0 && <>
      <h3>已清仓 / 只有分红</h3>
      <ul className="pi-list" aria-label="已清仓的已实现与分红">{data.closed.map((c) => <li key={c.symbol}>
        <b>{label(c)}</b><small>{c.symbol} · 最近 {c.last_date || "—"}</small>
        <span>已实现 <i className={tone(c.realized_pnl)}>{signed(c.realized_pnl)}</i>({c.closed_trades} 笔)</span><span>分红 <i className={tone(c.dividends)}>{signed(c.dividends)}</i></span></li>)}</ul>
    </>}
    <div className="pi-head"><h3>现金分红明细</h3><button type="button" className="pi-add" onClick={onAdd}><Plus size={13} /> 记一笔</button></div>
    {data.dividends.length === 0 ? <p className="pi-empty">还没记过分红。分红不会自动抓取,到账后自己记一笔。</p>
      : <ul className="pi-list" aria-label="分红明细">{data.dividends.map((d) => <li key={d.id}>
        <b>{d.date}</b><small>{label(d)}{d.name ? " " + d.symbol : ""}{d.note ? " · " + d.note : ""}</small>
        <span><i className={tone(d.amount)}>{signed(d.amount)}</i> 元</span>
        <button type="button" aria-label={`修改 ${d.date} ${label(d)} 分红`} onClick={() => onEdit(d)}><Pencil size={13} /></button></li>)}</ul>}
    <p className="pi-note">{data.note}{data.fees_default ? " 费率用的是默认值(不是你的真实费率),可在「风险与纪律」里改。" : ""}成本法:{data.method}。</p>
  </div>;
}

export function PortfolioIncome() {
  const [data, setData] = useState<IncomeData | null>(null);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState<DividendItem | "new" | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const today = new Date(Date.now() + 8 * 3600_000).toISOString().slice(0, 10);

  const load = useCallback(async () => {
    try {
      const response = await fetch("/api/mobile/portfolio/summary", { headers: { Accept: "application/json" } });
      const body = await response.json();
      if (!response.ok) throw new Error(body?.message ?? `HTTP ${response.status}`);
      setData(body as IncomeData);
      setError("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);
  useEffect(() => { void load(); }, [load]);

  async function act(body: unknown): Promise<boolean> {
    setBusy(true);
    setMessage("");
    try {
      const response = await fetch("/api/mobile/portfolio/dividend", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const result = await response.json().catch(() => null) as ({ message?: string; error?: string } & Partial<IncomeData>) | null;
      if (!response.ok) { setMessage(result?.message ?? result?.error ?? `HTTP ${response.status}`); return false; }
      setData(result as IncomeData);
      return true;
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(false);
    }
  }

  return <section className="portfolio-income" aria-label="已实现盈亏与现金分红">
    <div className="pi-title"><h2>已实现与分红</h2><small>只计清仓 · 分红手填</small></div>
    {error && <p className="portfolio-error">加载失败:{error}</p>}
    {!data && !error && <p className="pi-empty">正在汇总…</p>}
    {editing && <DividendEditor key={editing === "new" ? "new" : editing.id} item={editing === "new" ? null : editing} today={today} busy={busy} message={message}
      onCancel={() => setEditing(null)}
      onSave={async (v) => {
        const amount = Number(v.amount.trim());
        if (!v.amount.trim() || !Number.isFinite(amount)) { setMessage("分红金额必须是数字"); return; }
        if (await act({ action: "save", id: editing === "new" ? undefined : editing.id, symbol: v.symbol, date: v.date, amount, note: v.note })) setEditing(null);
      }}
      onDelete={editing === "new" ? undefined : async () => {
        if (!(await askConfirm("删除这笔分红记录?"))) return;
        if (await act({ action: "delete", id: editing.id })) setEditing(null);
      }} />}
    {data && <IncomeView data={data} onAdd={() => { setMessage(""); setEditing("new"); }} onEdit={(d) => { setMessage(""); setEditing(d); }} />}
  </section>;
}
