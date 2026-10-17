import { useMemo, useState, type ReactNode } from "react";
import { WebSelect, DateField } from "./controls";
import { CalendarDays, ChevronRight, RefreshCw, RotateCcw, Sparkles } from "lucide-react";
import { emotionContext, firstBoardContext, ladderContext, limitUpTableContext, marketFactsContext, themesContext, useRegisterAiPage } from "./aiPageContext";
import { fmtPercent, fmtYuan } from "./discoveryData";
import {
  chinaToday, emotionPath, fmtRate, useCachedResource, useEmotion,
  type CycleData, type EmotionData, type EmotionStock, type LadderLevel, type LedgerEvent, type PoolRow,
} from "./emotionData";
import type { MobilePage } from "./mobileRoutes";
import { PrevBrokenCard } from "./PrevBroken";
import { EmotionShareCard, LadderShareCard } from "./ShareCard";
import { ThemeTreeCard, useThemeTree } from "./ThemeTree";
import "./emotion.css";

type Open = (symbol: string) => void;

function retClass(value: number | null | undefined) {
  return value == null ? "" : value > 0 ? "up" : value < 0 ? "down" : "";
}
function hm(value: string | null | undefined) {
  return value ? value.slice(0, 5) : "—";
}
function Reason({ reason }: { reason: string }) {
  return <div className="mobile-empty">暂不可用：{reason}</div>;
}
function Card({ title, extra, children }: { title: string; extra?: ReactNode; children: ReactNode }) {
  return <section className="emo-card"><div className="emo-card-head"><h2>{title}</h2>{extra}</div>{children}</section>;
}
function Stat({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: ReactNode; tone?: string }) {
  return <div className="emo-stat"><span>{label}</span><strong className={tone}>{value}</strong>{sub != null && <small>{sub}</small>}</div>;
}
function CoverageNote({ sample, expected, partial }: { sample: number; expected: number; partial: boolean }) {
  return partial ? <p className="emo-warn">样本不全：{sample}/{expected} 只有当日涨跌幅，结论按部分样本看。</p> : null;
}
function versus(current: number, previous: number | null | undefined) {
  if (previous == null) return "昨日 —";
  const diff = current - previous;
  return `昨日 ${previous}${diff ? `（${diff > 0 ? "+" : ""}${diff}）` : ""}`;
}

// ------------------------------------------------------------ 公共头部:复盘日、默认、盘中、刷新

export function EmotionHeader({ title, detail, state }: {
  title: string; detail: string;
  state: { date: string; setDate: (date: string) => void; data: EmotionData | null; loading: boolean; error: string; refresh: () => void };
}) {
  const today = chinaToday();
  const { data } = state;
  return <div className="emo-header">
    <div className="emo-title"><div><h1>{title}</h1><p>{detail}</p></div>
      <button type="button" className="mobile-header-action" disabled={state.loading} onClick={state.refresh} aria-label="刷新复盘数据">
        <RefreshCw size={16} className={state.loading ? "spin" : ""} />刷新</button></div>
    <div className="emo-date-row">
      <label><CalendarDays size={15} /><span className="sr-only">复盘日</span>
        <DateField aria-label="复盘日" max={today} value={state.date || data?.date || ""} onChange={(event) => state.setDate(event.target.value)} /></label>
      <button type="button" className={!state.date ? "active" : ""} onClick={() => state.setDate("")}><RotateCcw size={14} />最近收盘</button>
      <button type="button" className={state.date === today ? "active" : ""} onClick={() => state.setDate(today)}>今日盘中</button>
    </div>
    {data && <p className="emo-meta">复盘日 {data.date} · 对照 {data.prev_date ?? "—"} · {data.source}
      {!data.settled && <strong className="emo-live">未收盘，读数随盘中变化</strong>}</p>}
    {data?.status === "partial" && <div className="discovery-notice">部分池子没取到，对应读数显示为 —：{data.errors.map((e) => e.message).join("；")}</div>}
    {state.loading && !data && <div className="mobile-empty"><RefreshCw size={16} className="spin" /> 正在拉取涨停、炸板、跌停与昨日涨停池…</div>}
    {state.error && <div className="mobile-error" role="alert">{state.error}<button className="discovery-action" onClick={state.refresh}>重试</button></div>}
  </div>;
}

function useEmotionPage() {
  return useEmotion();
}

// ------------------------------------------------------------ 首页情绪速览(始终是最近一个已收盘交易日)

export function HomeEmotionView({ data, onOpen }: { data: EmotionData; onOpen: () => void }) {
  const s = data.summary;
  const first = data.promotion.available ? data.promotion.tiers["1进2"] : null;
  const me = data.money_effect.available ? data.money_effect : null;
  return <section className="home-emotion" aria-label="情绪速览">
    <div className="home-section-heading"><div><h2>情绪速览</h2><span>{data.date} · 对照 {data.prev_date ?? "—"}{data.settled ? "" : " · 未收盘"}</span></div>
      <button type="button" className="home-section-link" onClick={onOpen}>情绪复盘<ChevronRight size={15} /></button></div>
    <div className="home-emotion-grid">
      <div><span>涨停</span><strong className="up">{s.limit_up}</strong></div>
      <div><span>炸板率</span><strong>{fmtRate(s.broken_rate)}</strong></div>
      <div><span>跌停</span><strong className="down">{s.limit_down ?? "—"}</strong></div>
      <div><span>最高板</span><strong>{s.highest}</strong></div>
      <div><span>1进2</span><strong>{first ? fmtRate(first.rate) : "—"}</strong></div>
      <div><span>赚钱效应</span><strong className={retClass(me?.median)}>{me ? fmtPercent(me.median) : "—"}</strong></div>
    </div>
  </section>;
}

