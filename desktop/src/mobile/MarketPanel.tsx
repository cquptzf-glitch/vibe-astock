/**
 * 复盘 · 市场面板(移动版,对应桌面版「每日复盘」的市场状态 / 主要指数 / 中长线 / 资金去了哪 / 全球市场)。
 * 一屏看全:涨跌面 · 量能 · 短线 · 中长线位置 · 杠杆 · 股债收益率。每一块各自标缺口,取不到写原因,不显示成 0。
 * 都是按固定公式算的客观事实,不是趋势判断,不给买卖建议。短线格子直接复用情绪复盘的数据(同一个复盘日)。
 * 红涨绿跌;全球指数也沿用红涨(和本产品一致,不是国际惯例)。
 */
import { Activity, Coins, Gauge, Globe, TrendingUp } from "lucide-react";

import { fmtNumber, fmtPercent, fmtYuan } from "./discoveryData";
import { fmtRate, useCachedResource, useEmotion, type EmotionData } from "./emotionData";
import { ReviewDateBar } from "./HistoryPanels";
import "./emotion.css";
import "./monitor.css";

type Maybe<T> = ({ available: true } & T) | { available: false; reason: string };
export interface TrendRow {
  code: string; name: string; source: string; date: string; close: number; chg1: number | null; chg5: number | null; chg20: number | null; chg60: number | null; chg250: number | null; ytd: number | null;
  ma20: number | null; ma60: number | null; ma250: number | null; above_ma20: boolean | null; above_ma60: boolean | null; above_ma250: boolean | null;
  high250: number; low250: number; from_high_pct: number; from_low_pct: number; window_full: boolean; rows: number;
}
export interface AmountData { date: string; last: number; partial: boolean; prev: number | null; ma20: number | null; ma60: number | null; vs_ma20: number | null; vs_ma60: number | null; rows: { date: string; total: number }[]; source: string }
export interface MarginData { date: string; balance: number; chg5: number | null; chg20: number | null; net_buy5: number | null; float_pct: number | null; financing_balance: number | null; rows: { date: string; balance: number }[]; note: string }
export interface BondData { date: string; y3m: number | null; y1: number | null; y3: number | null; y5: number | null; y10: number | null; y30: number | null; curve: string; note: string }
export interface FlowRow { code: string; name: string; change_pct: number | null; net: number | null; inflow: number | null; outflow: number | null; leader: string; leader_pct: number | null }
export interface TurnoverRow { rank: number; code: string; name: string; price: number | null; pct: number | null; amount: number | null; mcap: number | null }
export interface DragonRow { code: string; name: string; reason: string; change_pct: number | null; net_buy_wan: number | null; buy_wan: number | null; sell_wan: number | null }
export interface MarketPanelData {
  date: string; settled: boolean; partial: boolean; today: string; notes: string[]; source: string;
  trend: Maybe<{ partial: boolean; rows: TrendRow[]; failed: { code: string; name: string; error: string }[]; above: Record<"ma20" | "ma60" | "ma250", { above: number; total: number }> }>;
  amount: Maybe<AmountData>; margin: Maybe<MarginData>; bond: Maybe<BondData>; valuation: Maybe<Record<string, never>>;
  global: Maybe<{ rows: { symbol: string; name: string; area: string; price: number | null; pct: number | null; time: string | null; available: boolean }[]; note: string }>;
  board_flow: Maybe<{ total: number; top: FlowRow[]; bottom: FlowRow[]; matches_review_day: boolean; source: string; note: string }>;
  turnover: Maybe<{ rows: TurnoverRow[]; matches_review_day: boolean; source: string; note: string }>;
  dragon: Maybe<{ date: string; total: number; truncated: boolean; top_buy: DragonRow[]; top_sell: DragonRow[]; note: string }>;
  breadth: Maybe<{ matches_review_day: boolean; trading_date: string; exchange: { up: number | null; down: number | null; flat: number | null; amount_yuan: number | null } }>;
}

