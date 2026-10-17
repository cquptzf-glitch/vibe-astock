/**
 * K 线技术指标,公式与常用证券软件(通达信 / 同花顺)一致:
 * - MA(N):收盘价 N 日简单平均,不足 N 根为 null
 * - MACD(12,26,9):DIF = EMA12 − EMA26,DEA = EMA(DIF, 9),柱 = 2 × (DIF − DEA)
 * - KDJ(9,3,3):RSV = (C − LLV(L,9)) / (HHV(H,9) − LLV(L,9)) × 100,K = SMA(RSV,3,1),D = SMA(K,3,1),J = 3K − 2D,K/D 初值 50
 * - RSI(N):SMA(MAX(C − C₁, 0), N, 1) / SMA(|C − C₁|, N, 1) × 100
 * - BOLL(20,2):中轨 MA20,上下轨 ± 2 × 总体标准差
 * 只是把已发生的价格换一种方式描述,不构成买卖信号。
 */
export interface Bar { time: string; open: number; close: number; high: number; low: number; volume: number; amount: number | null; turnover: number | null }
type Series = (number | null)[];

export function parseBars(rows: unknown[]): Bar[] {
  return rows.flatMap((row): Bar[] => {
    if (!Array.isArray(row) || row.length < 6) return [];
    const [time, open, close, high, low, volume, amount, turnover] = row;
    const n = [open, close, high, low, volume].map(Number);
    if (typeof time !== "string" || n.some((x) => !Number.isFinite(x))) return [];
    return [{ time, open: n[0]!, close: n[1]!, high: n[2]!, low: n[3]!, volume: n[4]!,
      amount: typeof amount === "number" && Number.isFinite(amount) ? amount : null,
      turnover: typeof turnover === "number" && Number.isFinite(turnover) ? turnover : null }];
  });
}

export function ma(values: number[], n: number): Series {
  const out: Series = [];
  let sum = 0;
  values.forEach((v, i) => {
    sum += v;
    if (i >= n) sum -= values[i - n]!;
    out.push(i >= n - 1 ? sum / n : null);
  });
  return out;
}

export function ema(values: number[], n: number): number[] {
  const k = 2 / (n + 1);
  const out: number[] = [];
  values.forEach((v, i) => out.push(i === 0 ? v : v * k + out[i - 1]! * (1 - k)));
  return out;
}

/** 通达信 SMA(X, N, M) = (M × X + (N − M) × 前值) / N,首值取 X */
function sma(values: number[], n: number, m: number, first?: number): number[] {
  const out: number[] = [];
  values.forEach((v, i) => out.push(i === 0 ? (first === undefined ? v : (m * v + (n - m) * first) / n) : (m * v + (n - m) * out[i - 1]!) / n));
  return out;
}

export function macd(closes: number[], fast = 12, slow = 26, signal = 9) {
  const a = ema(closes, fast), b = ema(closes, slow);
  const dif = closes.map((_, i) => a[i]! - b[i]!);
  const dea = ema(dif, signal);
  return { dif, dea, hist: dif.map((d, i) => 2 * (d - dea[i]!)) };
}

export function kdj(bars: Bar[], n = 9, m1 = 3, m2 = 3) {
  const rsv = bars.map((_, i) => {
    const win = bars.slice(Math.max(0, i - n + 1), i + 1);
    const hh = Math.max(...win.map((b) => b.high)), ll = Math.min(...win.map((b) => b.low));
    return hh === ll ? 50 : ((bars[i]!.close - ll) / (hh - ll)) * 100;
  });
  const k = sma(rsv, m1, 1, 50);
  const d = sma(k, m2, 1, 50);
  return { k, d, j: k.map((x, i) => 3 * x - 2 * d[i]!) };
}

export function rsi(closes: number[], n: number): Series {
  if (closes.length < 2) return closes.map(() => null);
  const up = closes.slice(1).map((c, i) => Math.max(c - closes[i]!, 0));
  const all = closes.slice(1).map((c, i) => Math.abs(c - closes[i]!));
  const a = sma(up, n, 1), b = sma(all, n, 1);
  return [null, ...a.map((x, i) => (b[i]! === 0 ? 50 : (x / b[i]!) * 100))];
}

export function boll(closes: number[], n = 20, width = 2) {
  const mid = ma(closes, n);
  const up: Series = [], low: Series = [];
  closes.forEach((_, i) => {
    const m = mid[i];
    if (m == null) { up.push(null); low.push(null); return; }
    const win = closes.slice(i - n + 1, i + 1);
    const sd = Math.sqrt(win.reduce((s, v) => s + (v - m) ** 2, 0) / n);
    up.push(m + width * sd);
    low.push(m - width * sd);
  });
  return { mid, up, low };
}

/** 一根 K 线相对前一根收盘的涨跌幅与振幅(%);第一根没有前收,返回 null */
export function barChange(bars: Bar[], i: number) {
  const prev = i > 0 ? bars[i - 1]!.close : null;
  const bar = bars[i]!;
  return {
    pct: prev ? (bar.close / prev - 1) * 100 : null,
    change: prev ? bar.close - prev : null,
    amplitude: prev ? ((bar.high - bar.low) / prev) * 100 : null,
  };
}

export function fmtPrice(v: number | null | undefined, digits = 2): string {
  return v == null || !Number.isFinite(v) ? "—" : v.toFixed(digits);
}

export function fmtLots(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  if (Math.abs(v) >= 1e8) return (v / 1e8).toFixed(2) + "亿手";
  if (Math.abs(v) >= 1e4) return (v / 1e4).toFixed(2) + "万手";
  return Math.round(v) + "手";
}

export function fmtMoney(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  if (Math.abs(v) >= 1e12) return (v / 1e12).toFixed(2) + "万亿";
  if (Math.abs(v) >= 1e8) return (v / 1e8).toFixed(2) + "亿";
  if (Math.abs(v) >= 1e4) return (v / 1e4).toFixed(1) + "万";
  return v.toFixed(0);
}

/** 分时横轴:09:30–11:30 与 13:00–15:00 共 242 个分钟槽 */
export const MINUTE_SLOTS = 242;
export function minuteSlot(time: string): number | null {
  const m = /^(\d{2}):(\d{2})$/.exec(time);
  if (!m) return null;
  const mins = Number(m[1]) * 60 + Number(m[2]);
  if (mins >= 570 && mins <= 690) return mins - 570;
  if (mins >= 780 && mins <= 900) return 121 + (mins - 780);
  return null;
}