export function HomeEmotionBrief({ onOpen }: { onOpen: () => void }) {
  const result = useCachedResource<EmotionData>(emotionPath(""));
  if (result.data) return <HomeEmotionView data={result.data} onOpen={onOpen} />;
  return <section className="home-emotion" aria-label="情绪速览">
    <div className="home-section-heading"><div><h2>情绪速览</h2><span>{result.loading ? "正在拉取最近一个收盘日…" : "暂时取不到"}</span></div>
      <button type="button" className="home-section-link" onClick={result.error ? result.refresh : onOpen}>{result.error ? "重试" : "情绪复盘"}<ChevronRight size={15} /></button></div>
    {result.error && <p className="home-inline-error">{result.error}</p>}
  </section>;
}

// ------------------------------------------------------------ 今日情绪

export function SummaryStrip({ data }: { data: EmotionData }) {
  const { summary: s, prev_summary: p } = data;
  return <div className="emo-stats emo-stats-4">
    <Stat label="涨停" value={s.limit_up} sub={versus(s.limit_up, p?.limit_up)} tone="up" />
    <Stat label="炸板" value={s.broken ?? "—"} sub={`炸板率 ${fmtRate(s.broken_rate)}`} />
    <Stat label="跌停" value={s.limit_down ?? "—"} tone="down" sub={`封板率 ${fmtRate(s.seal_rate)}`} />
    <Stat label="最高板" value={`${s.highest} 板`} sub={versus(s.highest, p?.highest)} />
    <Stat label="连板" value={s.consec_count} sub={versus(s.consec_count, p?.consec_count)} />
    <Stat label="首板" value={s.first_count} />
  </div>;
}

export function FourReadings({ data }: { data: EmotionData }) {
  const me = data.money_effect, pr = data.promotion, cp = data.consec_premium, gap = data.ladder_gap;
  return <div className="emo-grid">
    <Card title="赚钱效应" extra={<small>昨日涨停股今日表现</small>}>
      {me.available ? <>
        <div className="emo-stats"><Stat label="中位" value={fmtPercent(me.median)} tone={retClass(me.median)} /><Stat label="均值" value={fmtPercent(me.avg)} tone={retClass(me.avg)} />
          <Stat label="翻红率" value={fmtRate(me.positive_rate)} /><Stat label="再度涨停" value={fmtRate(me.limit_up_again_rate)} /></div>
        <p className="emo-foot">样本 {me.sample} 只 · {me.prev_date ?? "前一交易日"} 收盘涨停</p>
        <CoverageNote sample={me.sample} expected={me.expected_sample} partial={me.partial} />
      </> : <Reason reason={me.reason} />}
    </Card>
    <Card title="晋级率" extra={pr.available ? <strong className="emo-big">{fmtRate(pr.overall.rate)}</strong> : undefined}>
      {pr.available ? <div className="emo-bars">{Object.entries(pr.tiers).map(([name, tier]) =>
        <div className="emo-bar-row" key={name}><span>{name}</span><div className="emo-bar"><i style={{ width: `${Math.round((tier.rate ?? 0) * 100)}%` }} /></div>
          <small>{tier.promoted}/{tier.base} · {fmtRate(tier.rate)}</small></div>)}
        <p className="emo-foot">整体 {pr.overall.promoted}/{pr.overall.base}；口径：昨日各档连板今日仍在涨停池</p></div> : <Reason reason={pr.reason} />}
    </Card>
    <Card title="连板溢价" extra={<small>昨日 2 板以上</small>}>
      {cp.available ? <>
        <div className="emo-stats"><Stat label="中位" value={fmtPercent(cp.median)} tone={retClass(cp.median)} /><Stat label="均值" value={fmtPercent(cp.avg)} tone={retClass(cp.avg)} />
          <Stat label="翻红率" value={fmtRate(cp.positive_rate)} /><Stat label="样本" value={`${cp.sample} 只`} /></div>
        <CoverageNote sample={cp.sample} expected={cp.expected_sample} partial={cp.partial} />
      </> : <Reason reason={cp.reason} />}
    </Card>
    <Card title="梯队结构" extra={gap.available ? <small>最高 {gap.highest} 板</small> : undefined}>
      {gap.available ? <>
        <div className="emo-chips">{gap.tiers.map((tier) => <span key={tier.boards}>{tier.boards}板 × {tier.count}</span>)}
          {gap.gaps.map((g) => <span className="emo-gap" key={"g" + g}>{g}板 缺</span>)}
          {!gap.tiers.length && <span>无 2 板以上</span>}</div>
        <p className="emo-foot">{gap.note}</p>
      </> : <Reason reason={gap.reason} />}
    </Card>
  </div>;
}

