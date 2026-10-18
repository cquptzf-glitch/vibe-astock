import { useState } from "react";
import { DateField } from "./controls";
import { useDiscoveryData } from "./discoveryData";
import { chinaToday } from "./emotionData";
export interface MonthCalendar { month: string; days: { date: string; is_open: boolean }[]; source: string; note: string }
export function ExchangeMonthView({ data }: { data: MonthCalendar }) {
  const start = data.days[0]?.date;
  const offset = start ? (new Date(`${start}T00:00:00Z`).getUTCDay() + 6) % 7 : 0;
  return <div><p>{data.month} · {data.source}</p><div className="exchange-month">{["一", "二", "三", "四", "五", "六", "日"].map((d) => <span key={d}>{d}</span>)}{Array.from({ length: offset }, (_, i) => <span key={`gap-${i}`} />)}{data.days.map((d) => <span key={d.date} className={d.is_open ? "open" : "closed"}><strong>{Number(d.date.slice(-2))}</strong><small>{d.is_open ? "开市" : "休市"}</small></span>)}</div><p>{data.note}</p></div>;
}
export function ExchangeCalendar() {
  const [date, setDate] = useState(chinaToday);
  const result = useDiscoveryData<MonthCalendar>(`/mobile/calendar/month?month=${date.slice(0, 7)}`);
  return <details><summary>交易所开休市月历</summary><DateField value={date} onChange={(e) => setDate(e.target.value)} />{result.loading && <p>正在读取交易所月历…</p>}{result.error && <p role="alert">{result.error}</p>}{result.data && <ExchangeMonthView data={result.data} />}</details>;
}
