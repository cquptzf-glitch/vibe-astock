/**
 * 自选 / 持仓「从截图识别」(AI 多模态):选截图 → 交给当前模型读出代码、名称(持仓还有数量、成本)→ 逐条对照截图核对、可改 → 勾选确认后才写入。
 * 草稿不会自动保存。读不到的项是空的,不当 0;代码和名称由腾讯股票搜索再核对一遍,对不上的会标出来;只有名称的给候选,不替你选。
 * 只给识别结果,不评价股票、不给买卖建议。
 */
import { Camera, ChevronDown, ChevronUp, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { DateField } from "./controls";
import { fmtNumber } from "./discoveryData";
import "./screenshot-import.css";

export interface Candidate { symbol: string; name: string }
export interface DraftIn { code: string | null; name: string; shares: number | null; cost: number | null; uncertain: string[]; candidates: Candidate[]; official_name: string | null }
export interface RecognizeResult { drafts: DraftIn[]; warnings: string[]; model: string; images: string[]; note: string }
export interface DraftState { code: string; name: string; shares: string; cost: string; reviewed: boolean }
export type Target = "watch" | "position";

export const MAX_IMAGES = 4;
export const MAX_EDGE = 1600;

/** 长边不超过 max 的缩放尺寸(不放大) */
export function fitSize(width: number, height: number, max = MAX_EDGE): { width: number; height: number } {
  const longest = Math.max(width, height);
  if (longest <= max) return { width, height };
  const k = max / longest;
  return { width: Math.max(1, Math.round(width * k)), height: Math.max(1, Math.round(height * k)) };
}

export function initialState(d: DraftIn): DraftState {
  return { code: d.code ?? "", name: d.official_name || d.name, shares: d.shares == null ? "" : String(d.shares), cost: d.cost == null ? "" : String(d.cost), reviewed: false };
}

const TODAY = () => new Date(Date.now() + 8 * 3600_000).toISOString().slice(0, 10);

/** 一条能不能写入:返回原因;可以写入返回 null */
export function problemOf(s: DraftState, target: Target, existing: readonly string[], date: string): string | null {
  if (!/^\d{6}$/.test(s.code.trim())) return "代码要是 6 位数字";
  if (existing.includes(s.code.trim())) return target === "watch" ? "已在自选里" : "该代码已有持仓:截图可能是总持仓,不能直接叠加,请先核对交易日志再手动记";
  if (target === "position") {
    const shares = Number(s.shares), cost = Number(s.cost);
    if (!s.shares.trim() || !Number.isFinite(shares) || shares <= 0) return "数量要填大于 0 的数";
    if (!s.cost.trim() || !Number.isFinite(cost) || cost <= 0) return "成本要填大于 0 的数";
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || date > TODAY()) return "建仓日期要填,且不能是未来";
  }
  return null;
}

/** 缩小到长边 1600、转 JPEG,省流量也省模型费用;不支持时原图直传(≤ 4 MB) */
export async function prepareImage(file: File): Promise<{ name: string; mime: string; data: string }> {
  const toB64 = (bytes: Uint8Array) => { let t = ""; for (let i = 0; i < bytes.length; i += 0x8000) t += String.fromCharCode(...bytes.subarray(i, i + 0x8000)); return btoa(t); };
  try {
    const bitmap = await createImageBitmap(file);
    const size = fitSize(bitmap.width, bitmap.height);
    const canvas = document.createElement("canvas");
    canvas.width = size.width;
    canvas.height = size.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no canvas");
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, size.width, size.height);
    ctx.drawImage(bitmap, 0, 0, size.width, size.height);
    const url = canvas.toDataURL("image/jpeg", 0.85);
    return { name: file.name, mime: "image/jpeg", data: url.slice(url.indexOf(",") + 1) };
  } catch {
    if (file.size > 4 * 1024 * 1024) throw new Error(`${file.name} 太大且无法压缩,请截小一点`);
    return { name: file.name, mime: file.type || "image/png", data: toB64(new Uint8Array(await file.arrayBuffer())) };
  }
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch("/api" + path, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const data = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data as T;
}

