/**
 * 图片复盘分享卡:把某一天的情绪读数画成一张图,预览后再分享。
 * 只放市场读数(涨停、炸板、跌停、最高板、炸板率、昨日涨停股今日表现、晋级率),不含自选、持仓、成本、盈亏;
 * 不放判断、不给买卖建议 —— 卡片会被转发,离开了页面的口径说明,一句判断就成了没出处的断言。
 */
import { Image as ImageIcon, Share2 } from "lucide-react";
import { useState } from "react";

import type { EmotionData } from "./emotionData";
import { shareFile } from "./nativeShare";

export interface CardRow { label: string; value: string }

const n = (v: number | null | undefined, digits = 0) => (v == null || !Number.isFinite(v) ? "—" : v.toLocaleString("zh-CN", { maximumFractionDigits: digits }));
const rate = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : `${Math.round(v * 1000) / 10}%`);

/** 卡片上的几行读数:只取数据里已有的,没有的写 —,不补 0 */
export function cardRows(data: EmotionData): CardRow[] {
  const s = data.summary;
  const rows: CardRow[] = [
    { label: "涨停", value: n(s.limit_up) }, { label: "炸板", value: n(s.broken) }, { label: "跌停", value: n(s.limit_down) },
    { label: "最高板", value: s.highest ? `${s.highest} 板` : "—" }, { label: "炸板率", value: rate(s.broken_rate) },
  ];
  const m = data.money_effect;
  if (m.available) rows.push({ label: "昨日涨停今日均涨跌", value: `${n(m.avg, 2)}%` }, { label: "昨日涨停翻红率", value: rate(m.positive_rate) });
  const p = data.promotion;
  if (p.available) rows.push({ label: "整体晋级率", value: rate(p.overall.rate) });
  return rows;
}

export const CARD_WIDTH = 720;
export const CARD_FOOTER = "Vibe AStock · 只列已经发生的盘面读数,不构成买卖建议";

/** 画一张「标题 + 日期行 + 若干行读数」的卡到 canvas(浏览器里才有);返回 PNG 的 base64,画不出来返回 null。右侧的值太长时自动缩小字号 */
export function drawRowsCard(title: string, subtitle: string, rows: CardRow[], dark = false): string | null {
  if (typeof document === "undefined") return null;
  const canvas = document.createElement("canvas");
  const rowH = 74;
  canvas.width = CARD_WIDTH;
  canvas.height = 190 + rows.length * rowH + 90;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  const bg = dark ? "#0d131b" : "#ffffff", fg = dark ? "#edf2f8" : "#172536", muted = dark ? "#a1adbd" : "#536477", line = dark ? "#334154" : "#d2dce7", accent = dark ? "#ff9b70" : "#ad3c19";
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = accent;
  ctx.fillRect(0, 0, canvas.width, 10);
  ctx.fillStyle = fg;
  ctx.font = "bold 44px sans-serif";
  ctx.fillText(title, 40, 92);
  ctx.fillStyle = muted;
  ctx.font = "28px sans-serif";
  ctx.fillText(subtitle, 40, 140);
  rows.forEach((row, i) => {
    const y = 190 + i * rowH;
    ctx.strokeStyle = line;
    ctx.beginPath();
    ctx.moveTo(40, y);
    ctx.lineTo(canvas.width - 40, y);
    ctx.stroke();
    ctx.fillStyle = muted;
    ctx.font = "28px sans-serif";
    ctx.fillText(row.label, 40, y + 48);
    ctx.fillStyle = fg;
    let size = 36;
    ctx.font = `bold ${size}px sans-serif`;
    while (size > 20 && ctx.measureText(row.value).width > 400) {
      size -= 2;
      ctx.font = `bold ${size}px sans-serif`;
    }
    ctx.textAlign = "right";
    ctx.fillText(row.value, canvas.width - 40, y + 50);
    ctx.textAlign = "left";
  });
  ctx.fillStyle = muted;
  ctx.font = "22px sans-serif";
  ctx.fillText(CARD_FOOTER, 40, canvas.height - 36);
  const url = canvas.toDataURL("image/png");
  const comma = url.indexOf(",");
  return comma > 0 ? url.slice(comma + 1) : null;
}

export function renderCard(data: EmotionData, dark = false): string | null {
  return drawRowsCard("情绪复盘", `${data.date}${data.settled ? "" : "(未收盘,读数随盘中变化)"}`, cardRows(data), dark);
}

/** 连板天梯卡上的行:2 板以上的层,高板在前,最多 6 层;每层写只数和前几只的名字。数据不可用返回空数组 */
export function ladderRows(data: EmotionData): CardRow[] {
  const l = data.ladder;
  if (!l.available) return [];
  const levels = [...l.levels].filter((x) => x.boards >= 2).sort((a, b) => b.boards - a.boards).slice(0, 6);
  return [
    { label: "涨停 / 最高板", value: `${l.total_limit_up} 只 / ${l.highest} 板` },
    ...levels.map((x) => ({ label: `${x.boards} 板 · ${x.stocks.length} 只`, value: x.stocks.slice(0, 3).map((s) => s.name).join("、") + (x.stocks.length > 3 ? " 等" : "") })),
  ];
}

export function renderLadderCard(data: EmotionData, dark = false): string | null {
  const rows = ladderRows(data);
  return rows.length ? drawRowsCard("连板天梯", `${data.date}${data.settled ? "" : "(未收盘,读数随盘中变化)"}`, rows, dark) : null;
}

export function EmotionShareCard({ data }: { data: EmotionData }) {
  return <ShareCardBox data={data} kind="情绪复盘" slug="emotion" render={renderCard} />;
}

export function LadderShareCard({ data }: { data: EmotionData }) {
  return <ShareCardBox data={data} kind="连板天梯" slug="ladder" render={renderLadderCard} />;
}

function ShareCardBox({ data, kind, slug, render }: { data: EmotionData; kind: string; slug: string; render: (data: EmotionData, dark: boolean) => string | null }) {
  const [png, setPng] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const make = () => {
    const dark = typeof document !== "undefined" && document.documentElement.dataset.theme === "dark";
    const out = render(data, dark);
    setPng(out);
    setMessage(out ? "" : "当前环境画不出图片,或这一页没有可画的读数。");
  };
  const share = async () => {
    if (!png) return;
    try {
      const how = await shareFile({ name: `vibe-astock-${slug}-${data.date}.png`, mime: "image/png", base64: png, title: `${kind} ${data.date}`, chooserTitle: `分享${kind}图` });
      setMessage(how === "download" ? "已下载图片。" : "");
    } catch (e) { setMessage(e instanceof Error ? e.message : String(e)); }
  };
  return <section className="emo-card" aria-label="图片分享卡">
    <div className="emo-card-head"><h2><ImageIcon size={14} /> 图片分享卡</h2><small>只含市场读数,不含自选与持仓</small></div>
    {!png ? <button type="button" className="emo-link" onClick={make}>生成 {data.date} 的分享图</button>
      : <><img className="share-preview" alt={`${data.date} ${kind}分享图预览`} src={`data:image/png;base64,${png}`} />
        <div className="share-actions"><button type="button" onClick={() => void share()}><Share2 size={14} /> 分享这张图</button><button type="button" onClick={() => setPng(null)}>重画</button></div></>}
    {message && <p className="emo-foot" role="status">{message}</p>}
  </section>;
}
