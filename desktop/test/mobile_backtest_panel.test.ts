import assert from "node:assert/strict";
import test from "node:test";

import { textOf, withRenderer } from "./helpers/render.ts";

const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("回测页只提供 Agent 对话入口，参数通过补问确认",async()=>{
  await withRenderer(async r=>{const {BacktestPanel}=await r.load("/src/mobile/BacktestPanel.tsx");const html=r.html(BacktestPanel,{});const text=textOf(html);
    assert.match(text,/用对话完成历史假设核对/);assert.match(text,/让 AI 配置并运行/);
    assert.doesNotMatch(html,/aria-label="股票代码"|aria-label="策略"|aria-label="日线根数"|<form/);
  });
});

// 合成结果,不是真实行情
const result = (over: Record<string, unknown> = {}) => ({
  status: "ok", symbol: "600001", market: "SH", symbols: ["600001"],
  strategy: { id: "donchian", name: "唐奇安通道突破 Lite", label: "突破 20 日高 / 跌破 10 日低", execution: "信号使用前一交易日收盘,下一交易日开盘成交", fees: "已计佣金、过户费与卖出印花税(费率见 costs);未计滑点", adjustment: "前复权日线", stop_loss_pct: 5, weighting: "单只全仓" },
  period: { start: "2026-01-02", end: "2026-06-30", bars: 120 },
  metrics: { initial_capital: 100000, final_equity: 108000, total_return_pct: 8, buy_hold_return_pct: 5, excess_return_pct: 3, max_drawdown_pct: -4.5, annualized_return_pct: 16.2, sharpe: 1.23, closed_trades: 2, win_rate_pct: 50, total_fees: 188.5, slippage_cost: 0, blocked_buys: 1, blocked_sells: 0, open_position: false },
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
    for (const part of ["唐奇安通道突破 Lite", "止损 5%", "年化收益+16.20%", "夏普(日度)1.23", "费用 / 滑点189 元 / 0 元", "涨跌停没成交买 1 / 卖 0 次", "成交明细(最近 2 笔,共 2 笔)", "止损", "-5.20%", "参数实验(2 组,100% 为正收益)", "(当前)", "不是自动寻优"]) {
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
    const base = { symbols: "600519", strategy: "sma_cross", params: { fast: "5", slow: "20" }, bars: "300", fees: "default", slippage: "", stopLoss: "", experiment: true, limitCheck: true, positionPct: "100", optimize: false };
    assert.deepEqual(buildRequest(base), { strategy: "sma_cross", bars: 300, fees: "default", slippage_bps: 0, limit_check: true, position_pct: 100, symbol: "600519", fast: 5, slow: 20, experiment: true });
    const multi = buildRequest({ ...base, symbols: "600519, 000001、300750", strategy: "rsi", params: { period: "14", buy: "25", sell: "75" }, fees: "none", slippage: "3", stopLoss: "8", limitCheck: false, positionPct: "50", optimize: true });
    assert.deepEqual(multi, { strategy: "rsi", bars: 300, fees: "none", slippage_bps: 3, limit_check: false, position_pct: 50, symbols: ["600519", "000001", "300750"], period: 14, buy: 25, sell: 75, stop_loss_pct: 8 });
    assert.equal("symbol" in multi, false);
  });
});

