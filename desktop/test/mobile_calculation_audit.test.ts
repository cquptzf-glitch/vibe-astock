import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

test("计算审计展示真实输入、依赖、函数版本与服务端 display", async () => {
  await withRenderer(async (r) => {
    const { CalculationAuditView } = await r.load("/src/mobile/CalculationAudit.tsx");
    const html = r.html(CalculationAuditView, { record: {
      calculation_id: "calc-synthetic", node_id: "pe", function: "pe_ttm_from_parts", executed: true, calc_version: "0.5.0",
      inputs: { total_market_cap: 1000, cap_unit: "亿元", ttm_profit: 40, profit_unit: "亿元" },
      inputs_refs: { ttm_profit: { type: "calculation", id: "calc-prior", path: "/value" } },
      bindings: {}, output: { status: "ok", value: 25, unit: "倍", display: "25.00 倍", reason: "" },
    } });
    const text = textOf(html);
    for (const s of ["25.00 倍", "已执行确定性计算", "0.5.0", "calc-prior", "实际参与计算的输入", "total_market_cap", "亿元"]) assert.ok(text.includes(s), s);
  });
});

test("依赖拒绝与无意义结果写明原因，不能显示成零或成功", async () => {
  await withRenderer(async (r) => {
    const { CalculationAuditView } = await r.load("/src/mobile/CalculationAudit.tsx");
    const record = { calculation_id: "calc-invalid", node_id: "ttm", function: "ttm_sum", executed: false, calc_version: "0.5.0", inputs: {}, inputs_refs: {}, bindings: {}, output: { status: "error", value: null, unit: "元", display: null, reason: "上游计算没有可用结果" } };
    const text = textOf(r.html(CalculationAuditView, { record }));
    assert.match(text, /节点未执行.*计算未通过/s);
    assert.ok(text.includes("上游计算没有可用结果"));
    assert.ok(!text.includes("0.00 元"));
    assert.ok(!text.includes("已执行确定性计算"));
  });
});

test("研究结论显示计算库的展示值和审计入口，不把计算链标成无来源", async () => {
  await withRenderer(async (r) => {
    const { ResearchTaskView } = await r.load("/src/mobile/ResearchWorkflowPanel.tsx");
    const text = textOf(r.html(ResearchTaskView, { task: { id: 1, title: "合成研究", status: "active", stages: ["财务"], gaps: [], results: [{ stage: "财务", paragraphs: [{ text: "合成计算已核对。", evidence: ["E1"] }], checks: [{ ok: true, level: "calculation", actual: 0.2, display: "20.00%", claim: { label: "增长" } }], calculations: [{ calculation_id: "calc-synthetic", node_id: "growth", function: "growth_rate", executed: true, output: { status: "ok", value: 0.2, unit: "小数", display: "20.00%", reason: "" } }], conditions: [], model: "fixture", at: "2026-10-18" }] } }));
    assert.ok(text.includes("增长：20.00% · 受控计算链"));
    assert.ok(text.includes("本阶段计算 (1)"));
    assert.ok(text.includes("查看计算依赖与输入审计"));
    assert.ok(!text.includes("无来源"));
  });
});