export function CycleCard({ data, loading, error }: { data: CycleData | null; loading: boolean; error: string }) {
  const cycle = data?.cycle;
  return <Card title="情绪周期" extra={<small>窗口内相对读数</small>}>
    {loading && !data && <div className="mobile-empty"><RefreshCw size={14} className="spin" /> 正在拉取近 10 个交易日…</div>}
    {error && <div className="mobile-error">{error}</div>}
    {cycle && !cycle.available && <Reason reason={cycle.reason} />}
    {cycle?.available && <>
      <div className="emo-stats"><Stat label="走向" value={cycle.trend} tone={cycle.trend.includes("强") ? "up" : cycle.trend.includes("弱") ? "down" : ""} />
        <Stat label="距低点" value={`第 ${cycle.day_n} 天`} sub={cycle.trough_date} /><Stat label="当前分" value={cycle.current_score} sub={`分位 ${fmtRate(cycle.pctile)}`} /></div>
      <div className="emo-cycle" role="img" aria-label={`近 ${cycle.window} 个交易日情绪分`}>
        {cycle.series.map((day) => <div key={day.date} title={`${day.date} 涨停 ${day.limit_up} 最高 ${day.highest_consec} 板 炸板率 ${fmtRate(day.broken_rate)}`}>
          <i className={day.date === cycle.trough_date ? "trough" : ""} style={{ height: `${Math.max(6, day.score * 100)}%` }} /><small>{day.date.slice(5)}</small></div>)}
      </div>
      <p className="emo-foot">分数 = 涨停家数、最高连板、(1−炸板率) 在窗口内归一化后平均；“距低点第 N 天”和“正在往上走”是两件事。</p>
    </>}
  </Card>;
}

const QUICK: { page: MobilePage; title: string; detail: (d: EmotionData) => string }[] = [
  { page: "ladder", title: "连板天梯", detail: (d) => `${d.summary.consec_count} 只连板 · 最高 ${d.summary.highest} 板` },
  { page: "limitUp", title: "涨停全表", detail: (d) => `涨停 ${d.summary.limit_up} · 炸板 ${d.summary.broken ?? "—"} · 跌停 ${d.summary.limit_down ?? "—"}` },
  { page: "firstBoard", title: "首板名单", detail: (d) => `${d.summary.first_count} 只首板` },
  { page: "seal", title: "盘面事实", detail: () => "强势股反馈 · 亏钱效应 · 封板质量" },
  { page: "themes", title: "行业题材", detail: (d) => d.themes.available ? `${d.themes.theme_count} 个行业 · 关键事件` : "行业结构 · 关键事件" },
];

export function EmotionTodayView({ data, cycle, onNavigate }: { data: EmotionData; cycle: { data: CycleData | null; loading: boolean; error: string }; onNavigate: (page: MobilePage) => void }) {
  return <div className="emo-body">
    <SummaryStrip data={data} />
    <FourReadings data={data} />
    <CycleCard {...cycle} />
    {data.seal_quality.available && <Card title="封板质量" extra={<button type="button" className="emo-link" onClick={() => onNavigate("seal")}>盘面事实<ChevronRight size={14} /></button>}>
      <div className="emo-stats emo-stats-4"><Stat label="没炸过" value={data.seal_quality.never_broken} sub={fmtRate(data.seal_quality.never_broken_rate)} />
        <Stat label="炸后回封" value={data.seal_quality.reopened} /><Stat label="开盘秒板" value={data.seal_quality.opening_seconds} /><Stat label="14:30后封" value={data.seal_quality.late_seal} /></div>
    </Card>}
    <div className="emo-quick">{QUICK.map((item) => <button type="button" key={item.page} onClick={() => onNavigate(item.page)}>
      <span><strong>{item.title}</strong><small>{item.detail(data)}</small></span><ChevronRight size={16} /></button>)}</div>
    <EmotionShareCard data={data} />
    <p className="emo-foot">{data.source_note} 只描述已经发生的盘面现象，不构成买卖建议。</p>
  </div>;
}

export function EmotionTodayPanel({ onNavigate, onAskAi }: { onNavigate: (page: MobilePage) => void; onAskAi?: (question: string) => void }) {
  const state = useEmotionPage();
  useRegisterAiPage("今日情绪", useMemo(() => (state.data ? emotionContext(state.data) : null), [state.data]));
  const cycle = useCachedResource<CycleData>("/mobile/shortline/cycle?days=10" + (state.date ? "&date=" + state.date : ""));
  return <section className="mobile-section emo-page">
    <EmotionHeader title="今日情绪" detail="赚钱效应、晋级率、连板溢价、梯队与周期" state={state} />
    {state.data && onAskAi && <button type="button" className="emo-link emo-ask" onClick={() => onAskAi("结合这页的读数,今天的赚钱效应、晋级率和梯队说明了什么?哪些读数取不到或样本偏少?")}><Sparkles size={13} />问 AI:解读今天的情绪读数</button>}
    {state.data && <EmotionTodayView data={state.data} cycle={cycle} onNavigate={onNavigate} />}
  </section>;
}

// ------------------------------------------------------------ 连板天梯

function StockChip({ stock, onOpen }: { stock: EmotionStock; onOpen: Open }) {
  return <button type="button" className="emo-stock" onClick={() => onOpen(stock.code)}>
    <strong>{stock.name}</strong>
    <span>{stock.rebound && <em>{stock.label}</em>}{stock.board !== "10cm" && <em className="emo-board">{stock.board}</em>}
      首封 {hm(stock.first_seal)}{stock.broken_times > 0 && ` · 炸${stock.broken_times}`}</span>
    <small>{stock.sector || "—"} · 封单 {fmtYuan(stock.seal_fund)}</small>
  </button>;
}

