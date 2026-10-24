import { useEffect, useRef, useState, type RefObject } from "react";
import { Image as ImageIcon, Share2 } from "lucide-react";
import { renderChartCard, type ChartCardSpec, type ChartImage } from "./chartShare";
import { shareFile } from "./nativeShare";

export function ChartShareCard({ chartRef, spec, identity, slug }: { chartRef: RefObject<SVGSVGElement | null>; spec: ChartCardSpec; identity: unknown; slug: string }) {
  const [snapshot, setSnapshot] = useState<(ChartImage & { identity: unknown; title: string }) | null>(null);
  const [busy, setBusy] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [message, setMessage] = useState("");
  const generation = useRef(0);
  const currentIdentity = useRef(identity);
  currentIdentity.current = identity;
  useEffect(() => { generation.current++; setSnapshot(null); setBusy(false); setMessage(""); return () => { generation.current++; }; }, [identity]);
  const image = snapshot?.identity === identity ? snapshot : null;
  async function make() {
    const id = ++generation.current, capturedIdentity = identity;
    setBusy(true); setMessage("");
    try {
      if (!chartRef.current) throw new Error("当前没有可分享的图表。");
      const result = await renderChartCard(chartRef.current, spec);
      if (generation.current === id && currentIdentity.current === capturedIdentity) setSnapshot({ ...result, identity: capturedIdentity, title: spec.title });
    } catch (error) {
      if (generation.current === id) setMessage(error instanceof Error ? error.message : String(error));
    } finally { if (generation.current === id) setBusy(false); }
  }
  async function share() {
    if (!image || sharing) return;
    setSharing(true); setMessage("");
    try {
      const how = await shareFile({ name: `vibe-astock-chart-${slug.replace(/[^a-zA-Z0-9_-]/g, "-")}.png`, mime: "image/png", base64: image.base64, title: image.title, chooserTitle: "分享当前图表" });
      setMessage(how === "download" ? "已下载图表图片。" : "已打开系统分享。");
    } catch (error) { setMessage(error instanceof Error ? error.message : String(error)); }
    finally { setSharing(false); }
  }
  return <section className="chart-share-card" aria-label="当前图表图片分享" data-page-actions="true">
    <p><ImageIcon size={14} /> 当前图表分享 · 保留图形、显示区间、指标及来源</p>
    {!image ? <button type="button" disabled={busy} onClick={() => void make()}>{busy ? "正在生成图表…" : "生成当前图表分享图"}</button> : <>
      <img className="share-preview" alt={`${image.title}图表分享预览`} src={`data:image/png;base64,${image.base64}`} />
      <div className="share-actions"><button type="button" disabled={sharing} onClick={() => void share()}><Share2 size={14} />{sharing ? "正在分享…" : "分享这张图表"}</button><button type="button" onClick={() => setSnapshot(null)}>重画图表</button></div>
    </>}
    <small>仅导出这一张图表。数据、区间或指标改变后需重新生成；分享前请核对预览。</small>
    {message && <p role="status">{message}</p>}
  </section>;
}