test("回测结果:回撤区间、月度收益、样本内外对照都在;样本外不可用时写原因", async () => {
  await withRenderer(async (r) => {
    const { ResultView, buildRequest } = await r.load("/src/mobile/BacktestPanel.tsx");
    const full = result({
      drawdowns: [{ peak_date: "2026-03-04", trough_date: "2026-04-20", recover_date: null, depth_pct: -12.5, bars_to_trough: 30, bars_total: 45 }],
      monthly: [{ month: "2026-02", return_pct: 3.2, bench_pct: 1.1 }, { month: "2026-03", return_pct: -1.5, bench_pct: null }],
      optimize: { available: true, in_sample: { start: "2025-01-02", end: "2026-02-27", bars: 280 }, out_of_sample: { start: "2026-03-02", end: "2026-06-30", bars: 120 },
        rows: [{ label: "MA10 / MA30", is_return_pct: 20, oos_return_pct: -4, is_trades: 6, oos_trades: 2, best_in_sample: true }, { label: "MA5 / MA60", is_return_pct: 8, oos_return_pct: 3, is_trades: 4, oos_trades: 1, best_in_sample: false }],
        best_in_sample: { label: "MA10 / MA30", is_return_pct: 20, oos_return_pct: -4, oos_rank: 2, of: 2 }, median_oos_return_pct: -0.5, oos_positive_share: 0.5, note: "样本内选出的最好一格,到样本外还站得住才有参考意义" },
    });
    const html = flat(r.html(ResultView, { result: full }));
    for (const part of ["最深的 1 段回撤", "到最后还没恢复", "-12.50%", "月度收益(策略 vs 买入持有)", "2026-02", "+3.20%", "样本内 / 样本外对照", "样本内最好:MA10 / MA30", "样本外排第 2/2", "50% 为正", "(样本内最好)"]) {
      assert.ok(html.includes(sq(part)), part);
    }
    const unavailable = flat(r.html(ResultView, { result: result({ optimize: { available: false, reason: "K 线太短,不够切成样本内/外" } }) }));
    assert.ok(unavailable.includes(sq("暂不可用:K 线太短,不够切成样本内/外")));
    const body = buildRequest({ symbols: "600519 000001", strategy: "sma_cross", params: { fast: "5", slow: "20" }, bars: "300", fees: "default", slippage: "0", stopLoss: "", experiment: true, limitCheck: true, positionPct: "", optimize: true });
    assert.equal("optimize" in body || "experiment" in body, false, "多只股票不带实验和样本外");
    assert.equal(body.position_pct, 100, "仓位留空按 100");
  });
});

test("回测分享卡:标明历史假设,只放这次回测的读数;没有成交时胜率写 —", async () => {
  await withRenderer(async (r) => {
    const { backtestCardRows, ResultView } = await r.load("/src/mobile/BacktestPanel.tsx");
    const rows = backtestCardRows(result()) as { label: string; value: string }[];
    assert.deepEqual(rows.map((x) => x.label), ["标的", "策略", "区间", "策略收益", "买入持有", "最大回撤", "已平仓 / 胜率", "手续费 / 滑点"]);
    assert.equal(rows[3].value, "+8.00%");
    assert.equal(rows[6].value, "2 笔 / +50.00%");
    const none = backtestCardRows(result({ metrics: { ...result().metrics, closed_trades: 0, win_rate_pct: null } })) as { label: string; value: string }[];
    assert.equal(none[6].value, "0 笔 / —");
    const text = flat(r.html(ResultView, { result: result() }));
    assert.ok(text.includes(sq("图片分享卡")) && text.includes(sq("生成回测结果(历史假设)分享图")) && text.includes(sq("不含自选与持仓")));
    assert.ok(!JSON.stringify(rows).includes("持仓") && !JSON.stringify(rows).includes("成交明细"));
  });
});

test("自然语言回测的对话状态:补问记进历史、工具报错原样显示且历史不变、跑完清空、最多留 6 条", async () => {
  await withRenderer(async (r) => {
    const { reduceNl, NL_EMPTY, NlBacktestBox } = await r.load("/src/mobile/BacktestPanel.tsx");
    const asked = reduceNl(NL_EMPTY, "回测一个策略", { status: "needs_info", question: "哪只股票?" });
    assert.deepEqual(asked.state.history, [{ role: "user", content: "回测一个策略" }, { role: "assistant", content: "哪只股票?" }]);
    assert.equal(asked.state.question, "哪只股票?");
    assert.equal(asked.done, null);
    const failed = reduceNl(asked.state, "茅台", { status: "tool_error", message: "回测工具没有运行成功:K 线不足" });
    assert.equal(failed.state.error, "回测工具没有运行成功:K 线不足");
    assert.equal(failed.state.history.length, 2, "报错不改历史");
    assert.equal(failed.state.question, "");
    let state = asked.state;
    for (let i = 0; i < 4; i++) state = reduceNl(state, "再补" + i, { status: "needs_info", question: "q" + i }).state;
    assert.equal(state.history.length, 6, "最多留 6 条");
    assert.equal(state.history[5].content, "q3");
    const ok = { status: "ok", report_markdown: "# 报告", result: result(), params_used: {}, ignored_params: [], resolved_symbols: ["600519"], saved_document_id: 7 };
    const done = reduceNl(state, "茅台", ok);
    assert.equal(done.done, ok);
    assert.deepEqual(done.state, NL_EMPTY);
    const html = flat(r.html(NlBacktestBox, { onDone: () => undefined }));
    for (const part of ["用一句话回测", "让AI配置并运行", "不采信模型自己编的代码", "报告按真实结果生成并存进研究记录"]) assert.ok(html.includes(sq(part)), part);
  });
});
