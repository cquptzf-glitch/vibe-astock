import sectorsData from "../verticals/finance/data/sectors.json";
import { useEffect, useRef, useState } from "react";
import { DateField, WebSelect, askConfirm } from "./controls";
import { chinaToday } from "./emotionData";
import "./research-workflow.css";
import { CalculationAuditPanel, type CalculationSummary } from "./CalculationAudit";

export const RESEARCH_MODES = [
  { id:"industry",label:"产业研究 · 六阶段",stock:false },
  { id: "deep", label: "深度研究 · 六阶段", stock: true },
  { id: "debate", label: "多空辩论 · 六阶段", stock: true },
  { id: "emotion", label: "情绪复盘 · 六阶段", stock: false },
  { id: "weekly", label: "市场周报", stock: false },
  { id: "market", label: "每日市场复盘", stock: false },
] as const;
interface Paragraph { text: string; evidence: string[] }
interface Condition { metric: string; direction: string; reason: string }
interface Stage { stage: string; paragraphs: Paragraph[]; checks: { ok: boolean; level: string; actual: number | null; display?: string | null; claim: { label?: string } }[]; calculations?: CalculationSummary[]; conditions: Condition[]; verified: boolean; model: string; at: string }
export interface ResearchTask { id: number; title: string; mode: string; date: string; status: string; stages: string[]; results: Stage[]; revision: number; archive_id?: number; error?: { message: string; audit?: unknown }; gaps: { source: string; reason: string }[] }

async function api<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api${path}`, { method: body === undefined ? "GET" : "POST", headers: { "Content-Type": "application/json" }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
  const data = await response.json();
  if (!response.ok) throw new Error(data.message ?? `HTTP ${response.status}`);
  return data as T;
}

export function ResearchTaskView({ task, onConditions }: { task: ResearchTask; onConditions?: (items: Condition[]) => void }) {
  return <section className="rw-results" aria-label="研究进度与结论">
    <h2>{task.title}</h2>
    <p>{task.status === "complete" ? "全部阶段已完成" : task.status === "cancelled" ? "已中止并归档已完成部分" : `已完成 ${task.results.length}/${task.stages.length} 阶段`}{task.archive_id ? ` · 报告已存入「我的研报」(#${task.archive_id})` : ""}</p>
    <ol className="rw-progress">{task.stages.map((name, i) => <li key={name} className={i < task.results.length ? "done" : ""}>{name}{i < task.results.length ? " ✓" : ""}</li>)}</ol>
    {task.gaps?.length > 0 && <details><summary>资料缺口 ({task.gaps.length})</summary>{task.gaps.map((g, i) => <p key={i}>{g.source}：{g.reason}</p>)}</details>}
    {task.error && <div role="alert" className="rw-error"><p>{task.error.message}</p>{task.error.audit != null && <details><summary>查看数字与引用核对详情</summary><pre>{JSON.stringify(task.error.audit, null, 2)}</pre></details>}</div>}
    {task.results.map((stage, i) => <article className="rw-stage" key={i}>
      <h3>{stage.stage}</h3>
      {stage.paragraphs.map((p, j) => <p key={j}>{p.text} <small>[{p.evidence.join("、")}]</small></p>)}
      {stage.checks.length > 0 && <details><summary>数字复核 ({stage.checks.length})</summary><ul>{stage.checks.map((c, j) => <li key={j}>{c.claim.label ?? "数字"}：{c.display ?? c.actual ?? "未能复核"} · {c.level === "calculation" ? "受控计算链" : c.level === "formula" ? "算式核对" : c.level === "source" ? "数据包一致" : "无来源"}</li>)}</ul></details>}
      {!!stage.calculations?.length && <details><summary>本阶段计算 ({stage.calculations.length})</summary>{stage.calculations.map((c) => <p key={`${c.node_id}-${c.calculation_id}`}>{c.function} · {c.output.status === "ok" ? c.output.display ?? "多项结果" : c.output.reason || c.output.status}</p>)}</details>}
      {stage.conditions.length > 0 && onConditions && <button type="button" onClick={() => onConditions(stage.conditions)}>将本阶段条件加入明日验证</button>}
      <small>{stage.model} · {stage.at}</small>
    </article>)}
    <CalculationAuditPanel key={task.id} taskId={task.id} />
  </section>;
}

