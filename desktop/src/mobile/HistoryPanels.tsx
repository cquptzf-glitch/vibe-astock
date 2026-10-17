import { useEffect, useState, type ReactNode } from "react";
import { CalendarDays, Plus, RefreshCw, RotateCcw, Save, Trash2 } from "lucide-react";
import { fmtPercent } from "./discoveryData";
import { chinaToday, clearEmotionCache, fmtRate, useCachedResource, useEmotionDate } from "./emotionData";
import "./emotion.css";

type Open = (symbol: string) => void;
type Kind = "count" | "rate" | "pct";
type Values = Record<string, number | null>;
interface MetricDef { key: string; label: string; unit: string; kind: Kind; hint: string; eps: number; diff_eps: number; higher_is_hotter: boolean; history: boolean }
interface DayRow { date: string; values: Values; status: "ok" | "partial" | "missing"; settled: boolean | null; note: string; cached: boolean }
type Maybe<T> = (T & { available: true }) | { available: false; reason: string };

export interface HistoryData {
  status: string; date: string; days: number; readings: MetricDef[]; series: DayRow[]; gaps: string[]; source: string;
  context: Maybe<{ sample_days: number; min_samples: number; extreme_count: number; items: {
    key: string; label: string; unit: string; kind: Kind; value: number | null; percentile: number | null;
    extreme: "偏热" | "偏冷" | null; hist_min: number | null; hist_max: number | null; hist_median: number | null }[] }>;
  diff: Maybe<{ prev_date: string; unchanged_count: number; changes: { key: string; label: string; kind: Kind; prev: number; cur: number; delta: number; hotter: boolean }[] }>;
}
interface Leader { code: string; name: string; boards: number; sector: string; first_seal: string }
export interface WeeklyData {
  status: string; date: string; gaps: string[]; note: string;
  days: { date: string; limit_up: number | null; highest_board: number | null; broken_rate: number | null; money_effect: number | null;
    promotion_1to2: number | null; leaders: Leader[]; leader_total: number; settled: boolean | null; status: string; note: string }[];
  rotation: { dates: string[]; missing_days: string[]; themes: { sector: string; series: (number | null)[]; ranks: (number | null)[];
    today: number; total: number; active_days: number; top3_days: number; streak: number; streak_partial: boolean; status: string }[] };
  lineage: { code: string; name: string; sector: string; appear_date: string; boards_then: number; is_current_top: boolean;
    cum_return_since?: number | null; peak_cum_ret?: number | null; drawdown_from_peak?: number | null;
    series: { date: string; cum_ret: number }[]; series_warning: string }[];
}
type Direction = "up" | "down" | "flat";
interface ResultRow {
  id: number; metric: string; label: string; kind: Kind; unit: string; direction: Direction; direction_label: string; reason: string;
  prev_value: number | null; cur_value: number | null; actual: Direction | null; actual_label: string | null;
  verified: boolean | null; late: boolean; baseline: number | null; baseline_note: string; baseline_reason: string | null; edge: number | null;
}
interface Summary { total: number; decided: number; hit: number; rate: number | null; undecidable: number; baseline_covered: number; expected_rate: number | null; edge: number | null }
export interface VerifyData {
  date: string; metrics: MetricDef[]; directions: { key: Direction; label: string }[]; gaps: string[]; note: string;
  check: Maybe<{ set_date: string; date: string; provisional: boolean; results: ResultRow[]; summary: Summary }>;
  plan: { date: string; next_date: string | null; locked: boolean; lock_note: string;
    items: { id: number; metric: string; label: string; kind: Kind; direction: Direction; reason: string; base_value: number | null; baseline: number | null; baseline_note: string; baseline_reason: string | null }[] };
  track: { from: string | null; to: string; summary: Summary; excluded_late: number; excluded_provisional: number;
    days: { set_date: string; date: string; total: number; decided: number; hit: number; excluded: number }[] };
}

