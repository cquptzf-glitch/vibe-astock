import assert from "node:assert/strict";
import test from "node:test";

import { textOf, withRenderer } from "./helpers/render.ts";

const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

// 合成结果,不是真实行情
const result = (over: Record<string, unknown> = {}) => ({
  status: "ok", symbol: "600001", market: "SH", symbols: ["600001"],
  strategy: { id: "donchian", name: "唐奇安通道突破 Lite", label: "突破 20 日高 / 跌破 10 日低", execution: "信号使用前一交易日收盘,下一交易日开盘成交", fees: "已计佣金、过户费与卖出印花税(费率见 costs);未计滑点", adjustment: "前复权日线", stop_loss_pct: 5, weighting: "单只全仓" },
  period: { start: "2026-01-02", end: "2026-06-30", bars: 120 },
  metrics: { initial_capital: 100000, final_equity: 108000, total_return_pct: 8, buy_hold_return_pct: 5, excess_return_pct: 3, max_drawdown_pct: -4.5, annualized_return_pct: 16.2, sharpe: 1.23, closed_trades: 2, win_rate_pct: 50, total_fees: 188.5, slippage_cost: 0, open_position: false },
  per_symbol: [], trades: [
    { symbol: "600001", entry_date: "2026-02-03", entry_price: 10, exit_date: "2026-03-04", exit_price: 11, shares: 9000, pnl: 8800, pnl_pct: 9.78, fees: 100, reason: "信号" },
    { symbol: "600001", entry_date: "2026-04-01", entry_price: 12, exit_date: "2026-04-20", exit_price: 11.4, shares: 8000, pnl: -5000, pnl_pct: -5.2, fees: 88.5, reason: "止损" },
  ],
  experiment: { rows: [{ label: "突破 10 日高 / 跌破 5 日低", is_current: false, total_return_pct: 3, max_drawdown_pct: -6, closed_trades: 5, win_rate_pct: 40 }, { label: "突破 20 日高 / 跌破 10 日低", is_current: true, total_return_pct: 8, max_drawdown_pct: -4.5, closed_trades: 2, win_rate_pct: 50 }], positive_share: 1, note: "固定的一小格参数,不是自动寻优" },
  equity_curve: [["2026-01-02", 100000], ["2026-06-30", 108000]], note: "这是移动端 Lite 回测", ...over,
});

test("回测结果:收益、年化、夏普、费用、成交明细(含止损原因)和参数实验都在,当前参数有标记", async () => {
  await withRenderer(async (r) => {
    const { ResultView } = await r.load("/src/mobile/BacktestPanel.tsx");
    const html = flat(r.html(ResultView, { result: result() }));
    for (const part of ["唐奇安通道突破 Lite", "止损 5%", "年化收益+16.20%", "夏普(日度)1.23", "费用 / 滑点189 元 / 0 元", "成交明细(最近 2 笔,共 2 笔)", "止损", "-5.20%", "参数实验(2 组,100% 为正收益)", "(当前)", "不是自动寻优"]) {
      assert.ok(html.includes(sq(part)), part);
    }
  });
});

test("回测结果:多只时多一张分标的表;没有的指标写 —,不写 0", async () => {
  await withRenderer(async (r) => {
    const { ResultView } = await r.load("/src/mobile/BacktestPanel.tsx");
    const multi = result({
      symbols: ["600001", "000002"], strategy: { ...result().strategy, weighting: "等权、各自独立,不再平衡" }, experiment: undefined,
      per_symbol: [{ symbol: "600001", total_return_pct: 8, max_drawdown_pct: -4, buy_hold_return_pct: 5, closed_trades: 2, win_rate_pct: 50, open_position: false }, { symbol: "000002", total_return_pct: -1, max_drawdown_pct: -7, buy_hold_return_pct: -3, closed_trades: 0, win_rate_pct: null, open_position: true }],
      metrics: { ...result().metrics, annualized_return_pct: null, sharpe: null, win_rate_pct: null },
    });
    const html = flat(r.html(ResultView, { result: multi }));
    assert.ok(html.includes(sq("分标的结果(等权、各自独立,不再平衡)")));
    assert.ok(html.includes(sq("000002(持仓中)")));
    assert.ok(html.includes(sq("年化收益—")) && html.includes(sq("夏普(日度)—")) && html.includes(sq("胜率—")));
    assert.ok(!html.includes(sq("参数实验")), "没传实验就没有这块");
  });
});

test("回测请求:按策略取参数、多只用 symbols、空止损不传、多只时不带参数实验", async () => {
  await withRenderer(async (r) => {
    const { buildRequest } = await r.load("/src/mobile/BacktestPanel.tsx");
    const base = { symbols: "600519", strategy: "sma_cross", params: { fast: "5", slow: "20" }, bars: "300", fees: "default", slippage: "", stopLoss: "", experiment: true };
    assert.deepEqual(buildRequest(base), { strategy: "sma_cross", bars: 300, fees: "default", slippage_bps: 0, symbol: "600519", fast: 5, slow: 20, experiment: true });
    const multi = buildRequest({ ...base, symbols: "600519, 000001、300750", strategy: "rsi", params: { period: "14", buy: "25", sell: "75" }, fees: "none", slippage: "3", stopLoss: "8" });
    assert.deepEqual(multi, { strategy: "rsi", bars: 300, fees: "none", slippage_bps: 3, symbols: ["600519", "000001", "300750"], period: 14, buy: 25, sell: 75, stop_loss_pct: 8 });
    assert.equal("symbol" in multi, false);
  });
});