export function DraftsView({ drafts, states, target, existing, date, onChange }: {
  drafts: DraftIn[]; states: DraftState[]; target: Target; existing: readonly string[]; date: string; onChange: (index: number, patch: Partial<DraftState>) => void;
}) {
  return <ul className="si-drafts" aria-label="识别草稿">{drafts.map((d, i) => {
    const s = states[i];
    if (!s) return null;
    const problem = problemOf(s, target, existing, date);
    return <li key={i} className={problem ? "bad" : "ok"}>
      <div className="si-head"><b>草稿 {i + 1}</b>{d.official_name && <small>代码对应:{d.official_name}</small>}</div>
      <div className="si-fields">
        <label><span>代码</span><input value={s.code} inputMode="numeric" aria-label={`草稿 ${i + 1} 代码`} onChange={(e) => onChange(i, { code: e.target.value.trim(), reviewed: false })} /></label>
        <label><span>名称</span><input value={s.name} aria-label={`草稿 ${i + 1} 名称`} onChange={(e) => onChange(i, { name: e.target.value })} /></label>
        {target === "position" && <>
          <label><span>数量(股)</span><input value={s.shares} inputMode="decimal" aria-label={`草稿 ${i + 1} 数量`} onChange={(e) => onChange(i, { shares: e.target.value, reviewed: false })} /></label>
          <label><span>成本(元)</span><input value={s.cost} inputMode="decimal" aria-label={`草稿 ${i + 1} 成本`} onChange={(e) => onChange(i, { cost: e.target.value, reviewed: false })} /></label>
        </>}
      </div>
      {d.candidates.length > 0 && <div className="si-cands" aria-label={`草稿 ${i + 1} 候选`}>按名称搜到:{d.candidates.map((c) => <button type="button" key={c.symbol} onClick={() => onChange(i, { code: c.symbol, name: c.name, reviewed: false })}>{c.name} {c.symbol}</button>)}</div>}
      {d.uncertain.length > 0 && <ul className="si-uncertain" aria-label={`草稿 ${i + 1} 待核对`}>{d.uncertain.map((u) => <li key={u}>{u}</li>)}</ul>}
      {problem ? <p className="si-problem" role="status">现在还不能写入:{problem}</p>
        : <label className="si-review"><input type="checkbox" checked={s.reviewed} aria-label={`草稿 ${i + 1} 已核对`} onChange={(e) => onChange(i, { reviewed: e.target.checked })} />已对照截图核对{target === "position" ? `这条(${fmtNumber(Number(s.shares), 0)} 股 · 成本 ${fmtNumber(Number(s.cost))})` : "这条"}</label>}
    </li>;
  })}</ul>;
}

