/**
 * 我的自选(移动版):报价 + 市盈率/市净率/换手率,按分组筛选、按任意一列排序、自定义顺序上移下移、给每只写分组和标签。
 * 分组、标签、顺序是用户自己写的,存在本机 SQLite、进备份;市盈率为负(亏损)显示「亏损」且不参与排序比较。
 * 只列已有读数,不给买卖建议。
 */
import { ArrowDown, ArrowUp, ArrowDownUp, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { askConfirm, WebSelect } from "./controls";
import { fmtNumber, fmtPercent, fmtYuan } from "./discoveryData";
import { usePresentation } from "./presentation";
import { useForegroundRefresh } from "./useForegroundRefresh";
import {
  filterByGroup, groupsOf, moveVisible, NO_GROUP, parseTags, pbText, peText, SORT_LABELS, sortRows,
  type BoardRow, type SortDir, type SortKey,
} from "./watchlistView";
import "./watchlist.css";

interface BoardData { watchlist: BoardRow[]; positions: { symbol: string }[]; fetched_at: string; missing: string[] }
const SORT_KEY = "vibe.watch.sort";
const SORT_KEYS = Object.keys(SORT_LABELS) as SortKey[];

function readSort(): { key: SortKey; dir: SortDir } {
  try {
    const saved = JSON.parse(window.localStorage.getItem(SORT_KEY) ?? "null") as { key?: string; dir?: string } | null;
    if (saved && SORT_KEYS.includes(saved.key as SortKey)) return { key: saved.key as SortKey, dir: saved.dir === "asc" ? "asc" : "desc" };
  } catch { /* 存储不可用就用默认 */ }
  return { key: "custom", dir: "desc" };
}

async function call(method: string, path: string, body?: unknown): Promise<unknown> {
  const response = await fetch("/api" + path, { method, headers: { Accept: "application/json", "Content-Type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await response.json().catch(() => null) as { message?: string; error?: string } | null;
  if (!response.ok) throw new Error(data?.message ?? data?.error ?? `HTTP ${response.status}`);
  return data;
}

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");

export function WatchRowView({ row, held, canMove, onOpen, onMove, onEdit, first, last }: {
  row: BoardRow; held: boolean; canMove: boolean; first: boolean; last: boolean;
  onOpen: () => void; onMove: (delta: -1 | 1) => void; onEdit: () => void;
}) {
  const sub = [row.symbol, !row.available ? "暂无报价" : row.is_today === false ? row.quote_date ?? "时间未知" : null, row.group || null].filter(Boolean).join(" · ");
  return <div className="wl-row">
    <button type="button" className="wl-main" onClick={onOpen}>
      <span className="wl-name"><span className="wl-title"><strong>{row.name || row.symbol}</strong>{held && <em>持仓</em>}</span><small>{sub}</small></span>
      <span className="wl-price"><strong>{fmtNumber(row.price)}</strong><b className={tone(row.change_pct)}>{fmtPercent(row.change_pct)}</b></span>
    </button>
    <div className="wl-meta">
      <span>PE <b>{peText(row.pe_ttm)}</b></span><span>PB <b>{pbText(row.pb)}</b></span>
      <span>换手 <b>{row.turnover_pct == null ? "—" : fmtNumber(row.turnover_pct) + "%"}</b></span><span>额 <b>{fmtYuan(row.amount_yuan)}</b></span>
      {(row.tags ?? []).map((tag) => <i key={tag}>{tag}</i>)}
    </div>
    <div className="wl-tools">
      {canMove && <><button type="button" aria-label={`上移 ${row.name || row.symbol}`} disabled={first} onClick={() => onMove(-1)}><ArrowUp size={14} /></button>
        <button type="button" aria-label={`下移 ${row.name || row.symbol}`} disabled={last} onClick={() => onMove(1)}><ArrowDown size={14} /></button></>}
      <button type="button" aria-label={`编辑 ${row.name || row.symbol}`} onClick={onEdit}><Pencil size={14} /></button>
    </div>
  </div>;
}

export function MetaEditor({ row, groups, busy, message, onSave, onRemove, onCancel }: {
  row: BoardRow; groups: string[]; busy: boolean; message: string;
  onSave: (group: string, tags: string[]) => void; onRemove: () => void; onCancel: () => void;
}) {
  const [group, setGroup] = useState(row.group ?? "");
  const [tags, setTags] = useState((row.tags ?? []).join(" "));
  return <div className="wl-editor" aria-label={`编辑 ${row.name || row.symbol}`}>
    <label><span>分组(最多 12 字)</span><input value={group} onChange={(e) => setGroup(e.target.value)} aria-label="分组" /></label>
    {groups.length > 0 && <div className="wl-chips" aria-label="已有分组">{groups.map((g) => <button type="button" key={g} className={g === group ? "on" : ""} onClick={() => setGroup(g === group ? "" : g)}>{g}</button>)}</div>}
    <label><span>标签(空格或逗号分隔,最多 5 个、每个 8 字)</span><input value={tags} onChange={(e) => setTags(e.target.value)} aria-label="标签" /></label>
    <div className="wl-actions">
      <button type="button" className="primary" disabled={busy} onClick={() => onSave(group, parseTags(tags))}>保存</button>
      <button type="button" disabled={busy} onClick={onCancel}>取消</button>
      <button type="button" className="danger" disabled={busy} onClick={onRemove}><Trash2 size={13} /> 移出自选</button>
    </div>
    {message && <p className="intra-warn" role="alert">{message}</p>}
  </div>;
}

export function WatchlistBoard({ onOpenSymbol, onChanged }: { onOpenSymbol: (symbol: string) => void; onChanged: () => void }) {
  const { refreshSeconds } = usePresentation();
  const [data, setData] = useState<BoardData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [group, setGroup] = useState("");
  const [sort, setSort] = useState(readSort);
  const [editing, setEditing] = useState<string | null>(null);
  const [adding, setAdding] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const request = useRef(0);

  const refresh = useCallback(async () => {
    const id = ++request.current;
    setLoading(true);
    setError("");
    try {
      const body = await call("GET", "/mobile/monitor") as BoardData;
      if (request.current === id) setData(body);
    } catch (e) {
      if (request.current === id) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (request.current === id) setLoading(false);
    }
  }, []);
  useEffect(() => { void refresh(); return () => { request.current++; }; }, [refresh]);
  useForegroundRefresh(refresh, refreshSeconds);
  useEffect(() => { try { window.localStorage.setItem(SORT_KEY, JSON.stringify(sort)); } catch { /* 可选 */ } }, [sort]);

  const rows = data?.watchlist ?? [];
  const groups = useMemo(() => groupsOf(rows), [rows]);
  const hasUngrouped = groups.length > 0 && rows.some((row) => !row.group);
  const visible = useMemo(() => sortRows(filterByGroup(rows, group), sort.key, sort.dir), [rows, group, sort]);
  const held = useMemo(() => new Set((data?.positions ?? []).map((p) => p.symbol)), [data]);
  const canMove = sort.key === "custom";

  async function mutate(run: () => Promise<unknown>) {
    setBusy(true);
    setMessage("");
    try {
      await run();
      await refresh();
      onChanged();
      return true;
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(false);
    }
  }
  const move = (symbol: string, delta: -1 | 1) => void mutate(() => call("POST", "/mobile/watchlist/order", {
    symbols: moveVisible(rows.map((r) => r.symbol), visible.map((r) => r.symbol), symbol, delta),
  }));
  const addCodes = async () => {
    const codes = adding.split(/[\s,，、]+/).filter(Boolean);
    if (!codes.length) return;
    const ok = await mutate(async () => { for (const symbol of codes) await call("POST", "/mobile/watchlist", { symbol }); });
    if (ok) setAdding("");
  };
  const editingRow = rows.find((row) => row.symbol === editing);

  return <section className="wl-board" aria-label="我的自选">
    <div className="wl-toolbar">
      <WebSelect aria-label="排序" value={sort.key} onChange={(e) => setSort({ key: e.target.value as SortKey, dir: sort.dir })}>
        {SORT_KEYS.map((key) => <option key={key} value={key}>{key === "custom" ? SORT_LABELS[key] : "按" + SORT_LABELS[key]}</option>)}
      </WebSelect>
      <button type="button" className="wl-dir" aria-label={sort.dir === "desc" ? "当前从高到低,点一下改为从低到高" : "当前从低到高,点一下改为从高到低"} disabled={sort.key === "custom"}
        onClick={() => setSort({ key: sort.key, dir: sort.dir === "desc" ? "asc" : "desc" })}><ArrowDownUp size={14} />{sort.key === "name" ? (sort.dir === "asc" ? "A→Z" : "Z→A") : sort.dir === "desc" ? "高→低" : "低→高"}</button>
      <button type="button" className="wl-refresh" aria-label="刷新自选行情" disabled={loading} onClick={() => void refresh()}><RefreshCw size={14} className={loading ? "spin" : ""} /></button>
    </div>
    {groups.length > 0 && <div className="wl-chips" role="tablist" aria-label="分组">
      <button type="button" role="tab" aria-selected={group === ""} className={group === "" ? "on" : ""} onClick={() => setGroup("")}>全部 {rows.length}</button>
      {groups.map((g) => <button type="button" role="tab" key={g} aria-selected={group === g} className={group === g ? "on" : ""} onClick={() => setGroup(g)}>{g} {rows.filter((r) => r.group === g).length}</button>)}
      {hasUngrouped && <button type="button" role="tab" aria-selected={group === NO_GROUP} className={group === NO_GROUP ? "on" : ""} onClick={() => setGroup(NO_GROUP)}>{NO_GROUP} {rows.filter((r) => !r.group).length}</button>}
    </div>}
    {error && <p className="home-inline-error">{error}{data ? ",保留上次快照。" : "。"}</p>}
    {!data && loading && <p className="wl-note">正在获取自选与报价…</p>}
    {data && rows.length === 0 && <div className="mobile-empty">还没有自选。在下面输入代码,或在个股页点右上角星标。</div>}
    <div className="wl-list">{visible.map((row, index) => <div key={row.symbol}>
      <WatchRowView row={row} held={held.has(row.symbol)} canMove={canMove} first={index === 0} last={index === visible.length - 1}
        onOpen={() => onOpenSymbol(row.symbol)} onMove={(delta) => move(row.symbol, delta)} onEdit={() => { setMessage(""); setEditing(editing === row.symbol ? null : row.symbol); }} />
      {editingRow && editing === row.symbol && <MetaEditor key={row.symbol} row={row} groups={groups} busy={busy} message={message} onCancel={() => setEditing(null)}
        onSave={(g, t) => void mutate(() => call("POST", "/mobile/watchlist/meta", { symbol: row.symbol, group: g, tags: t })).then((ok) => { if (ok) setEditing(null); })}
        onRemove={async () => {
          if (!(await askConfirm(`把 ${row.name || row.symbol} 移出自选?分组、标签随之删除。`))) return;
          const ok = await mutate(() => call("DELETE", "/mobile/watchlist", { symbol: row.symbol }));
          if (ok) setEditing(null);
        }} />}
    </div>)}</div>
    {canMove && group !== "" && visible.length > 1 && <p className="wl-note">在分组「{group}」内调整先后,不影响其它分组里的相对顺序。</p>}
    <div className="wl-add">
      <input value={adding} onChange={(e) => setAdding(e.target.value)} placeholder="输入代码添加,如 600519 000001" aria-label="添加自选代码" onKeyDown={(e) => { if (e.key === "Enter") void addCodes(); }} />
      <button type="button" disabled={busy || !adding.trim()} onClick={() => void addCodes()}><Plus size={14} /> 添加</button>
    </div>
    {!editing && message && <p className="intra-warn" role="alert">{message}</p>}
    {data && <p className="wl-note">{new Date(data.fetched_at).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })} 更新 · 市盈率为 TTM,亏损显示「亏损」且排序时和没有数据一起排最后{data.missing.length > 0 ? ` · ${data.missing.length} 只没取到报价` : ""} · 只列已有读数,不构成买卖建议。</p>}
  </section>;
}
