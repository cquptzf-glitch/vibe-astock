import { useEffect, useState } from "react";
import { priv, usePrivacyMode } from "./privacy";
interface WeeklyJournal { start:string;end:string;buys:number;sells:number;openings:number;splits:number;buy_amount:number;sell_amount:number;closed:number;planned:number;unplanned:number;performance:{net_pnl:number|null;win_rate:number|null};note:string }
export function JournalWeeklyView({data,hide=false}:{data:WeeklyJournal;hide?:boolean}) {
  const money=(n:number|null)=>hide?"••••":n==null?"—":n.toLocaleString("zh-CN",{maximumFractionDigits:2});
  return <section className="emo-card" data-private="true" aria-label="本周我的交易"><h2>本周我的交易</h2><small>{data.start} — {data.end}</small>
    <p>买入 {data.buys} 笔 · 卖出 {data.sells} 笔 · 清仓 {data.closed} 轮</p><p>买入金额 {money(data.buy_amount)} · 卖出金额 {money(data.sell_amount)} · 已实现净盈亏 {money(data.performance.net_pnl)}</p>
    <p>按计划 {data.planned} 轮 · 计划外 {data.unplanned} 轮 · 期初 {data.openings} 笔 · 送转 {data.splits} 笔</p><small>{data.note}</small></section>;
}
export function JournalWeekly({date}:{date?:string|null}) {
  const [data,setData]=useState<WeeklyJournal|null>(null);const [error,setError]=useState("");const hide=usePrivacyMode().on;
  useEffect(()=>{let active=true;setData(null);setError("");void fetch(`/api/mobile/journal/weekly${date?`?date=${encodeURIComponent(date)}`:""}`).then(async r=>{const d=await r.json();if(!r.ok)throw new Error(d.message??"本周交易读取失败");if(active)setData(d);}).catch(e=>{if(active)setError(String(e));});return()=>{active=false;};},[date]);
  return data?<JournalWeeklyView data={data} hide={hide}/>:error?<p data-private="true" role="alert">{priv(error)}</p>:null;
}