const tone = (v: number | null | undefined) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const wan = (v: number | null | undefined) => (v == null ? "—" : fmtYuan(v * 1e4));
const signedYi = (v: number | null | undefined) => (v == null ? "—" : `${v > 0 ? "+" : ""}${fmtYuan(v)}`);

function Card({ title, icon, hint, children, aria }: { title: string; icon?: React.ReactNode; hint?: string; children: React.ReactNode; aria?: string }) {
  return <section className="watch-card" aria-label={aria ?? title}><div className="watch-card-head"><h2>{icon}{title}</h2></div>{hint && <p className="watch-hint">{hint}</p>}{children}</section>;
}
function Cell({ k, v, sub, tone: t }: { k: string; v: React.ReactNode; sub?: React.ReactNode; tone?: string }) {
  return <div className="emo-stat"><span>{k}</span><strong className={t}>{v}</strong>{sub != null && <small>{sub}</small>}</div>;
}
function Gap({ reason }: { reason: string }) {
  return <p className="watch-empty" role="status">这次没取到:{reason}</p>;
}
function Stale({ show }: { show: boolean }) {
  return show ? <em className="mp-stale">不是复盘日的数据</em> : null;
}

// ------------------------------------------------------------ 市场状态八格

export function StateGrid({ d, emotion }: { d: MarketPanelData; emotion: EmotionData | null }) {
  const b = d.breadth.available ? d.breadth : null;
  const x = b?.exchange;
  const a = d.amount.available ? d.amount : null;
  const s = emotion?.summary;
  const me = emotion?.money_effect.available ? emotion.money_effect : null;
  const promo = emotion?.promotion.available ? emotion.promotion.overall : null;
  const t = d.trend.available ? d.trend : null;
  const m = d.margin.available ? d.margin : null;
  const bond = d.bond.available ? d.bond : null;
  const share = x && x.up != null && x.down != null && x.up + x.down > 0 ? x.up / (x.up + x.down + (x.flat ?? 0)) : null;
  const above = (k: "ma20" | "ma60" | "ma250") => (t ? `${t.above[k].above}/${t.above[k].total}` : "—");
  return <div className="emo-stats emo-stats-4" aria-label="市场状态">
    <Cell k="涨跌面" v={x && x.up != null ? <><span className="up">{x.up}</span> / <span className="down">{x.down ?? "—"}</span></> : "—"}
      sub={x && x.up != null ? <>上涨占比 {fmtRate(share)}<Stale show={!!b && !b.matches_review_day} /></> : "没取到(盘前、休市或接口异常)"} />
    <Cell k={`两市成交额${a ? ` · ${a.date.slice(5)}` : ""}${a?.partial ? " · 盘中累计" : ""}`} v={a ? fmtYuan(a.last) : "—"}
      sub={!a ? "没取到" : a.partial ? `还没收完 · 上一个交易日 ${fmtYuan(a.prev)}` : <>比 20 日均 <b className={tone(a.vs_ma20)}>{fmtPercent(a.vs_ma20)}</b> · 60 日均 <b className={tone(a.vs_ma60)}>{fmtPercent(a.vs_ma60)}</b></>} />
    <Cell k="涨停 / 跌停" v={s ? <><span className="up">{s.limit_up}</span> / <span className="down">{s.limit_down ?? "—"}</span></> : "—"} sub={s ? `炸板 ${s.broken ?? "—"} · 封板率 ${fmtRate(s.seal_rate)}` : "情绪数据没取到"} />
    <Cell k="连板高度" v={s ? `${s.highest} 板` : "—"} sub={s ? `2 板及以上 ${s.consec_count} 只` : undefined} />
    <Cell k="昨日涨停今日" v={me ? fmtPercent(me.avg) : "—"} tone={tone(me?.avg)} sub={me ? `平均涨跌 · 翻红 ${fmtRate(me.positive_rate)} · 晋级 ${fmtRate(promo?.rate)}` : "没取到"} />
    <Cell k="站上 20 日线的指数" v={above("ma20")} sub={`站上 60 日线 ${above("ma60")} · 250 日线 ${above("ma250")}`} />
    <Cell k={`两融余额${m ? ` · ${m.date.slice(5)}` : ""}`} v={m ? fmtYuan(m.balance) : "—"} sub={m ? <>比 5 个交易日前 <b className={tone(m.chg5)}>{signedYi(m.chg5)}</b></> : "没取到"} />
    <Cell k={`10 年国债${bond ? ` · ${bond.date.slice(5)}` : ""}`} v={bond?.y10 != null ? `${bond.y10}%` : "—"} sub="沪深 300 市盈率 / 股息率暂无(见下)" />
  </div>;
}