function LadderLevelBlock({ level, onOpen }: { level: LadderLevel; onOpen: Open }) {
  const [expanded, setExpanded] = useState(false);
  const fell = expanded ? level.fell : level.fell.slice(0, 6);
  const counts = level.fell.reduce<Record<string, number>>((acc, f) => ({ ...acc, [f.outcome]: (acc[f.outcome] ?? 0) + 1 }), {});
  return <div className="emo-level">
    <div className="emo-level-head"><strong>{level.boards === 1 ? "首板" : `${level.boards} 板`}</strong><span>{level.stocks.length} 只</span>
      {level.base != null && level.boards > 1 && <small>{level.boards - 1}进{level.boards} {level.promoted}/{level.base} · {fmtRate(level.rate)}</small>}</div>
    {level.stocks.length > 0 ? <div className="emo-stock-grid">{level.stocks.map((s) => <StockChip key={s.code} stock={s} onOpen={onOpen} />)}</div>
      : <p className="emo-foot">今日没有 {level.boards} 板。</p>}
    {level.fell.length > 0 && level.boards > 1 && <div className="emo-fell">
      <p>没接上：{Object.entries(counts).map(([k, v]) => `${k} ${v}`).join(" · ")}</p>
      <div className="emo-chips">{fell.map((f) => <button type="button" key={f.code} onClick={() => onOpen(f.code)}>{f.name} {f.outcome}<b className={retClass(f.ret)}>{fmtPercent(f.ret)}</b></button>)}</div>
      {level.fell.length > 6 && <button type="button" className="emo-link" onClick={() => setExpanded(!expanded)}>{expanded ? "收起" : `展开其余 ${level.fell.length - 6} 只`}</button>}
    </div>}
  </div>;
}

export function LadderView({ data, onOpenSymbol, showFirst = false }: { data: EmotionData; onOpenSymbol: Open; showFirst?: boolean }) {
  const ladder = data.ladder;
  if (!ladder.available) return <Reason reason={ladder.reason} />;
  const levels = ladder.levels.filter((level) => showFirst || level.boards > 1);
  return <div className="emo-body">
    <div className="emo-stats emo-stats-4"><Stat label="涨停" value={ladder.total_limit_up} /><Stat label="连板" value={ladder.consec_count} />
      <Stat label="最高板" value={ladder.highest} /><Stat label="对照日" value={data.prev_date?.slice(5) ?? "—"} /></div>
    {ladder.prev_note && <div className="discovery-notice">{ladder.prev_note}</div>}
    {levels.map((level) => <LadderLevelBlock key={level.boards} level={level} onOpen={onOpenSymbol} />)}
    <p className="emo-foot">“N天M板”为断板后反包；没接上的按跌停 → 炸板 → 没涨停判定，涨跌幅取定稿记录。</p>
  </div>;
}

export function LadderPanel({ onOpenSymbol, onAskAi }: { onOpenSymbol: Open; onAskAi?: (question: string) => void }) {
  const state = useEmotionPage();
  const [showFirst, setShowFirst] = useState(false);
  const aiText = useMemo(() => (state.data ? ladderContext(state.data) : null), [state.data]);
  useRegisterAiPage("连板天梯", aiText);
  return <section className="mobile-section emo-page">
    <EmotionHeader title="连板天梯" detail="每层晋级、没接上的票与反包标记" state={state} />
    {aiText && onAskAi && <button type="button" className="emo-link emo-ask" onClick={() => onAskAi("按这页的连板天梯,说说各层的晋级情况、断档和没接上的票;只描述已发生的现象,不要给买卖建议。")}><Sparkles size={13} />问 AI:解读这条连板梯队</button>}
    <label className="discovery-check"><input type="checkbox" checked={showFirst} onChange={(e) => setShowFirst(e.target.checked)} />同时显示首板层</label>
    {state.data && <LadderView data={state.data} onOpenSymbol={onOpenSymbol} showFirst={showFirst} />}
    {state.data && state.data.ladder.available && <LadderShareCard data={state.data} />}
  </section>;
}

// ------------------------------------------------------------ 涨停全表 / 炸板池 / 跌停池

type PoolTab = "zt" | "zb" | "dt";
const ZT_SORTS = {
  first_seal: { label: "首封时间", value: (s: EmotionStock) => s.first_seal ?? "99", asc: true },
  boards: { label: "连板高度", value: (s: EmotionStock) => s.boards, asc: false },
  seal_fund: { label: "封单", value: (s: EmotionStock) => s.seal_fund ?? -1, asc: false },
  amount: { label: "成交额", value: (s: EmotionStock) => s.amount ?? -1, asc: false },
  turnover: { label: "换手", value: (s: EmotionStock) => s.turnover ?? -1, asc: false },
  float_cap: { label: "流通市值", value: (s: EmotionStock) => s.float_cap ?? -1, asc: true },
  broken_times: { label: "炸板次数", value: (s: EmotionStock) => s.broken_times, asc: false },
} as const;
type ZtSort = keyof typeof ZT_SORTS;

function compare<T>(rows: T[], value: (row: T) => string | number, asc: boolean) {
  return [...rows].sort((a, b) => {
    const x = value(a), y = value(b);
    const order = x < y ? -1 : x > y ? 1 : 0;
    return asc ? order : -order;
  });
}

