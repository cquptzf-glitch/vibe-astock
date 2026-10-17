import assert from "node:assert/strict";
import test from "node:test";
import { createElement as h } from "react";
import { textOf, withRenderer } from "./helpers/render.ts";

test("日历格:周一开头、天数正确、前后补空(2026-10 从周四开始,31 天)", async () => {
  await withRenderer(async (r) => {
    const { monthCells } = await r.load("/src/mobile/controls.tsx");
    const cells = monthCells(2026, 10);
    assert.equal(cells.length % 7, 0);
    assert.deepEqual(cells.slice(0, 5), [null, null, null, 1, 2]);
    assert.equal(cells.filter((c: number | null) => c != null).length, 31);
    assert.equal(monthCells(2024, 2).filter((c: number | null) => c != null).length, 29, "闰年二月 29 天");
    assert.deepEqual(monthCells(2026, 6).slice(0, 2), [1, 2], "2026-06-01 是周一");
  });
});

test("下拉是网页控件:显示当前项,收起时不渲染选项,禁用时不可点,不出现原生 select", async () => {
  await withRenderer(async (r) => {
    const { WebSelect } = await r.load("/src/mobile/controls.tsx");
    const mk = (props: Record<string, unknown>) => r.html(WebSelect, { onChange: () => undefined, "aria-label": "排序", ...props,
      children: [h("option", { key: "a", value: "a" }, "按涨幅"), h("option", { key: "b", value: 2 }, "按成交额")] });
    const html = mk({ value: "a" });
    assert.ok(!html.includes("<select") && !html.includes("<option"));
    assert.match(html, /role="combobox"[^>]*aria-expanded="false"/);
    assert.ok(textOf(html).includes("按涨幅") && !textOf(html).includes("按成交额"));
    assert.ok(textOf(mk({ value: 2 })).includes("按成交额"), "数字 value 也能匹配");
    assert.match(mk({ value: "a", disabled: true }), /<button[^>]*role="combobox"[^>]*disabled/);
  });
});

test("日期字段显示所选日期或提示;收起时不渲染日历", async () => {
  await withRenderer(async (r) => {
    const { DateField } = await r.load("/src/mobile/controls.tsx");
    const filled = r.html(DateField, { value: "2026-10-16", onChange: () => undefined, "aria-label": "复盘日", max: "2026-10-17" });
    assert.ok(textOf(filled).includes("2026-10-16"));
    assert.match(filled, /aria-label="复盘日"[^>]*data-value="2026-10-16"/);
    assert.ok(!filled.includes("wc-cal"));
    assert.ok(textOf(r.html(DateField, { value: "", onChange: () => undefined })).includes("选择日期"));
  });
});