// ------------------------------------------------------------ 主要指数

function IndexTable({ t }: { t: Extract<MarketPanelData["trend"], { available: true }> }) {
  const cols: [string, (r: TrendRow) => number | null][] = [["当日", (r) => r.chg1], ["5日", (r) => r.chg5], ["20日", (r) => r.chg20], ["60日", (r) => r.chg60], ["250日", (r) => r.chg250], ["年初至今", (r) => r.ytd], ["离高点", (r) => r.from_high_pct]];
  const flag = (r: TrendRow, k: "above_ma20" | "above_ma60" | "above_ma250") => (r[k] == null ? "—" : r[k] ? "上" : "下");
  return <>
    <div className="intra-table-wrap"><table className="intra-table" aria-label="主要指数中长线">
      <thead><tr><th>指数</th><th>收盘</th>{cols.map(([n]) => <th key={n}>{n}</th>)}<th>20/60/250 线</th></tr></thead>
      <tbody>{t.rows.map((r) => <tr key={r.code}><td><b>{r.name}</b></td><td>{fmtNumber(r.close, 2)}</td>
        {cols.map(([n, f]) => <td key={n} className={tone(f(r))}>{fmtPercent(f(r))}</td>)}
        <td>{flag(r, "above_ma20")} / {flag(r, "above_ma60")} / {flag(r, "above_ma250")}</td></tr>)}</tbody>
    </table></div>
    {t.failed.length > 0 && <p className="intra-warn">没取到:{t.failed.map((f) => `${f.name}(${f.error})`).join(";")}</p>}
    {t.rows.some((r) => !r.window_full) && <p className="watch-hint">日线不足 250 根的指数,250 日的读数和均线是空的(不用短窗口顶替)。</p>}
  </>;
}

// ------------------------------------------------------------ 成交额与两融走势

export function MiniBars({ points, label }: { points: { date: string; v: number }[]; label: string }) {
  if (points.length < 2) return null;
  const max = Math.max(...points.map((p) => p.v)), min = Math.min(...points.map((p) => p.v)), span = max - min || 1;
  return <div className="mp-bars" role="img" aria-label={label}>{points.map((p) => <i key={p.date} title={`${p.date} ${fmtYuan(p.v)}`} style={{ height: `${12 + ((p.v - min) / span) * 88}%` }} />)}</div>;
}

// ------------------------------------------------------------ 整页

