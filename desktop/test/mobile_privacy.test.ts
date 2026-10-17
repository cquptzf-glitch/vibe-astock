import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const portfolio = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-synthetic.json", import.meta.url), "utf8"));
const perf = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-perf-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const noop = () => undefined;

test("隐私模式开关:关着原样,开着遮成 ••••,没有 window 存储也不抛", async () => {
  await withRenderer(async (r) => {
    const p = await r.load("/src/mobile/privacy.ts");
    assert.equal(p.isPrivate(), false);
    assert.equal(p.priv("1,234.5"), "1,234.5");
    p.setPrivate(true);
    try {
      assert.equal(p.isPrivate(), true);
      assert.equal(p.priv("1,234.5"), "••••");
      assert.equal(p.MASK, "••••");
    } finally {
      p.setPrivate(false);
    }
    assert.equal(p.priv("x"), "x");
  });
});

test("开着隐私模式:已实现与分红、收益表现、持仓体检里的金额和股数都遮住,百分比和代码不遮", async () => {
  await withRenderer(async (r) => {
    const p = await r.load("/src/mobile/privacy.ts");
    const { IncomeView } = await r.load("/src/mobile/PortfolioIncome.tsx");
    const { PerfView, XrayView, NO_THRESHOLDS } = await r.load("/src/mobile/PortfolioAnalysis.tsx");
    const xray = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-xray-synthetic.json", import.meta.url), "utf8"));
    p.setPrivate(true);
    try {
      const income = flat(r.html(IncomeView, { data: portfolio, onEdit: noop, onAdd: noop }));
      assert.ok(!income.includes("1,560.5") && !income.includes("388.5") && !income.includes("+2,100") && !income.includes("1,450"), "金额不出现");
      assert.ok(!income.includes("100股"), "股数不出现");
      assert.ok(income.includes("••••"), "换成 ••••");
      assert.ok(income.includes("600519") && income.includes("样本白酒"), "代码和名称照常");
      const perfText = flat(r.html(PerfView, { data: perf }));
      assert.ok(!perfText.includes(String(Math.round(perf.invested)).slice(0, 3)) || perfText.includes("••••"));
      for (const hidden of [perf.invested, perf.final_value, perf.pnl]) assert.ok(!perfText.includes(hidden.toLocaleString("zh-CN", { maximumFractionDigits: 2 })), `${hidden} 被遮住`);
      assert.ok(perfText.includes("+9.38%") || perfText.includes(`${perf.twr_pct}`), "收益率百分比照常显示");
      const xrayText = flat(r.html(XrayView, { data: xray, thresholds: NO_THRESHOLDS }));
      assert.ok(!xrayText.includes(xray.concentration.total_value.toLocaleString("zh-CN")), "持仓总市值被遮住");
      assert.ok(xrayText.includes(`${xray.concentration.top1_pct}%`), "权重百分比照常");
    } finally {
      p.setPrivate(false);
    }
    const visible = flat(r.html(IncomeView, { data: portfolio, onEdit: noop, onAdd: noop }));
    assert.ok(visible.includes("+1,560.5") && !visible.includes("••••"), "关掉后恢复显示");
  });
});

test("开着隐私模式权益曲线不画,说明原因;关着照常画", async () => {
  await withRenderer(async (r) => {
    const p = await r.load("/src/mobile/privacy.ts");
    const { EquityChart } = await r.load("/src/mobile/JournalRiskPanel.tsx");
    const points = [{ cum_pnl: 0 }, { cum_pnl: 120 }, { cum_pnl: 80 }];
    assert.match(r.html(EquityChart, { points }), /<svg[^>]*aria-label="权益曲线/);
    p.setPrivate(true);
    try {
      const html = r.html(EquityChart, { points });
      assert.ok(!html.includes("<svg") && flat(html).includes("隐私模式下不画权益曲线"));
    } finally {
      p.setPrivate(false);
    }
  });
});