export function fmtValue(value: number | null | undefined, kind: Kind, unit = ""): string {
  if (value == null || !Number.isFinite(value)) return "—";
  if (kind === "rate") return fmtRate(value);
  if (kind === "pct") return fmtPercent(value);
  return (Number.isInteger(value) ? String(value) : value.toFixed(1)) + (unit ? " " + unit : "");
}
function Card({ title, extra, children }: { title: string; extra?: ReactNode; children: ReactNode }) {
  return <section className="emo-card"><div className="emo-card-head"><h2>{title}</h2>{extra}</div>{children}</section>;
}
function Reason({ reason }: { reason: string }) {
  return <div className="mobile-empty">暂不可用：{reason}</div>;
}
function Gaps({ gaps }: { gaps: string[] }) {
  return gaps.length ? <div className="discovery-notice">{gaps.join("；")}</div> : null;
}

export function ReviewDateBar({ title, detail, loading, error, onRefresh, shownDate }: { title: string; detail: string; loading: boolean; error: string; onRefresh: () => void; shownDate?: string }) {
  const [date, setDate] = useEmotionDate();
  const today = chinaToday();
  return <div className="emo-header">
    <div className="emo-title"><div><h1>{title}</h1><p>{detail}</p></div>
      <button type="button" className="mobile-header-action" disabled={loading} onClick={onRefresh} aria-label="刷新"><RefreshCw size={16} className={loading ? "spin" : ""} />刷新</button></div>
    <div className="emo-date-row">
      <label><CalendarDays size={15} /><input type="date" aria-label="复盘日" max={today} value={date || shownDate || ""} onChange={(e) => setDate(e.target.value)} /></label>
      <button type="button" className={!date ? "active" : ""} onClick={() => setDate("")}><RotateCcw size={14} />最近收盘</button>
      <button type="button" className={date === today ? "active" : ""} onClick={() => setDate(today)}>今日盘中</button>
    </div>
    {loading && !shownDate && <div className="mobile-empty"><RefreshCw size={16} className="spin" /> 第一次打开要逐日拉取东财池子，之后已收盘的日子读本机缓存…</div>}
    {error && <div className="mobile-error" role="alert">{error}<button className="discovery-action" onClick={onRefresh}>重试</button></div>}
  </div>;
}

// ------------------------------------------------------------ 情绪历史

