import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";
import { mobilePath, parseMobileRoute } from "../src/mobile/mobileRoutes.ts";

test("资金流保留最新缺值、累计口径和旧日期提示", async () => {
  await withRenderer(async (r) => {
    const { FundFlowResults } = await r.load("/src/mobile/StockInsightsPanel.tsx");
    const rows = [
      { time: "2026-10-15 14:59", date: "2026-10-15", main_net: 100, small_net: -10, mid_net: -90, large_net: 40, super_net: 60 },
      { time: "2026-10-15 15:00", date: "2026-10-15", main_net: null, small_net: 0, mid_net: null, large_net: null, super_net: null },
    ];
    const html = r.html(FundFlowResults, { data: { status: "partial", mode: "minute", rows, latest: rows[1], count: 2, stale: true, method: "各分钟点为当日累计净额，不可逐分钟相加", source: "eastmoney", unit: "元" } });
    const text = textOf(html);
    assert.match(text, /并非今日数据/);
    assert.match(text, /最新缺值不会由前一条记录代替/);
    assert.match(text, /最新采样 2026-10-15 15:00/);
    assert.match(text, /主力净额 —/);
    assert.match(text, /小单净额 0/);
    assert.match(text, /不可逐分钟相加/);
    assert.match(html, /缺值 1 个/);
  });
});

test("空资金流与请求故障各有可见状态和重试", async () => {
  await withRenderer(async (r) => {
    const { FundFlowResults, RequestState } = await r.load("/src/mobile/StockInsightsPanel.tsx");
    assert.match(textOf(r.html(FundFlowResults, { data: { rows: [], latest: null } })), /空序列不代表净额为零/);
    assert.match(textOf(r.html(RequestState, { loading: true, error: "", refresh: () => undefined })), /正在获取公开数据/);
    assert.match(textOf(r.html(RequestState, { loading: false, error: "来源超时", refresh: () => undefined })), /来源超时.*重试/);
  });
});

test("量价观察明确不足窗口、未收盘与历史读数边界", async () => {
  await withRenderer(async (r) => {
    const { ObservationResults } = await r.load("/src/mobile/StockInsightsPanel.tsx");
    const data = { status: "partial", date: "2026-10-16", bars: 20, same_day: true, stale: false, ma20: 10, ma5: 11, ma10: 10.5, ma60: null, return5: 2, return20: null, return60: null, ma20_bias_pct: 10, ma_cross: null, range_observation: null, prior20_high: null, prior20_low: null, volume_ratio_prev5: null, source: "eastmoney", note: "不是盘中量比，不输出买卖建议。" };
    const text = textOf(r.html(ObservationResults, { data }));
    assert.match(text, /交易时段内尚未定型/);
    assert.match(text, /历史不足 61 根/);
    assert.match(text, /近20根涨跌 —/);
    assert.match(text, /比较窗口不含本根/);
    assert.match(text, /不是盘中量比/);
    assert.match(text, /均线交叉：数据不足/);
  });
});

test("反向板块保留缺值、截断说明及可点击入口", async () => {
  await withRenderer(async (r) => {
    const { StockBoardResults } = await r.load("/src/mobile/StockInsightsPanel.tsx");
    const html = r.html(StockBoardResults, { data: { status: "partial", items: [{ code: "BK0475", name: "<银行>", change_pct: null }], total: 3, dropped: 1, truncated: true, note: "含地区及指数分组", source: "eastmoney" }, onOpenBoard: () => undefined });
    assert.match(textOf(html), /仅获取 1 \/ 3 个关联/);
    assert.match(textOf(html), /BK0475 · —/);
    assert.match(html, /&lt;银行&gt;/);
    assert.match(html, /<button/);
    assert.match(textOf(html), /不是完整名单/);
  });
});

test("异常波动保留来源非当日和未知状态，筛选只作用当前页", async () => {
  await withRenderer(async (r) => {
    const { RiskResults } = await r.load("/src/mobile/RiskPanel.tsx");
    const data = { kind: "anomaly", date: "2026-10-15", stale: true, dropped: 1, items: [
      { symbol: "BJ920575", name: "<样本>", change_pct: null, deviation_pct: 100, days: 10, rule: "来源规则", is_today: false },
      { symbol: "SH600519", name: "样本二", change_pct: 0, deviation_pct: null, days: null, rule: "未知规则码 999", is_today: null },
    ], source: "eastmoney", note: "不替代交易所公告" };
    const html = r.html(RiskResults, { data, onOpenSymbol: () => undefined });
    const text = textOf(html);
    assert.match(text, /并非今日名单/);
    assert.match(text, /来源标记非当日/);
    assert.match(text, /当日状态未提供/);
    assert.match(text, /涨跌 —/);
    assert.match(text, /异常记录已跳过/);
    assert.match(html, /&lt;样本&gt;/);
    assert.match(textOf(r.html(RiskResults, { data, keyword: "不存在", onOpenSymbol: () => undefined })), /当前页没有匹配记录/);
  });
});

test("重点监控区分尚未生效和生效中，空名单不宣称无风险", async () => {
  await withRenderer(async (r) => {
    const { RiskResults } = await r.load("/src/mobile/RiskPanel.tsx");
    const data = { kind: "monitor", date: "2026-10-16", items: [
      { symbol: "BJ920575", name: "样本一", start: "2026-10-19", end: "2026-10-23", state: "upcoming" },
      { symbol: "SZ000001", name: "样本二", start: "2026-10-12", end: "2026-10-16", state: "active" },
    ], source: "eastmoney", note: "来源名单" };
    const text = textOf(r.html(RiskResults, { data, onOpenSymbol: () => undefined }));
    assert.match(text, /尚未生效/);
    assert.match(text, /生效中/);
    assert.match(text, /2026-10-19 ~ 2026-10-23/);
    assert.match(textOf(r.html(RiskResults, { data: { ...data, items: [] }, onOpenSymbol: () => undefined })), /不代表其他风险不存在/);
  });
});

test("异常监控使用短线路由，拒绝任意查询注入", () => {
  assert.deepEqual(parseMobileRoute("/mobile/shortline/risk"), { page: "risk" });
  assert.equal(mobilePath({ page: "risk" }), "/mobile/shortline/risk");
  assert.equal(parseMobileRoute("/mobile/shortline/risk?kind=arbitrary"), null);
});