export function ScreenshotImport({ target, existing, onSaved }: { target: Target; existing: readonly string[]; onSaved: () => void }) {
  const [open, setOpen] = useState(false);
  const [vision, setVision] = useState<boolean | null>(null);
  const [files, setFiles] = useState<File[]>([]);
  const [result, setResult] = useState<RecognizeResult | null>(null);
  const [states, setStates] = useState<DraftState[]>([]);
  const [date, setDate] = useState(TODAY());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const fileRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (!open) return;
    void fetch("/api/mobile/ai/config", { headers: { Accept: "application/json" } }).then((r) => r.json()).then((c: { vision?: boolean; configured?: boolean }) => setVision(Boolean(c.configured && c.vision))).catch(() => setVision(null));
  }, [open]);

  const recognize = async () => {
    setBusy(true); setError(""); setNotice(""); setResult(null);
    try {
      const images = [];
      for (const file of files) images.push(await prepareImage(file));
      const out = await post<RecognizeResult>("/mobile/ai/recognize", { images, target });
      setResult(out);
      setStates(out.drafts.map(initialState));
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const ready = result ? states.map((s, i) => ({ s, i })).filter(({ s }) => s.reviewed && !problemOf(s, target, existing, date)) : [];
  const save = async () => {
    setBusy(true); setError(""); setNotice("");
    let done = 0;
    const failed: string[] = [];
    for (const { s } of ready) {
      try {
        if (target === "watch") await post("/mobile/watchlist", { symbol: s.code.trim(), name: s.name.trim() });
        else await post("/mobile/journal", { action: "record", symbol: s.code.trim(), name: s.name.trim(), note: "截图识别(期初持仓)", fills: [{ type: "buy", date, price: Number(s.cost), shares: Number(s.shares), fee: 0 }] });
        done += 1;
      } catch (e) { failed.push(`${s.code} ${e instanceof Error ? e.message : String(e)}`); }
    }
    setBusy(false);
    if (done > 0) onSaved();
    setNotice(`已写入 ${done} 条${failed.length ? `;${failed.length} 条没写成:${failed.join(";")}` : ""}。`);
    if (done > 0) { setResult(null); setStates([]); setFiles([]); }
  };

  return <section className="screenshot-import" aria-label="从截图识别">
    <button type="button" className="si-toggle" aria-expanded={open} onClick={() => setOpen(!open)}><Camera size={15} /> 从截图识别({target === "watch" ? "自选" : "持仓"})· AI {open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</button>
    {open && <div className="si-body">
      <p className="si-note">选 1–{MAX_IMAGES} 张证券 App 的{target === "watch" ? "自选列表或持仓" : "持仓页"}截图,由你在 AI 设置里配置的多模态模型读出{target === "watch" ? "代码和名称" : "代码、名称、数量和成本"}。读出来的只是草稿:逐条对照截图核对(可以改),勾选确认后才写入。截图不保存。</p>
      {vision === false && <p className="si-problem" role="status">当前模型还没有打开「支持图片识别」。到「AI」页的模型配置里选一个多模态模型,打开开关(或点「测试图片识别」)后再来。</p>}
      <div className="si-actions">
        <button type="button" disabled={busy} onClick={() => fileRef.current?.click()}><Camera size={13} /> 选截图{files.length ? `(已选 ${files.length})` : ""}</button>
        <input ref={fileRef} type="file" accept="image/png,image/jpeg,image/webp,image/gif" multiple hidden onChange={(e) => { setFiles(Array.from(e.target.files ?? []).slice(0, MAX_IMAGES)); setResult(null); setError(""); setNotice(""); e.target.value = ""; }} />
        <button type="button" className="primary" disabled={busy || !files.length || vision === false} onClick={() => void recognize()}>{busy && !result ? <Loader2 size={13} className="spin" /> : null} 识别</button>
      </div>
      {files.length > 0 && <p className="si-note">{files.map((f) => f.name).join("、")}</p>}
      {error && <p className="intra-warn" role="alert">{error}</p>}
      {notice && <p className="si-note" role="status">{notice}</p>}
      {result && <>
        {result.warnings.map((w) => <p key={w} className="si-note">模型提示:{w}</p>)}
        {result.drafts.length === 0 ? <p className="si-problem" role="status">没读出任何股票。换一张更清晰的截图,或换一个模型再试。</p> : <>
          {target === "position" && <label className="si-date"><span>建仓日期(截图读不到,按这个日期记一笔买入)</span><DateField aria-label="建仓日期" max={TODAY()} value={date} onChange={(e) => setDate(e.target.value)} /></label>}
          <DraftsView drafts={result.drafts} states={states} target={target} existing={existing} date={date} onChange={(i, patch) => setStates((old) => old.map((s, j) => (j === i ? { ...s, ...patch } : s)))} />
          <div className="si-actions"><button type="button" className="primary" disabled={busy || ready.length === 0} onClick={() => void save()}>写入 {ready.length} 条{target === "watch" ? "到自选" : "(期初持仓,记为一笔买入)"}</button></div>
          {target === "position" && <p className="si-note">持仓按「建仓日期 + 截图里的成本价 + 数量」记成一笔买入(费用记 0),只是把现有持仓带进来,真实买入日期、分批成本以后可以在交易日志里改。已有持仓的代码不让叠加。</p>}
        </>}
        <p className="si-note">{result.note}(模型:{result.model})</p>
      </>}
    </div>}
  </section>;
}