export function HistoryView({ data }: { data: HistoryData }) {
  const defs = data.readings;
  const rows = [...data.series].reverse();
  const blocks: ({ gap: DayRow[] } | { row: DayRow })[] = [];
  for (const row of rows) {
    const last = blocks[blocks.length - 1];
    if (row.status === "missing" && last && "gap" in last) last.gap.push(row);
    else blocks.push(row.status === "missing" ? { gap: [row] } : { row });
  }
  return <div className="emo-body">
    <Gaps gaps={data.gaps} />
    <Card title="历史统计位置" extra={data.context.available ? <small>对比此前 {data.context.sample_days} 个有数的交易日</small> : undefined}>
      {data.context.available ? <div className="hist-list">{data.context.items.map((item) => <div className="hist-item" key={item.key}>
        <div><strong>{item.label}</strong><span>{fmtValue(item.value, item.kind, item.unit)}</span>{item.extreme && <em className={item.extreme === "偏热" ? "hot" : "cold"}>{item.extreme}</em>}</div>
        {item.percentile == null ? <small>样本不足 {data.context.available ? data.context.min_samples : 10} 天，不给分位</small>
          : <><i className="emo-bar"><b style={{ width: `${item.percentile * 100}%` }} /></i><small>分位 {fmtRate(item.percentile)} · 历史中位 {fmtValue(item.hist_median, item.kind, item.unit)} · 区间 {fmtValue(item.hist_min, item.kind)}~{fmtValue(item.hist_max, item.kind)}</small></>}
      </div>)}
        <p className="emo-foot">{data.context.extreme_count ? `${data.context.extreme_count} 项处在历史两头（≥85% 或 ≤15% 分位）` : "六项都不在历史两头，是平淡的一天"}。分位只和之前比，不含当天。</p>
      </div> : <Reason reason={data.context.reason} />}
    </Card>
    <Card title="复盘日 vs 前一交易日" extra={data.diff.available ? <small>{data.diff.prev_date} → {data.date}</small> : undefined}>
      {data.diff.available ? <>
        {data.diff.changes.length ? <div className="emo-detail-list">{data.diff.changes.map((c) => <div key={c.key} className="hist-change">
          <span>{c.label}<small>{fmtValue(c.prev, c.kind)} → {fmtValue(c.cur, c.kind)}</small></span>
          <b>{c.kind === "rate" ? (c.delta > 0 ? "+" : "") + Math.round(c.delta * 1000) / 10 + " 个点" : (c.delta > 0 ? "+" : "") + c.delta}</b>
          <em className={c.hotter ? "hot" : "cold"}>{c.hotter ? "转热" : "转冷"}</em></div>)}</div>
          : <p className="emo-foot">六项变化都在噪声门槛以内。</p>}
        <p className="emo-foot">{data.diff.unchanged_count} 项变化不超过噪声门槛，没列出。</p>
      </> : <Reason reason={data.diff.reason} />}
    </Card>
    <Card title="逐日读数" extra={<small>新的在上</small>}>
      <div className="hist-table" role="table" aria-label="逐日情绪读数">
        <div role="row" className="head"><span role="columnheader">日期</span>{defs.map((d) => <span role="columnheader" key={d.key}>{d.label.replace("赚钱效应中位数", "赚钱中位").replace(" 晋级率", "")}</span>)}</div>
        {blocks.map((b, i) => "row" in b
          ? <div role="row" key={b.row.date} className={b.row.status === "partial" ? "partial" : ""}>
              <span role="rowheader">{b.row.date.slice(5)}{b.row.settled === false && <small>未收盘</small>}</span>
              {defs.map((d) => <span role="cell" key={d.key}>{fmtValue(b.row.values[d.key], d.kind)}</span>)}</div>
          : <div role="row" key={"gap" + i} className="gap"><span role="cell">{b.gap.length === 1 ? b.gap[0]!.date : `${b.gap[b.gap.length - 1]!.date} ~ ${b.gap[0]!.date}`} 共 {b.gap.length} 天没取到：{b.gap[0]!.note}</span></div>)}
      </div>
    </Card>
    <p className="emo-foot">{data.source}。东财池子只留约一个月；已收盘且取全的日子存在本机，历史会越用越长。</p>
  </div>;
}

export function HistoryPanel() {
  const [date] = useEmotionDate();
  const [days, setDays] = useState(30);
  const result = useCachedResource<HistoryData>(`/mobile/shortline/history?days=${days}${date ? "&date=" + date : ""}`);
  return <section className="mobile-section emo-page">
    <ReviewDateBar title="情绪历史" detail="六项读数在近 N 个交易日里的位置与变化" loading={result.loading} error={result.error} onRefresh={result.refresh} shownDate={result.data?.date} />
    <div className="mobile-stock-tabs" role="tablist" aria-label="统计窗口">{[10, 20, 30, 60].map((n) =>
      <button key={n} type="button" role="tab" aria-selected={days === n} className={days === n ? "active" : ""} onClick={() => setDays(n)}>{n} 天</button>)}</div>
    {result.data && <HistoryView data={result.data} />}
  </section>;
}

// ------------------------------------------------------------ 近 5 天

function MiniLine({ points }: { points: { cum_ret: number }[] }) {
  if (points.length < 2) return null;
  const vals = points.map((p) => p.cum_ret);
  const min = Math.min(0, ...vals), max = Math.max(0, ...vals), span = max - min || 1;
  const d = vals.map((v, i) => `${(i / (vals.length - 1)) * 100},${30 - ((v - min) / span) * 28}`).join(" ");
  return <svg className="hist-mini" viewBox="0 0 100 32" preserveAspectRatio="none" aria-hidden="true"><polyline points={d} fill="none" vectorEffect="non-scaling-stroke" /></svg>;
}

