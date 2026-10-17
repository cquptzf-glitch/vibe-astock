import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/stock/{kline,intraday,snapshot} 对 600519 在 2026-10-16 收盘后的真实返回(日 K 裁剪到 160 根)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-chart-600519.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const near = (a: number | null | undefined, b: number, eps = 1e-6) => assert.ok(a != null && Math.abs(a - b) < eps, `${a} ≈ ${b}`);

test("指标公式与证券软件一致:MA、MACD、KDJ、RSI、BOLL", async () => {
  await withRenderer(async (r) => {
    const { ma, macd, kdj, rsi, boll, barChange, minuteSlot } = await r.load("/src/mobile/chartIndicators.ts");
    assert.deepEqual(ma([1, 2, 3, 4, 5], 3), [null, null, 2, 3, 4]);
    const closes = [10, 11, 12, 11, 13];
    const m = macd(closes, 2, 3, 2);
    // EMA2: k=2/3;EMA3: k=1/2
    near(m.dif[1], (11 * 2 / 3 + 10 / 3) - 10.5);
    near(m.hist[0], 0);
    const bars = closes.map((c, i) => ({ time: String(i), open: c, close: c, high: c + 1, low: c - 1, volume: 1, amount: null, turnover: null }));
    const k = kdj(bars, 3, 3, 3);
    // 第一根 RSV = (10-9)/(11-9) = 50 → K = (50 + 2×50)/3 = 50
    near(k.k[0], 50);
    near(k.j[0], 50);
    const r6 = rsi([10, 11, 10, 11], 2);
    assert.equal(r6[0], null);
    near(r6[1], 100);
    near(r6[2], 50);
    const b = boll([1, 1, 1, 1], 2, 2);
    near(b.up[1], 1);
    assert.equal(b.mid[0], null);
    const ch = barChange([{ ...bars[0]!, close: 10 }, { ...bars[1]!, close: 11, high: 11.5, low: 10.5 }], 1);
    near(ch.pct, 10);
    near(ch.amplitude, 10);
    assert.equal(barChange(bars, 0).pct, null);
    assert.deepEqual([minuteSlot("09:30"), minuteSlot("11:30"), minuteSlot("13:00"), minuteSlot("15:00"), minuteSlot("12:00")], [0, 120, 121, 241, null]);
  });
});

test("日 K:十项信息条、均线图例、高低点标注、缩放与回到最新按钮", async () => {
  await withRenderer(async (r) => {
    const { KlineView } = await r.load("/src/mobile/KlineChart.tsx");
    const html = r.html(KlineView, { data: fixture.kline, overlay: "ma", sub: "macd", width: 360, isMinute: false });
    const text = flat(html);
    for (const part of ["时间2026-10-16", "收1329.00", "涨幅+1.22%", "振幅2.15%", "量3.85万手", "额50.76亿", "换手0.31%", "MA5", "MA60", "MACD(12,26,9) DIF", "60 根 · "]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /aria-label="K 线图，显示 \d{4}-\d{2}-\d{2} 至 2026-10-16 共 60 根"/);
    assert.match(html, /← |→/);
    assert.match(html, /aria-label="缩小（显示更多 K 线）"/);
    assert.match(html, /aria-label="回到最新"[^>]*disabled/);
    const boll = flat(r.html(KlineView, { data: fixture.kline, overlay: "boll", sub: "kdj", width: 360, isMinute: false }));
    for (const part of ["BOLL(20,2)中", "KDJ(9,3,3)K"]) assert.ok(boll.includes(sq(part)), part);
    const m5 = flat(r.html(KlineView, { data: fixture.m5, overlay: "ma", sub: "vol", width: 360, isMinute: true }));
    assert.ok(m5.includes("VOL"));
    assert.match(m5, /时间\d{2}-\d{2}\d{2}:\d{2}/);
  });
});

test("分时:价格/均价/昨收基线与涨跌幅双轴,五日按天分隔", async () => {
  await withRenderer(async (r) => {
    const { IntradayView } = await r.load("/src/mobile/KlineChart.tsx");
    const html = r.html(IntradayView, { data: fixture.intraday, width: 360 });
    const text = flat(html);
    for (const part of ["时间15:00", "价格1329.00", "涨跌+1.22%", "均价1318.26", "昨收1312.97", "当日量3.85万手", "09:30", "11:30/13:00", "15:00"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /aria-label="分时图"/);
    const five = structuredClone(fixture.intraday);
    five.days = [{ ...five.days[0], date: "2026-10-15", pre_close: 1286 }, five.days[0]];
    const fiveHtml = r.html(IntradayView, { data: five, width: 360 });
    assert.match(fiveHtml, /aria-label="五日分时图"/);
    assert.ok(flat(fiveHtml).includes("10-15") && flat(fiveHtml).includes("10-16"));
    const empty = flat(r.html(IntradayView, { data: { ...fixture.intraday, days: [] }, width: 360 }));
    assert.ok(empty.includes("还没有分时数据"));
  });
});

test("盘口:二十项行情、外内盘比例、五档折叠与非今日提示", async () => {
  await withRenderer(async (r) => {
    const { QuoteBoardView } = await r.load("/src/mobile/QuoteBoard.tsx");
    const text = flat(r.html(QuoteBoardView, { data: fixture.snapshot }));
    for (const part of ["1329.00", "+16.03", "+1.22%", "非今日行情", "今开1311.57", "昨收1312.97", "最高1329.88", "最低1301.68", "成交量3.85万手", "成交额50.76亿",
      "量比1.10", "均价1318.26", "涨停1444.27", "跌停1181.67", "市盈动18.66", "市净率6.61", "委比-83.10%", "总市值1.66万亿", "外盘", "展开五档盘口"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const book = flat(r.html(QuoteBoardView, { data: fixture.snapshot, showBook: true }));
    for (const part of ["卖51329.18", "卖11329.0054", "买11328.581", "收起五档"]) assert.ok(book.includes(sq(part)), part);
    const noBook = flat(r.html(QuoteBoardView, { data: { ...fixture.snapshot, bids: [], asks: [], source: "eastmoney", fallback_reason: "腾讯快照:数据源请求超时" } }));
    assert.ok(!noBook.includes("展开五档"));
    assert.ok(noBook.includes("已改用东财快照（无五档）"));
  });
});