export function LimitUpTableView({ data, onOpenSymbol, initialTab = "zt" }: { data: EmotionData; onOpenSymbol: Open; initialTab?: PoolTab }) {
  const [tab, setTab] = useState<PoolTab>(initialTab);
  const [keyword, setKeyword] = useState("");
  const [height, setHeight] = useState("all");
  const [sort, setSort] = useState<ZtSort>("first_seal");
  const query = keyword.trim().toLocaleLowerCase();
  const match = (row: { code: string; name: string; sector: string }) => !query || (row.code + row.name + row.sector).toLocaleLowerCase().includes(query);
  const zt = compare(data.limit_up_rows.filter((row) => match(row)
    && (height === "all" || (height === "first" ? row.boards <= 1 : height === "consec" ? row.boards >= 2 : row.boards >= 3))), ZT_SORTS[sort].value, ZT_SORTS[sort].asc);
  const pool = tab === "zb" ? data.broken_rows : data.limit_down_rows;
  const poolRows = tab === "zt" ? [] : pool.rows.filter(match);
  return <div className="emo-body">
    <div className="mobile-stock-tabs" role="tablist" aria-label="涨停炸板跌停">
      {([["zt", `涨停 ${data.summary.limit_up}`], ["zb", `炸板 ${data.summary.broken ?? "—"}`], ["dt", `跌停 ${data.summary.limit_down ?? "—"}`]] as const).map(([id, label]) =>
        <button key={id} type="button" role="tab" aria-selected={tab === id} className={tab === id ? "active" : ""} onClick={() => setTab(id)}>{label}</button>)}
    </div>
    <div className="mobile-filter-row">
      <input type="search" aria-label="筛选代码名称行业" placeholder="代码 / 名称 / 行业" value={keyword} maxLength={40} onChange={(e) => setKeyword(e.target.value)} />
      {tab === "zt" && <>
        <WebSelect aria-label="连板筛选" value={height} onChange={(e) => setHeight(e.target.value)}><option value="all">全部</option><option value="first">首板</option><option value="consec">连板</option><option value="high">3板以上</option></WebSelect>
        <WebSelect aria-label="排序" value={sort} onChange={(e) => setSort(e.target.value as ZtSort)}>{Object.entries(ZT_SORTS).map(([key, item]) => <option key={key} value={key}>{item.label}</option>)}</WebSelect>
      </>}
    </div>
    {tab === "zt" ? <>
      <p className="mobile-filter-note">显示 {zt.length}/{data.limit_up_rows.length} 只 · 金额单位元</p>
      <div className="emo-table">{zt.map((s) => <button type="button" className="emo-row" key={s.code} onClick={() => onOpenSymbol(s.code)}>
        <div className="emo-row-main"><strong>{s.name}</strong><span>{s.code} · {s.sector || "—"}</span></div>
        <div className="emo-row-tags"><em>{s.label}</em>{s.board !== "10cm" && <em className="emo-board">{s.board}</em>}{s.prev_boards != null && s.prev_boards > 0 && <em>昨日 {s.prev_boards} 板</em>}</div>
        <dl><div><dt>首封</dt><dd>{hm(s.first_seal)}</dd></div><div><dt>末封</dt><dd>{hm(s.last_seal)}</dd></div><div><dt>炸板</dt><dd>{s.broken_times}</dd></div>
          <div><dt>封单</dt><dd>{fmtYuan(s.seal_fund)}</dd></div><div><dt>成交</dt><dd>{fmtYuan(s.amount)}</dd></div><div><dt>换手</dt><dd>{s.turnover == null ? "—" : s.turnover.toFixed(1) + "%"}</dd></div>
          <div><dt>流通</dt><dd>{fmtYuan(s.float_cap)}</dd></div><div><dt>收盘</dt><dd>{s.close?.toFixed(2) ?? "—"}</dd></div><div><dt>涨幅</dt><dd className={retClass(s.ret)}>{fmtPercent(s.ret)}</dd></div></dl>
      </button>)}</div>
      {!zt.length && <div className="mobile-empty">没有符合条件的涨停股。</div>}
    </> : !pool.available ? <Reason reason={pool.reason ?? "未取得"} /> : <>
      <p className="mobile-filter-note">显示 {poolRows.length}/{pool.rows.length} 只{tab === "zb" ? " · 盘中触及涨停但收盘未封住" : " · 收盘封在跌停"}</p>
      <div className="emo-table">{poolRows.map((row: PoolRow) => <button type="button" className="emo-row" key={row.code} onClick={() => onOpenSymbol(row.code)}>
        <div className="emo-row-main"><strong>{row.name}</strong><span>{row.code} · {row.sector || "—"}</span></div>
        <div className="emo-row-tags">{row.prev_boards != null && row.prev_boards > 0 && <em>昨日 {row.prev_boards} 板</em>}{row.board !== "10cm" && <em className="emo-board">{row.board}</em>}</div>
        <dl>{tab === "zb" ? <><div><dt>首封</dt><dd>{hm(row.first_seal)}</dd></div><div><dt>炸板</dt><dd>{row.broken_times}</dd></div><div><dt>振幅</dt><dd>{row.amplitude == null ? "—" : row.amplitude.toFixed(1) + "%"}</dd></div>
          <div><dt>涨停价</dt><dd>{row.limit_price?.toFixed(2) ?? "—"}</dd></div></>
          : <><div><dt>连续跌停</dt><dd>{row.consec_dt} 天</dd></div><div><dt>开板</dt><dd>{row.open_times} 次</dd></div><div><dt>封单</dt><dd>{fmtYuan(row.seal_fund)}</dd></div><div><dt>末封</dt><dd>{hm(row.last_seal)}</dd></div></>}
          <div><dt>成交</dt><dd>{fmtYuan(row.amount)}</dd></div><div><dt>换手</dt><dd>{row.turnover == null ? "—" : row.turnover.toFixed(1) + "%"}</dd></div>
          <div><dt>涨幅</dt><dd className={retClass(row.ret)}>{fmtPercent(row.ret)}</dd></div></dl>
      </button>)}</div>
      {!poolRows.length && <div className="mobile-empty">{query ? "没有匹配记录。" : "当日池子为空。"}</div>}
    </>}
  </div>;
}