export function WeeklyView({ data, onOpenSymbol }: { data: WeeklyData; onOpenSymbol: Open }) {
  const max = Math.max(1, ...data.days.map((d) => d.limit_up ?? 0));
  const rot = data.rotation;
  return <div className="emo-body">
    <Gaps gaps={data.gaps} />
    <Card title="近 5 天情绪热度">
      <div className="hist-week">{data.days.map((d) => <div key={d.date} className={d.status === "missing" ? "missing" : ""}>
        <strong>{d.date.slice(5)}{d.settled === false && <small>未收盘</small>}</strong>
        {d.status === "missing" ? <span className="emo-foot">{d.note}</span> : <>
          <span className="hist-heat"><i className="emo-bar"><b style={{ width: `${((d.limit_up ?? 0) / max) * 100}%` }} /></i><em>{d.limit_up ?? "—"}</em></span>
          <small>最高 {d.highest_board ?? "—"} 板 · 炸板率 {fmtRate(d.broken_rate)} · 1进2 {fmtRate(d.promotion_1to2)}</small>
          <small>最高标：{d.leaders.length ? d.leaders.map((l) => l.name).join("、") + (d.leader_total > d.leaders.length ? ` 等 ${d.leader_total} 只` : "") : "—"}</small>
        </>}
      </div>)}</div>
    </Card>
    <Card title="行业轮动" extra={<small>每格为当天涨停家数，前三名高亮</small>}>
      <div className="hist-matrix" role="table" aria-label="行业轮动矩阵">
        <div role="row" className="head"><span role="columnheader">行业</span>{rot.dates.map((d) => <span role="columnheader" key={d}>{d.slice(5)}</span>)}<span role="columnheader">状态</span></div>
        {rot.themes.map((t) => <div role="row" key={t.sector}><span role="rowheader">{t.sector}</span>
          {t.series.map((v, i) => <span role="cell" key={i} className={v == null ? "na" : t.ranks[i] != null && t.ranks[i]! <= 3 ? "top" : v === 0 ? "zero" : ""}>{v == null ? "—" : v}</span>)}
          <span role="cell" className="status">{t.status}</span></div>)}
      </div>
      {rot.missing_days.length > 0 && <p className="emo-foot">{rot.missing_days.join("、")} 没取到，显示为 —（不是 0），连续天数在这里中断。</p>}
      <p className="emo-foot">{data.note}</p>
    </Card>
    <Card title="龙头谱系" extra={<small>每天最高标后来走得怎么样</small>}>
      <div className="hist-lineage">{data.lineage.map((l) => <button type="button" key={l.code} onClick={() => onOpenSymbol(l.code)}>
        <div><strong>{l.name}</strong>{l.is_current_top && <em>当前最高标</em>}<small>{l.sector} · {l.appear_date.slice(5)} 登顶 {l.boards_then} 板</small></div>
        {l.series_warning ? <small className="emo-warn">{l.series_warning}</small> : <>
          <MiniLine points={l.series} />
          <dl><div><dt>自登顶</dt><dd className={l.cum_return_since == null ? "" : l.cum_return_since >= 0 ? "up" : "down"}>{fmtPercent(l.cum_return_since)}</dd></div>
            <div><dt>最高到过</dt><dd>{fmtPercent(l.peak_cum_ret)}</dd></div><div><dt>距高点</dt><dd className="down">{fmtPercent(l.drawdown_from_peak)}</dd></div></dl>
        </>}
      </button>)}</div>
      {!data.lineage.length && <div className="mobile-empty">没有取到最高标。</div>}
    </Card>
  </div>;
}

export function WeeklyPanel({ onOpenSymbol }: { onOpenSymbol: Open }) {
  const [date] = useEmotionDate();
  const result = useCachedResource<WeeklyData>("/mobile/shortline/weekly" + (date ? "?date=" + date : ""));
  return <section className="mobile-section emo-page">
    <ReviewDateBar title="近 5 天" detail="热度、行业轮动与龙头谱系" loading={result.loading} error={result.error} onRefresh={result.refresh} shownDate={result.data?.date} />
    {result.data && <WeeklyView data={result.data} onOpenSymbol={onOpenSymbol} />}
  </section>;
}

