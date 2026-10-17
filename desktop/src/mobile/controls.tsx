/**
 * 网页自绘控件:替换 Android WebView 自带的下拉、日期选择、确认/提示对话框。
 * 系统控件在不同机型上长相不一、不跟主题、会把页面顶出去;这里全部用页面里的元素画,跟着明暗主题和紧凑密度走。
 * WebSelect / DateField 的 onChange 回调形如 { target: { value } },原来写给原生控件的处理函数可以直接用。
 */
import { Check, ChevronDown, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, CalendarDays } from "lucide-react";
import { Children, Fragment, isValidElement, useCallback, useEffect, useId, useMemo, useRef, useState, useSyncExternalStore, type ReactElement, type ReactNode } from "react";

import "./controls.css";

export type ChangeLike = { target: { value: string } };

// ------------------------------------------------------------ 下拉
interface Option { value: string; label: ReactNode; disabled: boolean }

function collectOptions(children: ReactNode, out: Option[] = []): Option[] {
  Children.forEach(children, (child) => {
    if (!isValidElement(child)) return;
    const element = child as ReactElement<{ value?: string | number; disabled?: boolean; children?: ReactNode }>;
    if (element.type === Fragment) collectOptions(element.props.children, out);
    else if (element.type === "option") {
      const label = element.props.children;
      out.push({ value: String(element.props.value ?? (typeof label === "string" ? label : "")), label, disabled: Boolean(element.props.disabled) });
    }
  });
  return out;
}

export function useDismiss(open: boolean, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, close]);
}

/** 底部弹出的面板。遮罩是 button:放在 <label> 里时点击遮罩不会被转发成「再点一次触发器」。 */
export function Sheet({ open, onClose, title, children }: { open: boolean; onClose: () => void; title: string; children: ReactNode }) {
  useDismiss(open, onClose);
  if (!open) return null;
  return <div className="wc-layer">
    <button type="button" className="wc-backdrop" aria-label="关闭" tabIndex={-1} onClick={onClose} />
    <div className="wc-sheet" role="dialog" aria-label={title}><div className="wc-sheet-head"><strong>{title}</strong></div>{children}</div>
  </div>;
}

export function WebSelect({ value, onChange, children, disabled, className, "aria-label": ariaLabel }: {
  value: string | number; onChange: (event: ChangeLike) => void; children: ReactNode; disabled?: boolean; className?: string; "aria-label"?: string;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const options = useMemo(() => collectOptions(children), [children]);
  const current = options.find((item) => item.value === String(value));
  const close = useCallback(() => setOpen(false), []);
  const title = ariaLabel ?? "请选择";
  return <>
    <button type="button" className={"wc-select" + (className ? " " + className : "")} role="combobox" aria-haspopup="listbox" aria-expanded={open} aria-controls={id}
      aria-label={ariaLabel} disabled={disabled} onClick={() => setOpen(true)}>
      <span className="wc-value">{current?.label ?? "请选择"}</span><ChevronDown size={14} aria-hidden="true" />
    </button>
    <Sheet open={open} onClose={close} title={title}>
      <ul id={id} role="listbox" aria-label={title} className="wc-options">{options.map((item) => <li key={item.value} role="presentation">
        <button type="button" role="option" aria-selected={item.value === String(value)} disabled={item.disabled} className={item.value === String(value) ? "on" : ""}
          onClick={() => { setOpen(false); if (item.value !== String(value)) onChange({ target: { value: item.value } }); }}>
          <span>{item.label}</span>{item.value === String(value) && <Check size={16} aria-hidden="true" />}</button></li>)}</ul>
    </Sheet>
  </>;
}

// ------------------------------------------------------------ 日期
const pad = (n: number) => String(n).padStart(2, "0");
const iso = (y: number, m: number, d: number) => `${y}-${pad(m)}-${pad(d)}`;
const parse = (value: string | undefined) => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value ?? "");
  return m ? { y: Number(m[1]), m: Number(m[2]), d: Number(m[3]) } : null;
};
const WEEK = ["一", "二", "三", "四", "五", "六", "日"];

/** 某月的日历格:周一开头,前后补空 */
export function monthCells(year: number, month: number): (number | null)[] {
  const first = (new Date(Date.UTC(year, month - 1, 1)).getUTCDay() + 6) % 7;
  const days = new Date(Date.UTC(year, month, 0)).getUTCDate();
  const cells: (number | null)[] = Array(first).fill(null);
  for (let d = 1; d <= days; d += 1) cells.push(d);
  while (cells.length % 7 !== 0) cells.push(null);
  return cells;
}