export function MarketPanelView({ data: d, emotion, onOpenSymbol }: { data: MarketPanelData; emotion: EmotionData | null; onOpenSymbol: (symbol: string) => void }) {
  const a = d.amount.available ? d.amount : null;
  const m = d.margin.available ? d.margin : null;
  return <div className="watch-live">
    {d.notes.map((n) => <p key={n} className="intra-warn" role="status">{n}</p>)}
    <Card title="市场状态" icon={<Gauge size={15} />} hint="一眼看全:涨跌面 · 量能 · 短线 · 中长线位置 · 杠杆 · 股债收益率">
      <StateGrid d={d} emotion={emotion} />
    </Card>
    <Card title="主要指数" icon={<TrendingUp size={15} />} hint={`截至 ${d.date}${d.partial ? "(盘中,收盘前还会变)" : ""} · 按收盘价;20 / 60 / 250 日线是收盘价简单均线,只说位置`}>
      {d.trend.available ? <IndexTable t={d.trend} /> : <Gap reason={d.trend.reason} />}
    </Card>
    <Card title="中长线" icon={<Activity size={15} />} hint="趋势 · 量能 · 杠杆 · 股债收益率;都是按固定公式算的事实,不是趋势判断">
      <h3 className="watch-sub">两市成交额(亿元,近 60 日)</h3>
      {a ? <><MiniBars points={a.rows.map((r) => ({ date: r.date, v: r.total }))} label="两市成交额近 60 日" />
        <p className="smp-line"><span>{a.date.slice(5)} <b>{fmtYuan(a.last)}</b>{a.partial ? "(盘中累计)" : ""}</span><span>20 日均 <b>{fmtYuan(a.ma20)}</b></span><span>60 日均 <b>{fmtYuan(a.ma60)}</b></span>
          <span>{a.source}</span></p></> : <Gap reason={d.amount.available ? "" : d.amount.reason} />}
      <h3 className="watch-sub">两融余额(近 60 日)</h3>
      {m ? <><MiniBars points={m.rows.map((r) => ({ date: r.date, v: r.balance }))} label="两融余额近 60 日" />
        <p className="smp-line"><span>{m.date.slice(5)} <b>{fmtYuan(m.balance)}</b></span><span>比 5 日前 <b className={tone(m.chg5)}>{signedYi(m.chg5)}</b></span><span>比 20 日前 <b className={tone(m.chg20)}>{signedYi(m.chg20)}</b></span>
          <span>近 5 日融资净买入 <b className={tone(m.net_buy5)}>{signedYi(m.net_buy5)}</b></span><span>融资余额占流通市值 {m.float_pct == null ? "—" : `${m.float_pct}%`}</span></p>
        <p className="watch-hint">{m.note}</p></> : <Gap reason={d.margin.available ? "" : d.margin.reason} />}
      <h3 className="watch-sub">国债收益率与沪深 300 估值</h3>
      {d.bond.available ? <p className="smp-line"><span>{d.bond.date.slice(5)} 10 年 <b>{d.bond.y10 ?? "—"}%</b></span><span>1 年 {d.bond.y1 ?? "—"}%</span><span>3 年 {d.bond.y3 ?? "—"}%</span><span>5 年 {d.bond.y5 ?? "—"}%</span><span>30 年 {d.bond.y30 ?? "—"}%</span></p> : <Gap reason={d.bond.reason} />}
      {!d.valuation.available && <p className="watch-hint" role="status">{d.valuation.reason}。</p>}
    </Card>
    <Card title="全球市场" icon={<Globe size={15} />} hint="美股收盘与港股最新读数;红涨绿跌">
      {d.global.available ? <div className="watch-list" aria-label="全球指数">{d.global.rows.map((r) => <div key={r.symbol} className="watch-row mp-static">
        <span className="watch-name"><b>{r.name}</b><small>{r.area}{r.time ? ` · ${r.time}` : ""}</small></span>
        <span className="watch-quote"><strong>{r.price == null ? "没取到" : fmtNumber(r.price, 2)}</strong><span className={tone(r.pct)}>{fmtPercent(r.pct)}</span></span></div>)}</div> : <Gap reason={d.global.reason} />}
    </Card>
    <Card title="资金去了哪" icon={<Coins size={15} />} hint="行业资金 · 成交额居前 · 龙虎榜;客观公开榜单,非推荐 / 非预测">
      <h3 className="watch-sub">行业资金 <Stale show={d.board_flow.available && !d.board_flow.matches_review_day} /></h3>
      {d.board_flow.available ? <>
        <p className="watch-hint">{d.board_flow.note}</p>
        <div className="intra-table-wrap"><table className="intra-table" aria-label="行业资金净流入居前">
          <thead><tr><th>净流入居前</th><th>净额</th><th>涨跌</th><th>领涨</th></tr></thead>
          <tbody>{d.board_flow.top.map((r) => <tr key={r.code || r.name}><td><b>{r.name}</b></td><td className="up">{signedYi(r.net)}</td><td className={tone(r.change_pct)}>{fmtPercent(r.change_pct)}</td><td>{r.leader || "—"}</td></tr>)}</tbody></table></div>
        <div className="intra-table-wrap"><table className="intra-table" aria-label="行业资金净流出居前">
          <thead><tr><th>净流出居前</th><th>净额</th><th>涨跌</th><th>领涨</th></tr></thead>
          <tbody>{d.board_flow.bottom.map((r) => <tr key={r.code || r.name}><td><b>{r.name}</b></td><td className="down">{signedYi(r.net)}</td><td className={tone(r.change_pct)}>{fmtPercent(r.change_pct)}</td><td>{r.leader || "—"}</td></tr>)}</tbody></table></div>
      </> : <Gap reason={d.board_flow.reason} />}
      <h3 className="watch-sub">全市场成交额居前 20 <Stale show={d.turnover.available && !d.turnover.matches_review_day} /></h3>
      {d.turnover.available ? <div className="watch-list" aria-label="成交额居前 20">{d.turnover.rows.map((r) =>
        <button type="button" key={r.code} className="watch-row" onClick={() => onOpenSymbol(r.code)}>
          <span className="watch-lead"><em className="watch-rank">{r.rank}</em></span>
          <span className="watch-name"><b>{r.name}</b><small>{r.code} · 成交 {fmtYuan(r.amount)}</small></span>
          <span className="watch-quote"><strong>{r.price == null ? "—" : fmtNumber(r.price, 2)}</strong><span className={tone(r.pct)}>{fmtPercent(r.pct)}</span></span></button>)}</div> : <Gap reason={d.turnover.reason} />}
      <h3 className="watch-sub">龙虎榜 · {d.dragon.available ? d.dragon.date : d.date}</h3>
      {d.dragon.available ? <>
        <p className="watch-hint">共 {d.dragon.total} 条上榜记录{d.dragon.truncated ? "(只取了前几页)" : ""}。{d.dragon.note}</p>
        <div className="watch-list" aria-label="龙虎榜净买居前">{d.dragon.top_buy.map((r, i) =>
          <button type="button" key={`${r.code}-${i}`} className="watch-row" onClick={() => onOpenSymbol(r.code)}>
            <span className="watch-name"><b>{r.name}</b><small>{r.code} · {r.reason}</small></span>
            <span className="watch-quote"><strong className={tone(r.net_buy_wan)}>{wan(r.net_buy_wan)}</strong><span className={tone(r.change_pct)}>{fmtPercent(r.change_pct)}</span></span></button>)}</div>
        {d.dragon.top_sell.length > 0 && <><h3 className="watch-sub">净卖出居前</h3>
          <div className="watch-list" aria-label="龙虎榜净卖居前">{d.dragon.top_sell.map((r, i) =>
            <button type="button" key={`${r.code}-s${i}`} className="watch-row" onClick={() => onOpenSymbol(r.code)}>
              <span className="watch-name"><b>{r.name}</b><small>{r.code} · {r.reason}</small></span>
              <span className="watch-quote"><strong className={tone(r.net_buy_wan)}>{wan(r.net_buy_wan)}</strong><span className={tone(r.change_pct)}>{fmtPercent(r.change_pct)}</span></span></button>)}</div></>}
      </> : <Gap reason={d.dragon.reason} />}
    </Card>
    <p className="monitor-note">数据源:{d.source}。取不到的显示原因,不当成 0。AI 复盘和对话由「AI 助手」页单独提供。</p>
  </div>;
}

export function MarketPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const emotion = useEmotion();
  const result = useCachedResource<MarketPanelData>(`/mobile/review/market${emotion.date ? "?date=" + emotion.date : ""}`);
  return <section className="mobile-section emo-page">
    <ReviewDateBar title="市场面板" detail="每日复盘的市场状态、主要指数中长线、成交额与两融、全球市场和资金去向" loading={result.loading} error={result.error}
      onRefresh={() => { result.refresh(); emotion.refresh(); }} shownDate={result.data?.date} />
    {result.data && <MarketPanelView data={result.data} emotion={emotion.data} onOpenSymbol={onOpenSymbol} />}
  </section>;
}