export function LimitUpTablePanel({ onOpenSymbol, onAskAi }: { onOpenSymbol: Open; onAskAi?: (question: string) => void }) {
  const state = useEmotionPage();
  const aiText = useMemo(() => (state.data ? limitUpTableContext(state.data) : null), [state.data]);
  useRegisterAiPage("涨停全表", aiText);
  return <section className="mobile-section emo-page">
    <EmotionHeader title="涨停全表" detail="涨停、炸板、跌停三池完整字段，可筛选排序" state={state} />
    {aiText && onAskAi && <button type="button" className="emo-link emo-ask" onClick={() => onAskAi("按这页的涨停池,把高度板和主要题材梳理一下,再说说炸板池和跌停池的规模;只描述已发生的现象,不要给买卖建议。")}><Sparkles size={13} />问 AI:梳理今天的涨停池</button>}
    {state.data && <LimitUpTableView data={state.data} onOpenSymbol={onOpenSymbol} />}
  </section>;
}

// ------------------------------------------------------------ 首板名单

export function FirstBoardView({ data, onOpenSymbol, reasons }: { data: EmotionData; onOpenSymbol: Open; reasons?: Record<string, string> }) {
  const [onlyRebound, setOnlyRebound] = useState(false);
  const fb = data.first_board;
  if (!fb.available) return <Reason reason={fb.reason} />;
  const stocks = fb.stocks.filter((s) => !onlyRebound || s.rebound);
  const slots = data.themes.available ? data.themes.timeline : [];
  return <div className="emo-body">
    <div className="emo-stats emo-stats-4"><Stat label="首板" value={fb.first_count} /><Stat label="其中反包" value={fb.rebound_count} />
      <Stat label="全部涨停" value={fb.total_limit_up} /><Stat label="首板占比" value={fmtRate(fb.total_limit_up ? fb.first_count / fb.total_limit_up : null)} /></div>
    {slots.length > 0 && <div className="emo-timeline" aria-label="全部涨停首封时间分布">{slots.map((slot) => <div key={slot.slot}><b>{slot.count}</b><small>{slot.slot}</small></div>)}</div>}
    <label className="discovery-check"><input type="checkbox" checked={onlyRebound} onChange={(e) => setOnlyRebound(e.target.checked)} />只看反包（断板后再涨停）</label>
    <div className="emo-table">{stocks.map((s) => <button type="button" className="emo-row" key={s.code} onClick={() => onOpenSymbol(s.code)}>
      <div className="emo-row-main"><strong>{s.name}</strong><span>{s.code} · {s.sector || "—"}</span></div>
      <div className="emo-row-tags">{s.rebound && <em>{s.label}</em>}{s.board !== "10cm" && <em className="emo-board">{s.board}</em>}</div>
      {reasons?.[s.code] && <p className="emo-reason">{reasons[s.code]}</p>}
      <dl><div><dt>首封</dt><dd>{hm(s.first_seal)}</dd></div><div><dt>炸板</dt><dd>{s.broken_times}</dd></div><div><dt>收盘</dt><dd>{s.close?.toFixed(2) ?? "—"}</dd></div>
        <div><dt>涨幅</dt><dd className={retClass(s.ret)}>{fmtPercent(s.ret)}</dd></div><div><dt>成交</dt><dd>{fmtYuan(s.amount)}</dd></div><div><dt>流通</dt><dd>{fmtYuan(s.float_cap)}</dd></div></dl>
    </button>)}</div>
    {!stocks.length && <div className="mobile-empty">没有符合条件的首板。</div>}
  </div>;
}

export function FirstBoardPanel({ onOpenSymbol, onAskAi }: { onOpenSymbol: Open; onAskAi?: (question: string) => void }) {
  const state = useEmotionPage();
  const tree = useThemeTree(state.date);
  const aiText = useMemo(() => (state.data ? firstBoardContext(state.data, tree.data?.reasons) : null), [state.data, tree.data]);
  useRegisterAiPage("首板名单", aiText);
  return <section className="mobile-section emo-page">
    <EmotionHeader title="首板名单" detail="按首封时间排列，标出反包、制度与涨停原因" state={state} />
    {aiText && onAskAi && <button type="button" className="emo-link emo-ask" onClick={() => onAskAi("按这页的首板名单,把它们按题材和首封时间归类,说说有什么共性和差异;只描述已发生的现象,不要给买卖建议。")}><Sparkles size={13} />问 AI:归类这些首板</button>}
    {state.data && <FirstBoardView data={state.data} onOpenSymbol={onOpenSymbol} reasons={tree.data?.reasons} />}
    {state.data && !tree.data && <p className="emo-foot">{tree.error ? `涨停原因这次没取到:${tree.error}` : "涨停原因加载中…"}</p>}
    {tree.data && Object.keys(tree.data.reasons).length === 0 && <p className="emo-foot">涨停原因没取到:{tree.data.tree.available ? "没有原因覆盖" : tree.data.tree.reason}。</p>}
  </section>;
}