export function DateField({ value, onChange, min, max, "aria-label": ariaLabel, className, disabled, today }: {
  value: string; onChange: (event: ChangeLike) => void; min?: string; max?: string; "aria-label"?: string; className?: string; disabled?: boolean; today?: string;
}) {
  const [open, setOpen] = useState(false);
  const picked = parse(value);
  const now = new Date(Date.now() + 8 * 3600_000);
  const todayText = today ?? iso(now.getUTCFullYear(), now.getUTCMonth() + 1, now.getUTCDate());
  const seed = picked ?? parse(todayText)!;
  const [view, setView] = useState({ y: seed.y, m: seed.m });
  useEffect(() => { if (open) { const s = parse(value) ?? parse(todayText)!; setView({ y: s.y, m: s.m }); } }, [open]); // eslint-disable-line react-hooks/exhaustive-deps
  const close = useCallback(() => setOpen(false), []);
  const shift = (months: number) => setView((v) => { const total = v.y * 12 + (v.m - 1) + months; return { y: Math.floor(total / 12), m: (total % 12) + 1 }; });
  const allowed = (text: string) => (!min || text >= min) && (!max || text <= max);
  const title = ariaLabel ?? "选择日期";
  return <>
    <button type="button" className={"wc-select wc-date" + (className ? " " + className : "")} aria-haspopup="dialog" aria-expanded={open} aria-label={ariaLabel} data-value={value} disabled={disabled} onClick={() => setOpen(true)}>
      <CalendarDays size={14} aria-hidden="true" /><span className="wc-value">{value || "选择日期"}</span>
    </button>
    <Sheet open={open} onClose={close} title={title}>
      <div className="wc-cal">
        <div className="wc-cal-nav">
          <button type="button" aria-label="上一年" onClick={() => shift(-12)}><ChevronsLeft size={16} /></button>
          <button type="button" aria-label="上个月" onClick={() => shift(-1)}><ChevronLeft size={16} /></button>
          <strong aria-live="polite">{view.y} 年 {view.m} 月</strong>
          <button type="button" aria-label="下个月" onClick={() => shift(1)}><ChevronRight size={16} /></button>
          <button type="button" aria-label="下一年" onClick={() => shift(12)}><ChevronsRight size={16} /></button>
        </div>
        <div className="wc-cal-grid" role="grid" aria-label={`${view.y} 年 ${view.m} 月`}>
          {WEEK.map((w) => <span key={w} className="wc-cal-week" role="columnheader">{w}</span>)}
          {monthCells(view.y, view.m).map((d, i) => {
            if (d == null) return <span key={i} />;
            const text = iso(view.y, view.m, d);
            return <button type="button" key={i} role="gridcell" disabled={!allowed(text)} aria-pressed={text === value} aria-label={text}
              className={(text === value ? "on " : "") + (text === todayText ? "today" : "")} onClick={() => { setOpen(false); onChange({ target: { value: text } }); }}>{d}</button>;
          })}
        </div>
        <div className="wc-cal-foot">
          <button type="button" disabled={!allowed(todayText)} onClick={() => { setOpen(false); onChange({ target: { value: todayText } }); }}>今天</button>
          <button type="button" onClick={close}>取消</button>
        </div>
      </div>
    </Sheet>
  </>;
}

// ------------------------------------------------------------ 确认 / 提示(代替 window.confirm / window.alert)
type DialogRequest = { id: number; kind: "confirm" | "alert"; message: string; resolve: (ok: boolean) => void };
let dialogSeq = 0;
let dialogQueue: DialogRequest[] = [];
const dialogListeners = new Set<() => void>();
const emitDialogs = () => dialogListeners.forEach((listener) => listener());
let hostCount = 0;

function enqueue(kind: DialogRequest["kind"], message: string): Promise<boolean> {
  // 没有挂载对话框宿主(测试环境、页面外)就退回系统对话框,保证功能不丢
  if (hostCount === 0) {
    if (typeof window === "undefined") return Promise.resolve(false);
    if (kind === "confirm") return Promise.resolve(window.confirm(message));
    window.alert(message);
    return Promise.resolve(true);
  }
  return new Promise((resolve) => { dialogQueue = [...dialogQueue, { id: ++dialogSeq, kind, message, resolve }]; emitDialogs(); });
}
export const askConfirm = (message: string) => enqueue("confirm", message);
export const showAlert = (message: string) => enqueue("alert", message).then(() => undefined);

export function DialogHost() {
  const queue = useSyncExternalStore((listener) => { dialogListeners.add(listener); return () => dialogListeners.delete(listener); }, () => dialogQueue, () => dialogQueue);
  useEffect(() => { hostCount += 1; return () => { hostCount -= 1; }; }, []);
  const current = queue[0];
  const answer = useCallback((ok: boolean) => {
    if (!current) return;
    dialogQueue = dialogQueue.filter((item) => item.id !== current.id);
    emitDialogs();
    current.resolve(ok);
  }, [current]);
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => { ref.current?.focus(); }, [current?.id]);
  useDismiss(Boolean(current), () => answer(false));
  if (!current) return null;
  return <div className="wc-layer wc-center">
    <div className="wc-backdrop" aria-hidden="true" />
    <div className="wc-dialog" role="alertdialog" aria-modal="true" aria-label={current.kind === "confirm" ? "请确认" : "提示"}>
      <p>{current.message}</p>
      <div className="wc-dialog-actions">
        {current.kind === "confirm" && <button type="button" onClick={() => answer(false)}>取消</button>}
        <button type="button" ref={ref} className="primary" onClick={() => answer(true)}>{current.kind === "confirm" ? "确定" : "知道了"}</button>
      </div>
    </div>
  </div>;
}
