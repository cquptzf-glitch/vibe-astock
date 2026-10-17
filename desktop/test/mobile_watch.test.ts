import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/watch/poll 在 2026-10-17(周六)的真实返回 —— 昨日梯队是 10-15 涨停池、行情是 10-16 收盘。
// 盘中状态(异动、持仓自选、今日行情)在各用例里按需改写。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-watch-2026-10-17.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;
const base = { threshold: 1.5, scope: "all", ladderMin: 1, onTab: noop, onThreshold: noop, onScope: noop, onLadderMin: noop, onOpenSymbol: noop, error: "" };

function live() {
  const v = structuredClone(fixture);
  Object.assign(v, { date: "2026-10-16", clock: "10:12:03", phase: "morning", phase_label: "上午连续竞价", trading_day: true, detecting: true, poll_ms: 3000, quote_day: "2026-10-16" });
  for (const r of v.ladder.rows) Object.assign(r, { today: true, quote_time: "20261016101200" });
  v.ladder.rows[0].at_limit = true;
  for (const r of v.ladder.rows.slice(1)) r.at_limit = false;
  v.alerts = [
    { ts: "10:11:57", code: "002343", name: "慈文传媒", kind: "封板", change_pct: null, price: 9.33, pct: 10.02, msg: "触及涨停 9.33", sources: ["昨日5板"] },
    { ts: "10:05:12", code: "600519", name: "贵州茅台", kind: "急跌", change_pct: -1.62, price: 1308, pct: -1.6, msg: "3 分钟 -1.6%(现价 1308,今日 -1.6%)", sources: ["持仓", "大票"] },
    { ts: "09:48:30", code: "300308", name: "中际旭创", kind: "急拉", change_pct: 3.4, price: 160, pct: 4.2, msg: "2 分钟 +3.4%(现价 160,今日 +4.2%)", sources: ["自选", "昨日成交额前十"] },
  ];
  v.holdings = [{ code: "600519", name: "贵州茅台", price: 1308, pct: -1.6, amount_yuan: 5e9, quote_time: "20261016101200", today: true, pnl_pct: -4.25, risk_state: "stop_near", limit_up: 1460, limit_down: 1194 }];
  v.watchlist = [{ code: "300308", name: "中际旭创", price: 160, pct: 4.2, amount_yuan: 1.2e10, quote_time: "20261016101200", today: true, limit_up: 185.6, limit_down: 123.8 },
    { code: "920575", name: "北交所样本", price: null, pct: null, amount_yuan: null, quote_time: null, today: false }];
  v.turnover = { date: "2026-10-15", rows: [{ code: "300308", name: "中际旭创", price: 160, pct: 4.2, amount_yuan: 1.2e10, quote_time: "20261016101200", today: true, rank: 1, prev_amount_yuan: 1.636e10 }] };
  v.notes = [];
  return v;
}

