import { usePageActive } from "./PageActivity.ts";
import { useEffect, useRef, useState } from "react";
import { WebSelect } from "./controls";
import { fmtNumber, fmtPercent } from "./discoveryData";
type ScanRow = { code: string; name: string; price: number | null; pct: number | null; surge_pct: number | null };
interface ScanResult { total: number; matched: number; missing: number; rows: ScanRow[]; gaps: { reason: string }[]; note: string; sampled_at: string }
interface NativeStatus { supported: boolean; running: boolean; message: string; bytes: number; alerts: { code: string; name: string; price: number; change_pct: number; time: string }[] }
async function api<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api${path}`, { method: body === undefined ? "GET" : "POST", headers: { "Content-Type": "application/json" }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
  const result = await response.json();
  if (!response.ok) throw new Error(result.message ?? `HTTP ${response.status}`);
  return result as T;
}
export function ScanResults({ data, onOpen }: { data: ScanResult; onOpen: (code: string) => void }) {
  return <div><p>已扫描 {data.total} 只 · 满足 {data.matched} 只 · 缺字段 {data.missing} 只</p>{data.rows.map((r) => <button className="watch-row" type="button" key={r.code} onClick={() => onOpen(r.code)}><span>{r.name} {r.code}</span><span>{fmtNumber(r.price)} · 今日 {fmtPercent(r.pct)} · 采样变化 {fmtPercent(r.surge_pct)}</span></button>)}{data.gaps.map((g, i) => <p role="status" key={i}>{g.reason}</p>)}<p className="watch-hint">{data.note}</p></div>;
}
export function MarketScannerPanel({ onOpen }: { onOpen: (code: string) => void }) {
  const active = usePageActive();
  const [scope, setScope] = useState("watch");
  const [min, setMin] = useState(""); const [max, setMax] = useState(""); const [amount, setAmount] = useState(""); const [surge, setSurge] = useState("");
  const [fall,setFall]=useState("");
  const [data, setData] = useState<ScanResult | null>(null);
  const [native, setNative] = useState<NativeStatus | null>(null);
  const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  const [auto, setAuto] = useState(false); const [interval, setIntervalSeconds] = useState(60); const [budget, setBudget] = useState(50); const [battery, setBattery] = useState(15);
  const running = useRef(false);
  async function scan() {
    if (running.current) return;
    running.current = true; setBusy(true); setError("");
    try { if ([min,max,amount,surge,fall].some(v=>v.trim()&&!Number.isFinite(Number(v)))) throw new Error("筛选条件必须是有效数字"); setData(await api<ScanResult>("/mobile/scan", { scope, min_pct: min ? Number(min) : null, max_pct: max ? Number(max) : null, min_amount: amount ? Number(amount) * 100000000 : null, min_surge: surge ? Number(surge) : null, max_surge: fall ? Number(fall) : null })); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { running.current = false; setBusy(false); }
  }
  useEffect(() => {
    if (!auto || !active) return;
    const timer = window.setInterval(() => { if (document.visibilityState === "visible") void scan(); }, interval * 1000);
    return () => window.clearInterval(timer);
  }, [active, auto, interval, scope, min, max, amount, surge, fall]);
  async function nativeAction(action: string) {
    setBusy(true); setError("");
    try { setNative(await api<NativeStatus>("/mobile/monitor/background", { action, scope, interval_seconds: interval, threshold_pct: Math.abs(Number(surge) || 1.5), max_bytes_mb: budget, stop_battery_pct: battery })); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  return <details className="watch-card"><summary>批量条件扫描 / 持续后台盯盘</summary>
    <div className="rw-form"><label>范围<WebSelect value={scope} onChange={(e) => setScope(e.target.value)}><option value="watch">自选股票</option><option value="all">全市场 A 股</option></WebSelect></label>
      <label>今日涨跌幅下限 %<input inputMode="decimal" value={min} onChange={(e) => setMin(e.target.value)} /></label><label>今日涨跌幅上限 %<input inputMode="decimal" value={max} onChange={(e) => setMax(e.target.value)} /></label>
      <label>成交额下限(亿元)<input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} /></label><label>采样变化下限 % / 后台异动幅度<input inputMode="decimal" value={surge} onChange={(e) => setSurge(e.target.value)} placeholder="后台默认 ±1.5%" /></label>
      <label>采样变化上限 % (例如 -1.5 筛急跌)<input inputMode="decimal" value={fall} onChange={e=>setFall(e.target.value)}/></label>
      <label>采样间隔<WebSelect value={interval} onChange={(e) => setIntervalSeconds(Number(e.target.value))}><option value={30}>30 秒</option><option value={60}>60 秒</option><option value={180}>180 秒</option><option value={300}>300 秒</option></WebSelect></label>
      <button className="discovery-action" type="button" disabled={busy} onClick={() => void scan()}>{busy ? "正在处理…" : "扫描一次"}</button>
      <label><input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} /> 前台定时扫描</label>
    </div>
    {error && <p role="alert">{error}</p>}{data && <ScanResults data={data} onOpen={onOpen} />}
    <h3>Android 持续后台盯盘</h3><p className="watch-hint">主动开启后显示常驻通知，按交易时段持续采样价格，识别一分钟至三分钟的变化；每只同方向五分钟内不重复提醒。全市场订阅流量较大，可设置本次上限和低电量停止线。按官方月历跳过休市；跨月或月历失效后停止，需重新开启以刷新。系统终止或重启后需重新开启。</p>
    <div className="rw-form"><label>本次下载流量上限 MB<input inputMode="numeric" value={budget} onChange={(e) => setBudget(Number(e.target.value))} /></label><label>电量低于此百分比停止<input inputMode="numeric" value={battery} onChange={(e) => setBattery(Number(e.target.value))} /></label></div>
    <div className="rw-actions"><button className="discovery-action" type="button" disabled={busy} onClick={() => void nativeAction("start")}>开启后台盯盘</button><button className="discovery-action" type="button" onClick={() => void nativeAction("stop")}>停止</button><button className="discovery-action" type="button" onClick={() => void nativeAction("status")}>读取状态与异动</button></div>
    {native && <div><p>{native.supported ? native.running ? "服务正在运行" : "服务未运行" : "当前环境不支持原生服务"} · {native.message} · {fmtNumber(native.bytes / 1024)} KiB</p>{native.alerts?.slice(0, 20).map((a, i) => <button className="watch-row" key={i} type="button" onClick={() => onOpen(a.code)}>{a.name} {a.code} · {fmtPercent(a.change_pct)} · {a.time}</button>)}</div>}
  </details>;
}