// ------------------------------------------------------------ 明日验证

interface Draft { metric: string; direction: Direction; reason: string }

function verdict(r: ResultRow) {
  if (r.verified == null) return <em className="na">判不了</em>;
  return <em className={r.verified ? "hit" : "miss"}>{r.verified ? "成立" : "不成立"}</em>;
}
function SummaryLine({ s }: { s: Summary }) {
  return <p className="emo-foot">判定 {s.decided}/{s.total} 条，成立 {s.hit} 条{s.rate != null && `（${fmtRate(s.rate)}）`}
    {s.expected_rate != null && `；按历史常态随口猜的命中率 ${fmtRate(s.expected_rate)}，超额 ${s.edge != null && s.edge > 0 ? "+" : ""}${fmtRate(s.edge)}`}{s.undecidable ? `；${s.undecidable} 条数据取不到，不算判错` : ""}。</p>;
}

export function VerifyView({ data, onSave }: { data: VerifyData; onSave: (items: Draft[]) => Promise<void> }) {
  const initial = (): Draft[] => data.plan.items.map((i) => ({ metric: i.metric, direction: i.direction, reason: i.reason }));
  const [draft, setDraft] = useState<Draft[]>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => { setDraft(initial()); setError(""); }, [data]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(initial());
  const used = new Set(draft.map((d) => d.metric));
  const free = data.metrics.filter((m) => !used.has(m.key));
  const plan = new Map(data.plan.items.map((i) => [i.metric, i]));
  const check = data.check;
  const update = (i: number, patch: Partial<Draft>) => setDraft(draft.map((d, j) => (j === i ? { ...d, ...patch } : d)));
  async function save() {
    setSaving(true); setError("");
    try { await onSave(draft); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setSaving(false); }
  }
  return <div className="emo-body">
    <Gaps gaps={data.gaps} />
    <Card title={check.available ? `${check.set_date} 晚写的条件 → ${check.date} 对账` : "对账"} extra={check.available && check.provisional ? <small className="emo-live">未收盘，暂定</small> : undefined}>
      {!check.available ? <Reason reason={check.reason} /> : check.results.length === 0 ? <p className="emo-foot">{check.set_date} 那晚没有写条件。</p> : <>
        <div className="verify-results">{check.results.map((r) => <div key={r.id} className={r.late ? "late" : ""}>
          <div><strong>{r.label}</strong>{verdict(r)}{r.late && <small className="emo-warn">事后写的，不计入</small>}</div>
          <small>写的「{r.direction_label}」· 实际 {r.actual_label ?? "—"}（{fmtValue(r.prev_value, r.kind, r.unit)} → {fmtValue(r.cur_value, r.kind, r.unit)}）</small>
          {r.reason && <small>理由：{r.reason}</small>}
          {(r.baseline != null || r.baseline_reason) && <small>{r.baseline != null ? `这个方向历史发生率 ${fmtRate(r.baseline)}` : r.baseline_reason}{r.baseline_note ? "；" + r.baseline_note : ""}</small>}
        </div>)}</div>
        <SummaryLine s={check.summary} />
      </>}
    </Card>
    <Card title={`写给下一场的条件`} extra={<small>{data.plan.next_date ?? "下一个交易日"}</small>}>
      <p className={data.plan.locked ? "emo-warn" : "emo-foot"}>{data.plan.lock_note}。只写市场层面读数的方向判断，不写个股、点位或买卖。</p>
      <div className="verify-editor">{draft.map((d, i) => {
        const m = data.metrics.find((x) => x.key === d.metric);
        const p = plan.get(d.metric);
        return <div key={i} className="verify-row">
          <select aria-label={`第 ${i + 1} 条读数`} value={d.metric} disabled={data.plan.locked} onChange={(e) => update(i, { metric: e.target.value })}>
            {[...(m ? [m] : []), ...free].map((x) => <option key={x.key} value={x.key}>{x.label}</option>)}</select>
          <select aria-label={`第 ${i + 1} 条方向`} value={d.direction} disabled={data.plan.locked} onChange={(e) => update(i, { direction: e.target.value as Direction })}>
            {data.directions.map((x) => <option key={x.key} value={x.key}>{x.label}</option>)}</select>
          <button type="button" className="mobile-delete" aria-label={`删除第 ${i + 1} 条`} disabled={data.plan.locked} onClick={() => setDraft(draft.filter((_, j) => j !== i))}><Trash2 size={15} /></button>
          <input aria-label={`第 ${i + 1} 条理由`} maxLength={200} placeholder="理由（可选，200 字内）" value={d.reason} disabled={data.plan.locked} onChange={(e) => update(i, { reason: e.target.value })} />
          <small>{m?.hint}{p && ` · 今晚 ${fmtValue(p.base_value, p.kind, m?.unit)}`}{m && ` · 变化超过 ${m.kind === "rate" ? Math.round(m.eps * 100) + " 个点" : m.eps + (m.unit ? " " + m.unit : "")} 才算变`}{p?.baseline_note ? " · " + p.baseline_note : ""}</small>
        </div>;
      })}</div>
      {!data.plan.locked && <div className="verify-actions">
        <button type="button" className="discovery-action" disabled={!free.length} onClick={() => setDraft([...draft, { metric: free[0]!.key, direction: "up", reason: "" }])}><Plus size={14} />加一条</button>
        <button type="button" className="discovery-action primary" disabled={!dirty || saving} onClick={() => void save()}><Save size={14} />{saving ? "保存中…" : "保存"}</button>
        {dirty && <small className="emo-warn">有未保存的修改</small>}
      </div>}
      {error && <div className="mobile-error" role="alert">{error}</div>}
    </Card>
    <Card title="累计战绩" extra={<small>{data.track.from ?? "—"} ~ {data.track.to}</small>}>
      <SummaryLine s={data.track.summary} />
      {(data.track.excluded_late > 0 || data.track.excluded_provisional > 0) && <p className="emo-foot">未计入：事后写的 {data.track.excluded_late} 条，暂定的 {data.track.excluded_provisional} 条。</p>}
      {data.track.days.length ? <div className="emo-detail-list">{[...data.track.days].reverse().map((d) => <div key={d.set_date} className="hist-change">
        <span>{d.set_date.slice(5)} → {d.date.slice(5)}<small>{d.total} 条{d.excluded ? `，${d.excluded} 条不计` : ""}</small></span><b>{d.hit}/{d.decided}</b><em>{d.decided ? fmtRate(d.hit / d.decided) : "—"}</em></div>)}</div>
        : <p className="emo-foot">还没有可对账的条件。</p>}
    </Card>
    <p className="emo-foot">{data.note}</p>
  </div>;
}

export function VerifyPanel() {
  const [date] = useEmotionDate();
  const path = "/mobile/shortline/verify" + (date ? "?date=" + date : "");
  const result = useCachedResource<VerifyData>(path);
  async function save(items: Draft[]) {
    if (!result.data) return;
    const response = await fetch("/api/mobile/shortline/verify", {
      method: "POST", headers: { Accept: "application/json", "Content-Type": "application/json" },
      body: JSON.stringify({ action: "save", date: result.data.date, items }),
    });
    const body = await response.json().catch(() => null) as { message?: string } | null;
    if (!response.ok) throw new Error(body?.message ?? "保存失败");
    clearEmotionCache();
    result.refresh();
  }
  return <section className="mobile-section emo-page">
    <ReviewDateBar title="明日验证" detail="今晚写下读数判断，下一场收盘后逐条对账" loading={result.loading} error={result.error} onRefresh={result.refresh} shownDate={result.data?.date} />
    {result.data && <VerifyView data={result.data} onSave={save} />}
  </section>;
}