// ------------------------------------------------------------ 盘面事实:反馈矩阵 / 亏钱效应 / 封板质量 / 分制度

export function MarketFactsView({ data, onOpenSymbol }: { data: EmotionData; onOpenSymbol: Open }) {
  const [showDetails, setShowDetails] = useState(false);
  const fb = data.feedback, loss = data.loss_effect, seal = data.seal_quality, boards = data.by_board;
  return <div className="emo-body">
    <Card title="昨日强势股反馈" extra={<small>{data.prev_date ?? "—"} 收盘涨停 → 今日</small>}>
      {fb.available ? <>
        <div className="emo-matrix" role="table" aria-label="昨日强势股反馈矩阵">
          <div role="row" className="head"><span role="columnheader">昨日</span>{fb.result_order.map((k) => <span role="columnheader" key={k}>{k}</span>)}<span role="columnheader">晋级率</span></div>
          {fb.rows.map((row) => <div role="row" key={row.tier}><span role="rowheader">{row.tier}<small>{row.合计}</small></span>
            {fb.result_order.map((k) => <span role="cell" key={k} className={Number(row[k]) === 0 ? "zero" : k === "晋级涨停" || k === "收红" ? "up" : k === "跌超5%" || k === "跌停" ? "down" : ""}>{row[k] ?? 0}</span>)}
            <span role="cell">{fmtRate(row.晋级率)}</span></div>)}
        </div>
        <CoverageNote sample={fb.sample} expected={fb.expected_sample} partial={fb.partial} />
        <button type="button" className="emo-link" onClick={() => setShowDetails(!showDetails)}>{showDetails ? "收起明细" : `逐只明细（${fb.details.length}，差的在前）`}</button>
        {showDetails && <div className="emo-detail-list">{fb.details.map((d) => <button type="button" key={d.code} onClick={() => onOpenSymbol(d.code)}>
          <span>{d.name}<small>{d.prev_tier} · {d.sector || "—"}</small></span><em>{d.result}</em><b className={retClass(d.ret)}>{fmtPercent(d.ret)}</b></button>)}</div>}
        <p className="emo-foot">{fb.note}</p>
      </> : <Reason reason={fb.reason} />}
    </Card>
    <Card title="亏钱效应" extra={<small>昨日涨停股今天死在哪</small>}>
      {loss.available ? <>
        <div className="emo-stats emo-stats-4"><Stat label="跌超5%" value={loss.deep_loss_5_count} sub={fmtRate(loss.deep_loss_5_rate)} tone="down" /><Stat label="跌超7%" value={loss.deep_loss_7_count} tone="down" />
          <Stat label="跌停" value={loss.limit_down_count} tone="down" /><Stat label="最差" value={fmtPercent(loss.worst)} tone="down" /></div>
        <p className="emo-foot">样本 {loss.sample} 只 · 全市场跌停 {loss.market_limit_down ?? "—"} 家。{loss.note}</p>
      </> : <Reason reason={loss.reason} />}
    </Card>
    <Card title="封板质量">
      {seal.available ? <>
        <div className="emo-stats emo-stats-4"><Stat label="没炸过" value={seal.never_broken} sub={fmtRate(seal.never_broken_rate)} /><Stat label="炸后回封" value={seal.reopened} sub={`开过板 ${fmtRate(seal.ever_opened_rate)}`} />
          <Stat label="开盘秒板" value={seal.opening_seconds} sub="09:35 前首封" /><Stat label="14:30后封" value={seal.late_seal} />
          <Stat label="炸板率" value={fmtRate(seal.broken_rate)} sub={`炸板池 ${seal.broken_pool ?? "—"}`} /><Stat label="平均炸板" value={seal.avg_broken_times} sub="次 / 涨停股" /></div>
        <p className="emo-foot">{seal.note}</p>
      </> : <Reason reason={seal.reason} />}
    </Card>
    <Card title="分涨跌幅制度">
      {boards.available ? <><div className="emo-board-grid">{boards.boards.map((b) => <div key={b.board} className={b.limit_up === 0 ? "muted" : ""}>
        <strong>{b.board}{b.limit_up === 0 && <small>零涨停</small>}</strong>
        <span>涨停 {b.limit_up} · 最高 {b.highest} 板</span><span>晋级 {b.promoted ?? "—"}/{b.prev_base ?? "—"} · {fmtRate(b.promotion_rate)}</span>
        <span>炸 {b.broken ?? "—"} · 跌停 {b.limit_down ?? "—"}</span></div>)}</div>
        <p className="emo-foot">{boards.note}</p></> : <Reason reason={boards.reason} />}
    </Card>
  </div>;
}

export function MarketFactsPanel({ onOpenSymbol, onAskAi }: { onOpenSymbol: Open; onAskAi?: (question: string) => void }) {
  const state = useEmotionPage();
  const aiText = useMemo(() => (state.data ? marketFactsContext(state.data) : null), [state.data]);
  useRegisterAiPage("盘面事实", aiText);
  return <section className="mobile-section emo-page">
    <EmotionHeader title="盘面事实" detail="强势股反馈、亏钱效应、封板质量与分制度" state={state} />
    {aiText && onAskAi && <button type="button" className="emo-link emo-ask" onClick={() => onAskAi("按这页的读数,说说强势股今天的反馈、亏钱效应和封板质量各说明了什么;取不到或样本偏少的要指出来。只描述已发生的现象,不要给买卖建议。")}><Sparkles size={13} />问 AI:解读盘面事实</button>}
    {state.data && <MarketFactsView data={state.data} onOpenSymbol={onOpenSymbol} />}
    {state.data && <PrevBrokenCard date={state.date} onOpenSymbol={onOpenSymbol} />}
  </section>;
}

