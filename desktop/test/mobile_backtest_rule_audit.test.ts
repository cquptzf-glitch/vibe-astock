import assert from "node:assert/strict";
import test from "node:test";
import { withRenderer, textOf } from "./helpers/render.ts";

test("历史制度覆盖显示实际上市日、缺口和来源，不把接口失败显示成完整覆盖", async () => {
  await withRenderer(async r => {
    const { BacktestRuleAudit } = await r.load("/src/mobile/BacktestRuleAudit.tsx");
    const text = textOf(r.html(BacktestRuleAudit, { audit: { mode: "auto", status: "partial", note: "重新上市仍需明确区间", sources: [
      { symbol: "600000", status: "partial", covered_bars: 30, ipo_date: "1999-11-10", st_days: 4, ipo_rule_bars: 0, missing_dates: ["2026-07-03"], source: "BaoStock" },
      { symbol: "920001", status: "failed", covered_bars: 0, reason: "不覆盖北交所历史状态" },
    ] } }));
    for (const value of ["历史制度数据存在缺口", "1999-11-10", "30 根", "2026-07-03", "BaoStock", "不覆盖北交所历史状态", "重新上市仍需明确区间"]) assert.ok(text.includes(value), value);
    assert.ok(!text.includes("已取得本轮历史日状态"));
  });
});

test("用户关闭历史制度取数时界面明确说明近似规则", async () => {
  await withRenderer(async r => {
    const { BacktestRuleAudit } = await r.load("/src/mobile/BacktestRuleAudit.tsx");
    assert.match(textOf(r.html(BacktestRuleAudit, { audit: { mode: "off", status: "disabled", note: "仅按板块" } })), /已关闭历史制度取数，结果使用近似规则/);
  });
});
