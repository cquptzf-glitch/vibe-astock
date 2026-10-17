import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/intraday 在 2026-10-17(周六)的真实返回 —— 休市,六个时点全部是「休市」。
// 盘中状态(快照、验证条件读数)在用例里按真实字段形状构造。
const weekend = JSON.parse(readFileSync(new URL("./fixtures/mobile-intraday-2026-10-17.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const props = { onCapture: null, capturing: false, captureMsg: null };

const cohort = (median: number, extra: Record<string, unknown> = {}) => ({
  expected: 36, sample: 34, coverage_rate: 0.944, avg: median - 0.3, median, up_count: 20, down_count: 14, up_rate: 0.588, deep_loss_5: 3,
  by_tier: { 首板: { sample: 28, avg: 1.1, median: 0.9, up_rate: 0.64 }, "2板": { sample: 4, avg: -0.5, median: -0.2, up_rate: 0.5 } },
  top_board_level: 5, top_boards: [{ code: "002343", name: "慈文传媒", boards: 5, sector: "影视院线", pct: 4.71, at_limit: null }], ...extra,
});
const snap = (slot: string, clock: string, median: number, live: Record<string, number | null> | null, extra: Record<string, unknown> = {}) => ({
  date: "2026-10-16", slot, captured_at: "2026-10-16T01:25:40Z", clock, drift_minutes: 0, basis: slot === "09:25" ? "open" : "latest", prev_date: "2026-10-15",
  cohort: cohort(median, extra.cohort as Record<string, unknown>), live, gaps: [], ...(slot === "09:25" ? { live_reason: "集合竞价阶段还没有当天的涨停 / 炸板池" } : {}),
});
const livePath = () => {
  const v = structuredClone(weekend);
  Object.assign(v, { date: "2026-10-16", today: true, trading_day: true });
  const a = snap("09:25", "09:25:40", 1.2, null), b = snap("10:00", "10:01:10", -0.4, { limit_up: 40, broken: 12, limit_down: 3, highest_board: 5, consec_count: 9, seal_rate: 0.77, broken_rate: 0.23, promotion_1to2: 0.31, promotion_all: 0.4 });
  v.slots = [
    { slot: "09:25", window_end: "09:29", state: "saved", reason: null, snapshot: a },
    { slot: "09:35", window_end: "09:43", state: "failed", reason: "行情时间都不是今天", snapshot: null },
    { slot: "10:00", window_end: "10:08", state: "saved", reason: null, snapshot: b },
    { slot: "11:30", window_end: "11:38", state: "upcoming", reason: null, snapshot: null },
    { slot: "14:00", window_end: "14:08", state: "upcoming", reason: null, snapshot: null },
    { slot: "15:00", window_end: "15:08", state: "upcoming", reason: null, snapshot: null },
  ];
  v.auction = a;
  v.auction_reason = null;
  v.checks = { set_date: "2026-10-15", items: [
    { id: "1", metric: "money_effect_median", label: "赚钱效应中位数", unit: "%", kind: "pct", eps: 0.5, direction: "down", direction_label: "下降", reason: "高标断层", base_value: 0.5,
      close_only: false, points: [{ slot: "09:25", value: 1.2, actual: "up", actual_label: "上升", verified: false }, { slot: "10:00", value: -0.4, actual: "down", actual_label: "下降", verified: true }] },
    { id: "2", metric: "theme_concentration", label: "头部行业集中度", unit: "", kind: "rate", eps: 0.05, direction: "up", direction_label: "上升", reason: "", base_value: 0.22, close_only: true, points: [] },
    { id: "3", metric: "limit_up_count", label: "涨停家数", unit: "家", kind: "count", eps: 5, direction: "up", direction_label: "上升", reason: "", base_value: 36,
      close_only: false, points: [{ slot: "09:25", value: null, actual: null, actual_label: null, verified: null }] },
  ] };
  v.warnings = ["14:00 的快照不是这个时点当场存的(或格式不对),原件保留,没算进来"];
  return v;
};

test("休市日:六个时点标休市、竞价说明原因、没有路径和读数,不显示「现在存一张」", async () => {
  await withRenderer(async (r) => {
    const { IntradayView } = await r.load("/src/mobile/IntradayPanel.tsx");
    const html = r.html(IntradayView, { r: weekend, ...props, onCapture: () => undefined });
    const text = flat(html);
    for (const part of ["2026-10-17(今天)· 休市,不存快照", "09:25休市", "15:00休市", "周末休市,没有竞价", "这一天休市,没有盘中路径", "2026-10-17 休市,没有盘中读数"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(!text.includes("现在存一张"));
    assert.ok(!html.includes("各时点读数"));
  });
});

test("竞价强弱:开盘价口径,分板位,最高标;盘中路径表按时点列全市场家数,竞价那行为 —", async () => {
  await withRenderer(async (r) => {
    const { IntradayView } = await r.load("/src/mobile/IntradayPanel.tsx");
    const text = flat(r.html(IntradayView, { r: livePath(), ...props, onCapture: () => undefined }));
    for (const part of ["2026-10-16(今天)", "现在存一张", "09:35没存成", "行情时间都不是今天", "11:30待存", "11:30–11:38 自动存",
      "09:25:40 存 · 34/36 只", "样本是 2026-10-15 收盘封住的 36 只,取到竞价行情 34 只", "当日开盘价 ÷ 昨收", "+1.2%", "高开占比(20/34)", "低开超 5%",
      "首板 (28 只)", "高开 64% · 均 +1.1%", "最高标(5 板)", "慈文传媒002343 · 影视院线+4.71%",
      "全市场涨停 / 炸板家数:集合竞价阶段还没有当天的涨停 / 炸板池"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    // 路径表:09:25 是高开口径、10:00 是红盘口径;竞价那行没有全市场家数
    const table = flat(r.html(IntradayView, { r: livePath(), ...props }).match(/<table[\s\S]*?<\/table>/)![0]);
    assert.ok(table.includes(sq("09:25 09:25:40 +1.2% +0.9% 高开 58.8% 3 34/36 — — — — — —")), table);
    assert.ok(table.includes(sq("10:00 10:01:10 -0.4% -0.7% 红盘 58.8% 3 34/36 40 12 3 5板 77% 31%")), table);
    assert.ok(text.includes(sq("⚠️ 14:00 的快照不是这个时点当场存的")));
  });
});

test("验证条件盘中读数:暂成立 / 暂不成立 / 判不了,只在收盘后才有的项如实说明", async () => {
  await withRenderer(async (r) => {
    const { IntradayView, reading, upLabel } = await r.load("/src/mobile/IntradayPanel.tsx");
    const text = flat(r.html(IntradayView, { r: livePath(), ...props }));
    for (const part of ["2026-10-15 晚上在「明日验证」里写的条件", "赚钱效应中位数预期下降昨天 +0.5%· 高标断层", "09:25+1.2%上升暂不成立", "10:00-0.4%下降暂成立",
      "头部行业集中度预期上升昨天 22%", "这项只在收盘后才有读数,盘中不判断", "涨停家数预期上升昨天 36 家", "09:25—判不了"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.equal(reading(null, "count", "家"), "—");
    assert.equal(reading(0.23, "rate", ""), "23%");
    assert.equal(upLabel("09:25"), "高开");
    assert.equal(upLabel("10:00"), "红盘");
    const noCond = livePath();
    noCond.checks = { set_date: "2026-10-15", items: [] };
    assert.ok(flat(r.html(IntradayView, { r: noCond, ...props })).includes(sq("2026-10-15 晚上没写验证条件")));
    const empty = livePath();
    Object.assign(empty, { slots: weekend.slots.map((s: object) => ({ ...s, state: "upcoming", reason: null })), auction: null, auction_reason: "09:25 竞价结果出来后,App 开着盯盘就会存(09:25–09:29)", checks: { set_date: null, items: [] } });
    const emptyText = flat(r.html(IntradayView, { r: empty, ...props }));
    assert.ok(emptyText.includes(sq("这一天还没有存到时点快照")) && emptyText.includes(sq("还没存到时点快照,存下第一张后才有读数可对")));
  });
});