export function ResearchWorkflowPanel() {
  const [sector,setSector]=useState(sectorsData.sectors[0]?.key ?? "");
  const [mode, setMode] = useState("deep");
  const [symbol, setSymbol] = useState("");
  const [date, setDate] = useState(chinaToday);
  const [question, setQuestion] = useState("");
  const [tasks, setTasks] = useState<ResearchTask[]>([]);
  const [task, setTask] = useState<ResearchTask | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const sequence = useRef(0);
  const mounted = useRef(true);
  const advancing = useRef(false);
  useEffect(() => {
    mounted.current = true;
    void api<{ tasks: ResearchTask[] }>("/mobile/research/workflows").then((d) => { if (mounted.current) setTasks(d.tasks); }).catch((e) => { if (mounted.current) setError(String(e)); });
    return () => { mounted.current = false; sequence.current++; };
  }, []);
  async function refresh() { const d = await api<{ tasks: ResearchTask[] }>("/mobile/research/workflows"); if (mounted.current) setTasks(d.tasks); }
  async function run(current: ResearchTask, all: boolean) {
    if (advancing.current) return;
    advancing.current = true; setBusy(true); setError("");
    const token = ++sequence.current;
    try {
      do {
        current = await api<ResearchTask>("/mobile/ai/workflow", { action: "advance", id: current.id });
        if (!mounted.current || token !== sequence.current) break;
        setTask(current);
        if (!all || current.status !== "active" || current.error) break;
      } while (token === sequence.current);
      if (mounted.current) await refresh();
    } catch (e) { if (mounted.current && token === sequence.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { advancing.current = false; if (mounted.current) setBusy(false); }
  }
  async function create() {
    if (busy) return;
    setBusy(true); setError(""); setNotice("");
    try {
      const current = await api<ResearchTask>("/mobile/ai/workflow", { mode, symbol, sector, date: (RESEARCH_MODES.find((m) => m.id === mode)?.stock || mode==="industry") ? chinaToday() : date, question });
      if (mounted.current) { setTask(current); await refresh(); }
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { if (mounted.current) setBusy(false); }
  }
  async function cancel() {
    if (!task || !await askConfirm("中止这项研究？已完成的部分会存入研报，正在返回的阶段不会写入。")) return;
    sequence.current++;
    try { const stopped = await api<ResearchTask>("/mobile/ai/workflow", { action: "cancel", id: task.id }); setTask(stopped); await refresh(); }
    catch (e) { setError(String(e)); }
  }
  async function conditions(items: Condition[]) {
    if (!task || !await askConfirm("将这些条件加入该复盘日的明日验证？已有条件会保留，同指标条件需先在明日验证页确认。")) return;
    try {
      const existing = await api<{ plan: { locked: boolean; items: Condition[] } }>(`/mobile/shortline/verify?date=${task.date}`);
      if (existing.plan.locked) throw new Error("这一天已锁定，不能再补写验证条件");
      const merged = [...existing.plan.items];
      for (const item of items) { if (!merged.some((m) => m.metric === item.metric)) merged.push({ ...item, reason: item.reason.slice(0, 200) }); }
      await api("/mobile/shortline/verify", { action: "save", date: task.date, items: merged });
      setNotice("条件已写入明日验证。");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  return <section className="mobile-section rw-panel">
    <div className="mobile-section-title"><div><h1>研究工作流</h1><p>程序先取公开数据，模型逐阶段研究；数字和证据核对通过后才保存结论。每阶段自动生成一版研报，可暂停后继续。</p></div></div>
    <div className="rw-form">
      <label>研究类型<WebSelect value={mode} onChange={(e) => setMode(e.target.value)}>{RESEARCH_MODES.map((m) => <option value={m.id} key={m.id}>{m.label}</option>)}</WebSelect></label>
      {RESEARCH_MODES.find((m) => m.id === mode)?.stock ? <label>股票代码<input aria-label="研究股票代码" value={symbol} maxLength={16} placeholder="例如 600519" onChange={(e) => setSymbol(e.target.value)} /></label> : <label>复盘日期<DateField value={date} onChange={(e) => setDate(e.target.value)} /></label>}
      {mode==="industry"&&<label>产业赛道<WebSelect value={sector} onChange={e=>setSector(e.target.value)}>{sectorsData.sectors.map(s=><option key={s.key} value={s.key}>{s.label}</option>)}</WebSelect></label>}
      <label>研究问题<textarea aria-label="研究问题" value={question} maxLength={1000} placeholder="希望核对哪些事实、分歧或风险？" onChange={(e) => setQuestion(e.target.value)} /></label>
      <button type="button" disabled={busy} onClick={() => void create()}>创建新研究</button>
    </div>
    {error && <p role="alert" className="rw-error">{error}</p>}{notice && <p role="status">{notice}</p>}
    {task && <>
      <div className="rw-actions">{task.status === "active" && <><button type="button" disabled={busy} onClick={() => void run(task, false)}>{busy ? "当前阶段执行中…" : task.error ? "重试当前阶段" : "执行下一阶段"}</button><button type="button" disabled={busy} onClick={() => void run(task, true)}>连续完成剩余阶段</button>{busy && <button type="button" onClick={() => { sequence.current++; setNotice("已暂停后续阶段，当前阶段结束后会保存；稍后可继续。"); }}>暂停后续阶段</button>}<button type="button" onClick={() => void cancel()}>中止并归档</button></>}</div>
      <ResearchTaskView task={task} onConditions={(items) => void conditions(items)} />
    </>}
    {tasks.length > 0 && <details><summary>历史研究与未完成任务 ({tasks.length})</summary><div className="rw-history">{tasks.map((j) => <button type="button" key={j.id} disabled={busy} onClick={() => { setTask(j); setError(""); }}>{j.title} · {j.results.length}/{j.stages.length} · {j.status === "complete" ? "已完成" : j.status === "cancelled" ? "已中止" : "可继续"}</button>)}</div></details>}
  </section>;
}