test("异动流:幅度与范围筛选只筛这一页,封板开板不受幅度影响,来源标签可见", async () => {
  await withRenderer(async (r) => {
    const { LiveWatchView, filterAlerts } = await r.load("/src/mobile/MonitorPanel.tsx");
    const v = live();
    assert.deepEqual(filterAlerts(v.alerts, 3, "all").map((a: { kind: string }) => a.kind), ["封板", "急拉"]);
    assert.deepEqual(filterAlerts(v.alerts, 1.5, "mine").map((a: { kind: string }) => a.kind), ["急跌", "急拉"]);
    const text = flat(r.html(LiveWatchView, { ...base, v, tab: "position" }));
    for (const part of ["盯盘中 · 上午连续竞价", "北京时间 10:12:03", "每 3 秒刷新", "盘中异动", "今天 3 条", "3 分钟内 ±1.5%", "不是推荐也不是预测",
      "10:11:57封板慈文传媒002343触及涨停 9.33昨日5板", "急跌贵州茅台", "持仓大票", "急拉中际旭创"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const filtered = flat(r.html(LiveWatchView, { ...base, v, tab: "position", threshold: 5, scope: "mine" }));
    assert.ok(filtered.includes(sq("按当前筛选没有异动(今天共记下 3 条)")));
  });
});

test("持仓只给浮动盈亏 % 和止损状态,不给股数和成本;自选没取到的不当 0", async () => {
  await withRenderer(async (r) => {
    const { LiveWatchView } = await r.load("/src/mobile/MonitorPanel.tsx");
    const v = live();
    const position = flat(r.html(LiveWatchView, { ...base, v, tab: "position" }));
    for (const part of ["1 个持仓到了止损/目标附近或已触发", "贵州茅台600519 · 浮盈亏-4.25%", "1,308", "-1.6%", "接近止损", "股数和成本在「我的持仓」"]) {
      assert.ok(position.includes(sq(part)), part);
    }
    assert.ok(!/股$|成本\d/.test(position));
    const watch = flat(r.html(LiveWatchView, { ...base, v, tab: "watch" }));
    for (const part of ["中际旭创300308 · 成交 120 亿", "+4.2%", "北交所样本920575", "没取到"]) assert.ok(watch.includes(sq(part)), part);
    assert.ok(!watch.includes("北交所样本920575·成交0"));
  });
});

test("昨日梯队:今日行情统计、封住只数今天的、按板位筛;周末说清楚是哪天的读数", async () => {
  await withRenderer(async (r) => {
    const { LiveWatchView, ladderStats } = await r.load("/src/mobile/MonitorPanel.tsx");
    const v = live();
    const stats = ladderStats(v.ladder.rows);
    assert.equal(stats.total, 36);
    assert.equal(stats.sample, 36);
    assert.equal(stats.sealed, 1);
    const text = flat(r.html(LiveWatchView, { ...base, v, tab: "ladder" }));
    for (const part of ["2026-10-15 收盘涨停 36 只、最高 5 板", `今日行情36/36`, `上涨${stats.up}`, `下跌${stats.down}`, "封住1", "5板慈文传媒002343 · 影视院线", "封住"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const twoPlus = flat(r.html(LiveWatchView, { ...base, v, tab: "ladder", ladderMin: 2 }));
    const firstOnly = v.ladder.rows.find((row: { boards: number }) => row.boards === 1);
    assert.ok(!twoPlus.includes(sq(firstOnly.name + firstOnly.code)), "首板被筛掉");
    const weekend = flat(r.html(LiveWatchView, { ...base, v: fixture, tab: "ladder" }));
    assert.ok(weekend.includes(sq("今天还没取到这 36 只的行情,下面是 2026-10-16 的读数(标「非今日」)")));
    assert.ok(weekend.includes("非今日"));
    assert.ok(weekend.includes(sq("周末休市,不判断异动")));
  });
});

test("大票前十 / 跌幅前五、昨日成交额前十、缺口说明", async () => {
  await withRenderer(async (r) => {
    const { LiveWatchView } = await r.load("/src/mobile/MonitorPanel.tsx");
    const big = flat(r.html(LiveWatchView, { ...base, v: fixture, tab: "bigcap" }));
    for (const part of ["总市值 500 亿以上 356 只", "涨幅前十", "江淮汽车600418", "+9.98%", "跌幅前五", "瑞芯微603893", "-9.99%"]) assert.ok(big.includes(sq(part)), part);
    const missing = flat(r.html(LiveWatchView, { ...base, v: fixture, tab: "turnover" }));
    assert.ok(missing.includes(sq("昨日成交额前十没有:原因见下方说明")));
    assert.ok(missing.includes(sq("昨日成交额前十(2026-10-15)没存下")));
    const turnover = flat(r.html(LiveWatchView, { ...base, v: live(), tab: "turnover" }));
    assert.ok(turnover.includes(sq("1中际旭创300308 · 昨 163.6 亿 · 今 120 亿")));
  });
});
