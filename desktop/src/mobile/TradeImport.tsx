/**
 * 交易日志 · 从表格导入成交与分红(移动版)。粘贴或选一个 CSV / 制表符分隔的文件 → 本机预览(不写)→ 导入(一个事务)。
 * 全部在本机解析,不发给任何 AI,不联网。每行先过和手动记一笔同样的检查;不对的行写原因、不导入,不猜不补。
 */
import { ChevronDown, ChevronUp, FileUp, Undo2 } from "lucide-react";
import { useRef, useState } from "react";

import { askConfirm } from "./controls";
import { fmtNumber } from "./discoveryData";
import "./trade-import.css";

export interface ImportRow { line: number; date: string; type: string; symbol: string; name: string; shares: number | null; price: number | null; fee: number | null; amount: number | null; status: "ok" | "duplicate" | "problem"; why: string | null }
export interface ImportSummary { rows: number; importable: number; duplicates: number; problems: number; episodes: number; closed_episodes: number; open_episodes: number; dividends: number }
export interface ImportPreview { summary: ImportSummary; rows: ImportRow[]; episodes: { symbol: string; name: string; fills: number; open_shares: number; closed: boolean }[]; note: string }
export interface ImportDone { imported_entries: number; entry_ids: number[]; imported_fills: number; imported_dividends: number; skipped: { line: number; why: string | null }[] }

export const TEMPLATE = "日期,类型,代码,名称,数量,价格,费用,金额,比例\n2026-03-05,买入,600519,贵州茅台,100,1450,5,\n2026-03-20,卖出,600519,贵州茅台,100,1500,12.5,\n2026-06-01,现金分红,600519,贵州茅台,,,,300";
const STATUS_TEXT = { ok: "可导入", duplicate: "已记过", problem: "有问题" } as const;

/** 券商导出常是 GBK:先按 UTF-8 严格解码,失败再按 GBK */
export function decodeFile(buffer: ArrayBuffer): string {
  try { return new TextDecoder("utf-8", { fatal: true }).decode(buffer); } catch { /* 不是 UTF-8 */ }
  try { return new TextDecoder("gbk").decode(buffer); } catch { return new TextDecoder().decode(buffer); }
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch("/api" + path, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data as T;
}

export function ImportPreviewView({ preview }: { preview: ImportPreview }) {
  const s = preview.summary;
  return <div className="ti-preview">
    <div className="ti-stats" aria-label="导入预览汇总">
      <div><span>可导入</span><strong>{s.importable}</strong><small>{s.episodes} 轮交易 · {s.dividends} 笔分红</small></div>
      <div><span>已记过</span><strong>{s.duplicates}</strong><small>不重复导入</small></div>
      <div><span>有问题</span><strong className={s.problems > 0 ? "down" : ""}>{s.problems}</strong><small>不导入</small></div>
    </div>
    <div className="intra-table-wrap"><table className="intra-table" aria-label="逐行预览">
      <thead><tr><th>行</th><th>日期</th><th>类型</th><th>代码</th><th>数量</th><th>价格/金额</th><th>状态</th></tr></thead>
      <tbody>{preview.rows.map((r) => <tr key={r.line} className={"ti-" + r.status}>
        <td>{r.line}</td><td>{r.date}</td><td>{r.type}</td><td>{r.symbol}{r.name ? " " + r.name : ""}</td><td>{r.shares == null ? "—" : fmtNumber(r.shares, 0)}</td>
        <td>{r.amount != null ? fmtNumber(r.amount) : r.price == null ? "—" : fmtNumber(r.price)}</td>
        <td>{STATUS_TEXT[r.status]}{r.why && <small>{r.why}</small>}</td></tr>)}</tbody></table></div>
    <p className="ti-note">{preview.note}</p>
  </div>;
}

export function TradeImportCard({ onImported }: { onImported: () => void }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [done, setDone] = useState<ImportDone | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const fileRef = useRef<HTMLInputElement | null>(null);

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try { await work(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };
  const edit = (next: string) => { setText(next); setPreview(null); setDone(null); setError(""); };
  const onFile = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > 512 * 1024) { setError("文件超过 512 KB,请分成几份导入"); return; }
    edit(decodeFile(await file.arrayBuffer()));
  };
  const undo = async () => {
    if (!done || !(await askConfirm(`撤销这次导入?将删除刚导入的 ${done.imported_entries} 条交易记录;导入的 ${done.imported_dividends} 笔分红不会自动删除(在「已实现与分红」里逐笔删)。`))) return;
    await run(async () => {
      for (const id of done.entry_ids) await post("/mobile/journal", { action: "delete", id });
      setDone(null);
      onImported();
    });
  };

  return <section className="trade-import" aria-label="从表格导入成交">
    <button type="button" className="ti-toggle" aria-expanded={open} onClick={() => setOpen(!open)}><FileUp size={15} /> 从表格导入成交与分红 {open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</button>
    {open && <div className="ti-body">
      <p className="ti-note">粘贴券商导出的表格(CSV 或从表格软件复制),或选一个文件。第一行要是列名,至少要有「日期」「类型」「代码」;类型认得买入 / 卖出 / 现金分红 / 期初持仓 / 送转拆股；期初价格填每股成本，送转另加「比例」列，填新股数 / 原股数(十转十填 2)，无现金流。全部在本机处理,不发给任何 AI。</p>
      <textarea value={text} onChange={(e) => edit(e.target.value)} rows={5} aria-label="表格内容" placeholder={TEMPLATE} spellCheck={false} />
      <div className="ti-actions">
        <button type="button" disabled={busy} onClick={() => fileRef.current?.click()}><FileUp size={13} /> 选文件</button>
        <input ref={fileRef} type="file" accept=".csv,.tsv,.txt,text/csv,text/plain" hidden onChange={(e) => { void onFile(e.target.files?.[0]); e.target.value = ""; }} />
        <button type="button" disabled={busy} onClick={() => edit(TEMPLATE)}>填入示例</button>
        <button type="button" className="primary" disabled={busy || !text.trim()} onClick={() => void run(async () => setPreview(await post<ImportPreview>("/mobile/journal/import/preview", { text })))}>预览</button>
      </div>
      {error && <p className="intra-warn" role="alert">{error}</p>}
      {preview && <ImportPreviewView preview={preview} />}
      {preview && <div className="ti-actions">
        <button type="button" className="primary" disabled={busy || preview.summary.importable === 0}
          onClick={() => void run(async () => { setDone(await post<ImportDone>("/mobile/journal/import", { text })); setPreview(null); onImported(); })}>
          导入 {preview.summary.episodes} 轮交易{preview.summary.dividends > 0 ? ` + ${preview.summary.dividends} 笔分红` : ""}</button>
      </div>}
      {done && <div className="ti-done" role="status">已导入 {done.imported_entries} 轮交易({done.imported_fills} 笔成交)、{done.imported_dividends} 笔分红;跳过 {done.skipped.length} 行。
        <button type="button" disabled={busy || done.entry_ids.length === 0} onClick={() => void undo()}><Undo2 size={13} /> 撤销这批交易</button></div>}
    </div>}
  </section>;
}
