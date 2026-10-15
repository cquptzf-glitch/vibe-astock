import { useEffect, useMemo, useState } from "react";
import {
  CircleDollarSign,
  ClipboardCheck,
  NotebookPen,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";

import "./journal.css";

type Mode = "record" | "manual";
type Page = "records" | "form";
type Planned = "" | "true" | "false";

interface FillDraft {
  type: "buy" | "sell";
  date: string;
  price: string;
  shares: string;
  fee: string;
}

interface JournalFill {
  id: number;
  type: "buy" | "sell";
  date: string;
  price: number;
  shares: number;
  fee: number;
  fee_estimated: boolean;
}

interface JournalRow {
  id: number;
  key: string;
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
  created_at: string;
  updated_at: string;
}

interface JournalView {
  available: boolean;
  source: string;
  playbooks: string[];
  fee_defaults: {
    commission_rate: number;
    commission_min: number;
    stamp_tax_rate: number;
    transfer_fee_rate: number;
    note: string;
  };
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
}

interface JournalActionResult {
  result: { id?: number; removed?: boolean };
  view: JournalView;
}

async function journalApi<T>(init?: RequestInit): Promise<T> {
  const response = await fetch("/api/mobile/journal", {
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
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(new Date());
  const value = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${value.year}-${value.month}-${value.day}`;
}

function emptyFill(type: "buy" | "sell", date = chinaToday()): FillDraft {
  return { type, date, price: "", shares: "", fee: "" };
}

function numberOrNull(value: string): number | null {
  const text = value.trim();
  if (!text) return null;
  const parsed = Number(text);
  return Number.isFinite(parsed) ? parsed : null;
}

function money(value: number | null): string {
  return value === null || !Number.isFinite(value)
    ? "—"
    : value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function pct(value: number | null, ratio = false): string {
  if (value === null || !Number.isFinite(value)) return "—";
  const actual = ratio ? value * 100 : value;
  return `${actual > 0 ? "+" : ""}${actual.toFixed(1)}%`;
}

export function JournalPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [page, setPage] = useState<Page>("records");
  const [mode, setMode] = useState<Mode>("record");
  const [data, setData] = useState<JournalView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

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

  const playbooks = data?.playbooks ?? ["打板", "低吸", "接力", "半路", "套利", "其它"];
  const openCount = useMemo(() => data?.rows.filter((row) => row.kind === "episode" && !row.closed).length ?? 0, [data]);

  async function load() {
    setLoading(true);
    setError("");
    try {
      setData(await journalApi<JournalView>());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
  }, []);

  function resetForm() {
    const today = chinaToday();
    setSymbol("");
    setName("");
    setDate(today);
    setPlaybook("");
    setPlanned("");
    setStop("");
    setTarget("");
    setNote("");
    setManualPnl("");
    setFills([emptyFill("buy", today)]);
  }

  async function submit() {
    const code = symbol.trim();
    setError("");
    setMessage("");
    if (!/^\d{6}$/.test(code)) {
      setError("代码要是 6 位数字，交易日志只记录 A 股。");
      return;
    }

    const common = {
      symbol: code,
      ...(name.trim() ? { name: name.trim() } : {}),
      ...(playbook ? { playbook } : {}),
      ...(planned ? { as_planned: planned === "true" } : {}),
      ...(note.trim() ? { note: note.trim() } : {}),
    };

    let body: Record<string, unknown>;
    if (mode === "manual") {
      const value = numberOrNull(manualPnl);
      if (manualPnl.trim() && value === null) {
        setError("盈亏% 要是数字，例如 3.2 或 -5。");
        return;
      }
      body = { action: "manual", ...common, date, ...(value !== null ? { pnl_pct: value } : {}) };
    } else {
      if (fills.length === 0) {
        setError("至少需要一笔成交。");
        return;
      }
      const payload = [];
      for (const fill of fills) {
        const price = numberOrNull(fill.price);
        const shares = numberOrNull(fill.shares);
        const fee = numberOrNull(fill.fee);
        if (price === null || price <= 0 || shares === null || shares <= 0) {
          setError("每笔成交都要填写大于 0 的成交价和股数。");
          return;
        }
        if (fill.fee.trim() && (fee === null || fee < 0)) {
          setError("费用要是大于等于 0 的数字；留空会按原项目默认费率估算。");
          return;
        }
        payload.push({ type: fill.type, date: fill.date, price, shares, ...(fee !== null ? { fee } : {}) });
      }
      const plannedStop = numberOrNull(stop);
      const plannedTarget = numberOrNull(target);
      if (stop.trim() && (plannedStop === null || plannedStop <= 0)) {
        setError("计划止损价要大于 0。");
        return;
      }
      if (target.trim() && (plannedTarget === null || plannedTarget <= 0)) {
        setError("计划目标价要大于 0。");
        return;
      }
      body = {
        action: "record",
        ...common,
        fills: payload,
        ...(plannedStop !== null ? { planned_stop: plannedStop } : {}),
        ...(plannedTarget !== null ? { planned_target: plannedTarget } : {}),
      };
    }

    setLoading(true);
    try {
      const result = await journalApi<JournalActionResult>({ method: "POST", body: JSON.stringify(body) });
      setData(result.view);
      setMessage(mode === "record" ? "成交已写入本地日志。" : "盈亏记录已补记。");
      resetForm();
      setPage("records");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function remove(id: number) {
    setLoading(true);
    setError("");
    try {
      const result = await journalApi<JournalActionResult>({
        method: "POST",
        body: JSON.stringify({ action: "delete", id }),
      });
      setData(result.view);
      setMessage("记录已删除。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="journal-panel">
      <div className="journal-title-row">
        <div>
          <div className="journal-kicker"><NotebookPen size={15} /> 本地交易日志</div>
          <h1>交易记录 · 复盘素材</h1>
          <p>个人交易只保存在手机 SQLite；不自动发送给 AI。</p>
        </div>
        <button className="journal-refresh" onClick={() => void load()} disabled={loading} aria-label="刷新交易日志">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="journal-page-tabs">
        <button className={page === "records" ? "active" : ""} onClick={() => setPage("records")}>记录</button>
        <button className={page === "form" ? "active" : ""} onClick={() => setPage("form")}><Plus size={14} />记一笔</button>
      </div>

      {error && <div className="journal-error">{error}</div>}
      {message && <div className="journal-message">{message}</div>}

      {page === "records" && (
        <>
          <div className="journal-summary">
            <div><span>记录</span><strong>{data?.summary.entries ?? 0}</strong><small>笔</small></div>
            <div><span>已结算</span><strong>{data?.summary.closed ?? 0}</strong><small>笔</small></div>
            <div><span>胜率</span><strong>{pct(data?.summary.win_rate ?? null, true)}</strong><small>{data?.summary.scored ?? 0} 笔有结果</small></div>
            <div><span>按计划</span><strong>{pct(data?.summary.execution_rate ?? null, true)}</strong><small>{data?.summary.planned_known ?? 0} 笔已标注</small></div>
          </div>

          <div className="journal-money-line">
            <CircleDollarSign size={15} />
            <span>已结算净盈亏</span>
            <strong className={(data?.summary.net_pnl ?? 0) >= 0 ? "up" : "down"}>{money(data?.summary.net_pnl ?? null)} 元</strong>
            <small>{data?.summary.money_scored ?? 0} 笔可计算 · 当前未平仓 {openCount} 笔</small>
          </div>

          {!data && loading && <div className="journal-empty"><RefreshCw size={18} className="spin" /> 正在读取本地日志…</div>}
          {data && data.rows.length === 0 && (
            <button className="journal-empty add" onClick={() => setPage("form")}>
              <NotebookPen size={25} />
              <strong>还没有交易记录</strong>
              <span>从第一笔成交或一条盈亏补记开始。</span>
            </button>
          )}

          <div className="journal-list">
            {data?.rows.map((row) => (
              <article className="journal-row" key={row.id}>
                <div className="journal-row-head">
                  <button onClick={() => onOpenSymbol(row.symbol)} className="journal-symbol">
                    <strong>{row.name || row.symbol}</strong>
                    <span>{row.symbol} · {row.date}</span>
                  </button>
                  <div className="journal-result">
                    <strong className={(row.pnl_pct ?? 0) > 0 ? "up" : (row.pnl_pct ?? 0) < 0 ? "down" : ""}>{pct(row.pnl_pct)}</strong>
                    <span>{row.kind === "manual" ? "盈亏补记" : row.closed ? `${money(row.realized_pnl)} 元` : `持有 ${row.open_shares.toLocaleString("zh-CN")} 股`}</span>
                  </div>
                </div>

                <div className="journal-tags">
                  {row.playbook && <span>{row.playbook}</span>}
                  {row.as_planned === true && <span className="planned"><ClipboardCheck size={10} />按计划</span>}
                  {row.as_planned === false && <span className="unplanned">计划外</span>}
                  {row.planned_stop !== null && <span>止损 {row.planned_stop}</span>}
                  {row.planned_target !== null && <span>目标 {row.planned_target}</span>}
                </div>

                {row.fills.length > 0 && (
                  <div className="journal-fills">
                    {row.fills.map((fill) => (
                      <div key={fill.id}>
                        <b className={fill.type === "buy" ? "buy" : "sell"}>{fill.type === "buy" ? "买" : "卖"}</b>
                        <span>{fill.date}</span>
                        <strong>{fill.price.toLocaleString("zh-CN", { maximumFractionDigits: 3 })} × {fill.shares.toLocaleString("zh-CN")}</strong>
                        <small>费 {money(fill.fee)}{fill.fee_estimated ? " 估" : ""}</small>
                      </div>
                    ))}
                  </div>
                )}

                {row.note && <p className="journal-note">{row.note}</p>}

                <div className="journal-row-foot">
                  <span>{row.fees_are_estimated ? "含默认费率估算" : row.fees !== null ? `费用 ${money(row.fees)} 元` : ""}</span>
                  <button onClick={() => void remove(row.id)} disabled={loading} aria-label={`删除 ${row.symbol} 交易记录`}><Trash2 size={15} />删除</button>
                </div>
              </article>
            ))}
          </div>

          {data?.fee_defaults && (
            <p className="journal-fee-note">费用留空时：佣金万 2.5（最低 5 元）+ 原项目默认印花税/过户费估算。该默认值仅用于日志计算，不代表你的实际券商费率。</p>
          )}
        </>
      )}

      {page === "form" && (
        <div className="journal-form">
          <div className="journal-mode-tabs">
            <button className={mode === "record" ? "active" : ""} onClick={() => setMode("record")}>有成交明细</button>
            <button className={mode === "manual" ? "active" : ""} onClick={() => setMode("manual")}>只记盈亏%</button>
          </div>

          <div className="journal-form-grid two">
            <label>股票代码<input inputMode="numeric" value={symbol} onChange={(e) => setSymbol(e.target.value)} placeholder="如 300308" maxLength={6} /></label>
            <label>简称（可选）<input value={name} onChange={(e) => setName(e.target.value)} placeholder="如 中际旭创" /></label>
          </div>

          <div className="journal-form-grid two">
            <label>打法<select value={playbook} onChange={(e) => setPlaybook(e.target.value)}><option value="">不标注</option>{playbooks.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
            <label>执行<select value={planned} onChange={(e) => setPlanned(e.target.value as Planned)}><option value="">未标注</option><option value="true">按计划</option><option value="false">计划外</option></select></label>
          </div>

          {mode === "manual" ? (
            <div className="journal-form-grid two">
              <label>日期<input type="date" value={date} onChange={(e) => setDate(e.target.value)} /></label>
              <label>盈亏 %<input inputMode="decimal" value={manualPnl} onChange={(e) => setManualPnl(e.target.value)} placeholder="3.2 / -5" /></label>
            </div>
          ) : (
            <>
              <div className="journal-plan-row">
                <label>计划止损<input inputMode="decimal" value={stop} onChange={(e) => setStop(e.target.value)} placeholder="可选" /></label>
                <label>计划目标<input inputMode="decimal" value={target} onChange={(e) => setTarget(e.target.value)} placeholder="可选" /></label>
              </div>

              <div className="journal-fills-editor">
                <div className="journal-editor-title"><strong>成交明细</strong><span>费用留空则按默认费率估算</span></div>
                {fills.map((fill, index) => (
                  <div className="journal-fill-editor" key={index}>
                    <select value={fill.type} onChange={(e) => setFills((current) => current.map((item, i) => i === index ? { ...item, type: e.target.value as "buy" | "sell" } : item))}>
                      <option value="buy">买入</option><option value="sell">卖出</option>
                    </select>
                    <input type="date" value={fill.date} onChange={(e) => setFills((current) => current.map((item, i) => i === index ? { ...item, date: e.target.value } : item))} />
                    <input inputMode="decimal" value={fill.price} onChange={(e) => setFills((current) => current.map((item, i) => i === index ? { ...item, price: e.target.value } : item))} placeholder="成交价" />
                    <input inputMode="numeric" value={fill.shares} onChange={(e) => setFills((current) => current.map((item, i) => i === index ? { ...item, shares: e.target.value } : item))} placeholder="股数" />
                    <input inputMode="decimal" value={fill.fee} onChange={(e) => setFills((current) => current.map((item, i) => i === index ? { ...item, fee: e.target.value } : item))} placeholder="费用(可空)" />
                    <button onClick={() => setFills((current) => current.filter((_, i) => i !== index))} aria-label={`删除第 ${index + 1} 笔成交`}><X size={15} /></button>
                  </div>
                ))}
                <div className="journal-add-fill">
                  <button onClick={() => setFills((current) => [...current, emptyFill("buy")])}><Plus size={13} />买入</button>
                  <button onClick={() => setFills((current) => [...current, emptyFill("sell")])}><Plus size={13} />卖出</button>
                </div>
              </div>
            </>
          )}

          <label className="journal-note-input">复盘备注<textarea value={note} onChange={(e) => setNote(e.target.value)} placeholder="当时为什么做这笔？预期是什么？执行哪里偏离？" rows={3} /></label>

          <button className="journal-submit" onClick={() => void submit()} disabled={loading}>
            {loading ? <RefreshCw size={16} className="spin" /> : <NotebookPen size={16} />}
            {mode === "record" ? "保存成交记录" : "保存盈亏补记"}
          </button>
        </div>
      )}
    </section>
  );
}
