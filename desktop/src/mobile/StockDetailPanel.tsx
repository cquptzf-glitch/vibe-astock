import { Activity, BarChart3, Gauge, Share2, TrendingDown, TrendingUp } from "lucide-react";
import { useMemo } from "react";

import { shareText } from "./nativeShare";
import "./stock-detail.css";

interface QuoteLike {
  symbol?: string;
  market?: string;
  evidence: { field: string; value: number | string | null; unit: string }[];
  primary_source: string | null;
  extra?: Record<string, unknown>;
}

interface Bar {
  date: string;
  open: number;
  close: number;
  high: number;
  low: number;
  volume: number;
}

function parseBars(rows: unknown[][]): Bar[] {
  return rows.flatMap((row): Bar[] => {
    if (row.length < 6) return [];
    const [date, open, close, high, low, volume] = row;
    const openValue = Number(open);
    const closeValue = Number(close);
    const highValue = Number(high);
    const lowValue = Number(low);
    const volumeValue = Number(volume);
    if (
      typeof date !== "string"
      || !Number.isFinite(openValue)
      || !Number.isFinite(closeValue)
      || !Number.isFinite(highValue)
      || !Number.isFinite(lowValue)
      || !Number.isFinite(volumeValue)
    ) return [];
    return [{
      date,
      open: openValue,
      close: closeValue,
      high: highValue,
      low: lowValue,
      volume: volumeValue,
    }];
  });
}

function average(values: number[]): number | null {
  if (!values.length) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function tailAverage(bars: Bar[], count: number): number | null {
  return average(bars.slice(-count).map((bar) => bar.close));
}

function changeFrom(bars: Bar[], sessions: number): number | null {
  if (bars.length < 2) return null;
  const latest = bars[bars.length - 1]!.close;
  const index = Math.max(0, bars.length - 1 - sessions);
  const base = bars[index]?.close;
  if (!base) return null;
  return (latest / base - 1) * 100;
}

function signedPct(value: number | null, digits = 1): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return `${value > 0 ? "+" : ""}${value.toFixed(digits)}%`;
}

function price(value: number | null): string {
  return value === null || !Number.isFinite(value)
    ? "—"
    : value.toLocaleString("zh-CN", { maximumFractionDigits: 3 });
}

function volumeText(value: number): string {
  if (value >= 100_000_000) return `${(value / 100_000_000).toFixed(2)} 亿`;
  if (value >= 10_000) return `${(value / 10_000).toFixed(1)} 万`;
  return value.toLocaleString("zh-CN");
}

function CandleChart({ bars }: { bars: Bar[] }) {
  const visible = bars.slice(-30);
  if (visible.length < 2) return <div className="stock-detail-empty">K 线不足，暂时无法绘制蜡烛图</div>;
  const max = Math.max(...visible.map((bar) => bar.high));
  const min = Math.min(...visible.map((bar) => bar.low));
  const span = max - min || 1;
  const width = 300;
  const height = 124;
  const step = width / visible.length;
  const bodyWidth = Math.max(2.8, step * 0.58);
  const y = (value: number) => 7 + ((max - value) / span) * (height - 14);

  return (
    <svg className="stock-candles" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-label="近 30 日蜡烛图">
      {visible.map((bar, index) => {
        const x = index * step + step / 2;
        const openY = y(bar.open);
        const closeY = y(bar.close);
        const highY = y(bar.high);
        const lowY = y(bar.low);
        const rising = bar.close >= bar.open;
        return (
          <g key={`${bar.date}-${index}`} className={rising ? "candle-up" : "candle-down"}>
            <line x1={x} x2={x} y1={highY} y2={lowY} />
            <rect x={x - bodyWidth / 2} y={Math.min(openY, closeY)} width={bodyWidth} height={Math.max(1.6, Math.abs(closeY - openY))} rx="0.5" />
          </g>
        );
      })}
    </svg>
  );
}