// ------------------------------------------------------------ 行业题材 + 关键事件

function EventGroup({ title, events, onOpen }: { title: string; events: LedgerEvent[]; onOpen: Open }) {
  if (!events.length) return null;
  return <div className="emo-events"><h3>{title}</h3>{events.map((e, i) => <button type="button" key={e.tag + e.code + i} onClick={() => onOpen(e.code)}>
    <em>{e.tag}</em><span><strong>{e.name}</strong><small>{e.note}{e.sector ? ` · ${e.sector}` : ""}</small></span><b className={retClass(e.ret)}>{fmtPercent(e.ret)}</b></button>)}</div>;
}

export function ThemesView({ data, onOpenSymbol }: { data: EmotionData; onOpenSymbol: Open }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const themes = data.themes, events = data.events;
  if (!themes.available) return <Reason reason={themes.reason} />;
  const max = Math.max(1, ...themes.themes.map((t) => t.limit_up));
  const rows = showAll ? themes.themes : themes.themes.slice(0, 10);
  const slotMax = Math.max(1, ...themes.timeline.map((s) => s.count));
  return <div className="emo-body">
    <Card title="行业结构 · 发酵节奏" extra={<small>头部集中度 {fmtRate(themes.concentration)}</small>}>
      <div className="emo-theme-list">{rows.map((theme) => <div key={theme.sector} className="emo-theme">
        <button type="button" className="emo-theme-head" aria-expanded={expanded === theme.sector} onClick={() => setExpanded(expanded === theme.sector ? null : theme.sector)}>
          <span><strong>{theme.sector}</strong><small>首板 {theme.first_boards} · 连板 {theme.consec_boards} · 最高 {theme.highest} 板{theme.broken ? ` · 炸 ${theme.broken}` : ""}</small></span>
          <i className="emo-bar"><b style={{ width: `${(theme.limit_up / max) * 100}%` }} /></i><em>{theme.limit_up}</em></button>
        <p className="emo-foot">首封 {hm(theme.first_seal)} → {hm(theme.last_seal)}{theme.prev_limit_up != null && ` · 昨日 ${theme.prev_limit_up} 只，今日仍涨停 ${theme.continued}（延续率 ${fmtRate(theme.continuation_rate)}）`}</p>
        {expanded === theme.sector && <div className="emo-chips">{theme.names.map((n) => <button type="button" key={n.code} onClick={() => onOpenSymbol(n.code)}>{n.name}<b>{n.label}</b></button>)}</div>}
      </div>)}</div>
      {themes.themes.length > 10 && <button type="button" className="emo-link" onClick={() => setShowAll(!showAll)}>{showAll ? "只看前 10" : `展开全部 ${themes.theme_count} 个行业`}</button>}
      {themes.faded.length > 0 && <p className="emo-foot">昨日有涨停、今日没有：{themes.faded.slice(0, 12).map((f) => `${f.sector}(${f.prev_limit_up})`).join("、")}</p>}
    </Card>
    <Card title="涨停时间分布" extra={<small>首封时间</small>}>
      <div className="emo-histogram">{themes.timeline.map((slot) => <div key={slot.slot}><i style={{ height: `${(slot.count / slotMax) * 100}%` }} /><b>{slot.count}</b><small>{slot.slot}</small></div>)}</div>
    </Card>
    <Card title="关键事件账本">
      {events.available ? <>
        <EventGroup title="今天钱亏在哪" events={events.loss_events} onOpen={onOpenSymbol} />
        <EventGroup title="分歧最大的" events={events.split_events} onOpen={onOpenSymbol} />
        <EventGroup title="今天钱赚在哪" events={events.gain_events} onOpen={onOpenSymbol} />
        {!events.prev_available && <p className="emo-foot">前一交易日涨停池未取得，断板类标签无法判断。</p>}
      </> : <Reason reason={events.reason} />}
    </Card>
    <p className="emo-foot">下面的行业按东财所属行业归类;题材看上面的事件树(同花顺涨停原因)。</p>
  </div>;
}

export function ThemesPanel({ onOpenSymbol, onAskAi }: { onOpenSymbol: Open; onAskAi?: (question: string) => void }) {
  const state = useEmotionPage();
  const aiText = useMemo(() => (state.data ? themesContext(state.data) : null), [state.data]);
  useRegisterAiPage("行业题材", aiText);
  return <section className="mobile-section emo-page">
    <EmotionHeader title="行业题材" detail="题材事件树、行业梯队、跨日延续、发酵节奏与关键事件" state={state} />
    {aiText && onAskAi && <button type="button" className="emo-link emo-ask" onClick={() => onAskAi("按这页的行业分布,梳理今天涨停集中在哪些行业、哪些有跨日延续;只描述已发生的现象,不要给买卖建议。")}><Sparkles size={13} />问 AI:梳理今天的题材</button>}
    {state.data && <ThemeTreeCard date={state.data.date} onOpenSymbol={onOpenSymbol} />}
    {state.data && <ThemesView data={state.data} onOpenSymbol={onOpenSymbol} />}
  </section>;
}

