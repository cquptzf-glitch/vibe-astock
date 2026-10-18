/**
 * 我的研报:从文件导入,先预览再保存。支持文本类文件(.txt / .md / .markdown / .csv / .json,UTF-8 或 GBK);
 * PDF、Word、Excel 通过本机 Rust 解析；PDF 保留原始分页供检索引用。
 * 文件只在本机读,不上传、不发给 AI;预览看到的就是将要保存的正文,保存前可以改标题、分类、股票代码。
 */
import { FileUp } from "lucide-react";
import { useRef, useState } from "react";

import { decodeFile } from "./TradeImport";
import "./report-import.css";

export const MAX_CHARS = 250_000;
export const MAX_FILE_BYTES = 2 * 1024 * 1024;
const TEXT_EXT = /\.(txt|md|markdown|csv|tsv|json)$/i;

export interface ReportDraft { title: string; body: string; chars: number; source?: string }

/** 文件名去掉扩展名当标题(≤ 80 字);正文原样,不改写 */
export function draftOf(fileName: string, text: string): ReportDraft | { error: string } {
  if (!TEXT_EXT.test(fileName)) return { error: "只支持 .txt / .md / .csv / .json 这类文本文件;PDF、Word、Excel 请先复制出文字或导出成文本" };
  const body = text.replace(/^﻿/, "").replace(/\r\n?/g, "\n");
  if (!body.trim()) return { error: "文件里没有文字" };
  if (body.includes("\u0000")) return { error: "这像是二进制文件,不是文本" };
  if (body.length > MAX_CHARS) return { error: `正文有 ${body.length.toLocaleString("zh-CN")} 个字符,超过上限 ${MAX_CHARS.toLocaleString("zh-CN")},请拆成几份` };
  return { title: fileName.replace(/\.[^.]+$/, "").slice(0, 80), body, chars: body.length };
}

export function ReportPreview({ draft, title, category, symbol, busy, onTitle, onCategory, onSymbol, onSave, onCancel }: {
  draft: ReportDraft; title: string; category: string; symbol: string; busy: boolean;
  onTitle: (v: string) => void; onCategory: (v: string) => void; onSymbol: (v: string) => void; onSave: () => void; onCancel: () => void;
}) {
  return <div className="ri-preview" aria-label="研报导入预览">
    <label><span>标题</span><input value={title} onChange={(e) => onTitle(e.target.value)} aria-label="研报标题" maxLength={200} /></label>
    <div className="ri-row">
      <label><span>分类(选填)</span><input value={category} onChange={(e) => onCategory(e.target.value)} aria-label="研报分类" /></label>
      <label><span>股票代码(选填)</span><input value={symbol} inputMode="numeric" onChange={(e) => onSymbol(e.target.value)} aria-label="研报股票代码" /></label>
    </div>
    <p className="ri-meta">共 {draft.chars.toLocaleString("zh-CN")} 个字符 · 下面是开头的预览,保存的是全文</p>
    {draft.source === "file_import_pdf_pages_v1" && <p className="ri-meta">已保留 PDF 原始分页（含空白页），检索引用将包含页码。</p>}
    <pre className="ri-body" aria-label="正文预览">{draft.body.slice(0, 800)}{draft.body.length > 800 ? "\n…" : ""}</pre>
    <div className="ri-actions"><button type="button" className="primary" disabled={busy || !title.trim()} onClick={onSave}>保存为研报</button><button type="button" disabled={busy} onClick={onCancel}>取消</button></div>
  </div>;
}

export function ReportImport({ onSaved }: { onSaved: () => void }) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<ReportDraft | null>(null);
  const [title, setTitle] = useState("");
  const [category, setCategory] = useState("");
  const [symbol, setSymbol] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const fileRef = useRef<HTMLInputElement | null>(null);

  const pick = async (file: File | undefined) => {
    if (!file) return;
    setError("");
    setDraft(null);
    if (file.size > MAX_FILE_BYTES) { setError("文件超过 2 MB,请拆开再导入"); return; }
    setBusy(true);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let out: ReportDraft | { error: string };
      if (/\.(pdf|docx|xlsx|xls|xlsb|ods)$/i.test(file.name)) {
        let binary = "";
        for (let i = 0; i < bytes.length; i += 16384) binary += String.fromCharCode(...bytes.subarray(i, i + 16384));
        const response = await fetch("/api/mobile/documents/parse", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ name: file.name, data: btoa(binary) }) });
        const data = await response.json() as { body?: string; source?: string; message?: string };
        if (!response.ok || !data.body) throw new Error(data.message ?? "文件没有解析出正文");
        out = draftOf(file.name.replace(/\.[^.]+$/, ".txt"), data.body);
        if (!("error" in out)) {out.title = file.name.replace(/\.[^.]+$/, "").slice(0, 80);out.source=data.source;}
      } else out = draftOf(file.name, decodeFile(bytes.buffer));
      if ("error" in out) { setError(out.error); return; }
      setDraft(out); setTitle(out.title);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  };
  const save = async () => {
    if (!draft) return;
    setBusy(true);
    setError("");
    try {
      const response = await fetch("/api/mobile/documents", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ kind: "report", title: title.trim(), body: draft.body, category: category.trim(), symbol: symbol.trim() || null, status: "active", source: draft.source ?? "file_import" }) });
      const data = await response.json().catch(() => null) as { message?: string; error?: string } | null;
      if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
      setDraft(null);
      setOpen(false);
      onSaved();
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  return <section className="report-import" aria-label="从文件导入研报">
    <button type="button" className="ri-toggle" aria-expanded={open} onClick={() => setOpen(!open)}><FileUp size={15} /> 从文件导入研报(先预览)</button>
    {open && <div className="ri-body-wrap">
      <p className="ri-meta">支持文本、PDF、DOCX、Excel 与 ODS，≤ 2 MB；先预览再保存。正文在本机解析，不发给 AI。扫描 PDF 需先做 OCR；旧 DOC 请另存为 DOCX。</p>
      <button type="button" className="ri-pick" disabled={busy} onClick={() => fileRef.current?.click()}><FileUp size={13} /> {busy ? "解析中…" : "选文件"}</button>
      <input ref={fileRef} type="file" accept=".txt,.md,.markdown,.csv,.tsv,.json,.pdf,.docx,.xlsx,.xls,.xlsb,.ods" hidden onChange={(e) => { void pick(e.target.files?.[0]); e.target.value = ""; }} />
      {error && <p className="intra-warn" role="alert">{error}</p>}
      {draft && <ReportPreview draft={draft} title={title} category={category} symbol={symbol} busy={busy} onTitle={setTitle} onCategory={setCategory} onSymbol={setSymbol} onSave={() => void save()} onCancel={() => setDraft(null)} />}
    </div>}
  </section>;
}