export function StockDetailPanel({ quote, rows }: { quote: QuoteLike; rows: unknown[][] }) {
  const bars = useMemo(() => parseBars(rows), [rows]);
  const latest = bars.length ? bars[bars.length - 1]! : null;
  const stats = useMemo(() => {
    if (!bars.length) return null;
    const latestClose = bars[bars.length - 1]!.close;
    const ma5 = tailAverage(bars, 5);
    const ma10 = tailAverage(bars, 10);
    const ma20 = tailAverage(bars, 20);
    const last20 = bars.slice(-20);
    const high20 = Math.max(...last20.map((bar) => bar.high));
    const low20 = Math.min(...last20.map((bar) => bar.low));
    const range = high20 - low20;
    const rangePosition = range > 0 ? ((latestClose - low20) / range) * 100 : 50;
    const ma20Bias = ma20 ? (latestClose / ma20 - 1) * 100 : null;
    const trend = ma5 !== null && ma10 !== null && ma20 !== null
      ? ma5 > ma10 && ma10 > ma20
        ? "多头排列"
        : ma5 < ma10 && ma10 < ma20
          ? "空头排列"
          : "均线交织"
      : "数据不足";
    return {
      ma5,
      ma10,
      ma20,
      high20,
      low20,
      rangePosition,
      ma20Bias,
      trend,
      return5: changeFrom(bars, 5),
      return20: changeFrom(bars, 20),
      return60: changeFrom(bars, 60),
    };
  }, [bars]);

  const amount = quote.evidence.find((item) => item.field === "amount")?.value;
  const amountNumber = typeof amount === "number" ? amount : Number(amount);

  if (!latest || !stats) return null;
  const trendPositive = stats.trend === "多头排列";
  const trendNegative = stats.trend === "空头排列";
  const displayName = String(quote.extra?.name ?? quote.symbol ?? "个股");
  const symbol = quote.symbol ?? "";

  async function shareSnapshot() {
    const lines = [
      `【${displayName}${symbol ? ` ${symbol}` : ""}】`,
      `${quote.market ? `${quote.market} · ` : ""}${latest.date} K线收盘 ${price(latest.close)}`,
      `价格结构：${stats.trend}；距 MA20 ${signedPct(stats.ma20Bias)}`,
      `近5日 ${signedPct(stats.return5)} · 近20日 ${signedPct(stats.return20)} · 近60日 ${signedPct(stats.return60)}`,
      `MA5 ${price(stats.ma5)} · MA10 ${price(stats.ma10)} · MA20 ${price(stats.ma20)}`,
      `20日区间 ${price(stats.low20)} ~ ${price(stats.high20)}，当前位置 ${Math.round(stats.rangePosition)}%`,
      `数据源：${quote.primary_source ?? "—"}`,
      "",
      "来自 Vibe AStock Android，仅作行情记录与复盘，不构成投资建议。",
    ];
    try {
      await shareText({
        title: `${displayName}${symbol ? ` ${symbol}` : ""} · Vibe AStock`,
        text: lines.join("\n"),
        chooserTitle: "分享个股行情",
      });
    } catch (error) {
      window.alert(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <article className="stock-detail-card">
      <div className="stock-detail-head">
        <div>
          <span className="stock-detail-kicker"><BarChart3 size={14} /> 个股详情</span>
          <h2>价格结构 · 近 60 日观察</h2>
        </div>
        <div className="stock-detail-actions">
          <button className="stock-share-button" type="button" onClick={() => void shareSnapshot()} aria-label="分享个股行情">
            <Share2 size={14} />
          </button>
          <span className={`stock-trend-pill ${trendPositive ? "positive" : trendNegative ? "negative" : ""}`}>
            {trendPositive ? <TrendingUp size={13} /> : trendNegative ? <TrendingDown size={13} /> : <Activity size={13} />}
            {stats.trend}
          </span>
        </div>
      </div>

      <CandleChart bars={bars} />

      <div className="stock-detail-returns">
        <div><span>近 5 日</span><strong className={(stats.return5 ?? 0) >= 0 ? "up" : "down"}>{signedPct(stats.return5)}</strong></div>
        <div><span>近 20 日</span><strong className={(stats.return20 ?? 0) >= 0 ? "up" : "down"}>{signedPct(stats.return20)}</strong></div>
        <div><span>近 60 日</span><strong className={(stats.return60 ?? 0) >= 0 ? "up" : "down"}>{signedPct(stats.return60)}</strong></div>
      </div>

      <div className="stock-detail-grid">
        <div><span>今开</span><strong>{price(latest.open)}</strong></div>
        <div><span>最高</span><strong>{price(latest.high)}</strong></div>
        <div><span>最低</span><strong>{price(latest.low)}</strong></div>
        <div><span>成交量</span><strong>{volumeText(latest.volume)}</strong></div>
        <div><span>MA5</span><strong>{price(stats.ma5)}</strong></div>
        <div><span>MA10</span><strong>{price(stats.ma10)}</strong></div>
        <div><span>MA20</span><strong>{price(stats.ma20)}</strong></div>
        <div><span>距 MA20</span><strong className={(stats.ma20Bias ?? 0) >= 0 ? "up" : "down"}>{signedPct(stats.ma20Bias)}</strong></div>
      </div>

      <div className="stock-range-box">
        <div className="stock-range-label">
          <span><Gauge size={14} />20 日价格位置</span>
          <strong>{Math.round(stats.rangePosition)}%</strong>
        </div>
        <div className="stock-range-track"><i style={{ width: `${Math.min(100, Math.max(0, stats.rangePosition))}%` }} /></div>
        <div className="stock-range-foot"><span>低 {price(stats.low20)}</span><span>高 {price(stats.high20)}</span></div>
      </div>

      <p className="stock-detail-note">
        这里展示的是历史价格与均线位置，不是买卖建议。{Number.isFinite(amountNumber) ? ` 当前行情成交额字段：${amountNumber.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}。` : ""}
      </p>
    </article>
  );
}
