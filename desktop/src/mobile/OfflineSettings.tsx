import { useEffect, useState } from "react";
export function OfflineSettings() {
  const [offline, setOffline] = useState(false); const [count, setCount] = useState(0); const [error, setError] = useState("");
  useEffect(() => { let current = true; void fetch("/api/mobile/cache").then(async (r) => { if (!r.ok) throw new Error("无法读取缓存设置"); const d = await r.json(); if (current) { setOffline(d.offline); setCount(d.cached_responses); } }).catch((e) => { if (current) setError(String(e)); }); return () => { current = false; }; }, []);
  async function toggle(value: boolean) { try { const r = await fetch("/api/mobile/cache", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ offline: value }) }); const d = await r.json(); if (!r.ok) throw new Error(d.message); setOffline(d.offline); setError(""); } catch (e) { setError(String(e)); } }
  return <section className="settings-card"><h2>公开数据缓存与离线阅读</h2><label><input type="checkbox" checked={offline} onChange={(e) => void toggle(e.target.checked)} /> 离线模式</label><p>已保存 {count} 份公开数据响应。离线时展示带时间标记的旧快照，暂停联网取数、AI 与持续后台盯盘；个人记录仍可在本机查看。未缓存的数据会说明原因。</p>{error && <p role="alert">{error}</p>}</section>;
}
