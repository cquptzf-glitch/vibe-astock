import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/review/structure 在 2026-10-17(周六)对真实上游的返回。东财只留约一个月,
// 「之前 20 个交易日」只取到 4 天,所以真实返回里市场结构比较是「暂不可用」;可比较的形状在用例里按真实字段构造。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-structure-2026-10-16.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;

test("真实返回:市场结构比较暂不可用时说清楚天数,字段清单四个池子都稳定,没有登记事件", async () => {
  await withRenderer(async (r) => {
    const { StructureView } = await r.load("/src/mobile/StructurePanel.tsx");
    const text = flat(r.html(StructureView, { data: fixture, onChanged: noop }));
    for (const part of ["结构移位不等于制度变化", "② 市场:涨停结构", "暂不可用:最近 10 个交易日取到 10 天、之前 20 个交易日取到 4 天", "每个窗口至少要 5 天", "东财池子只留约一个月",
      "① 数据源:上游字段清单", "涨停池14 天字段清单稳定", "昨日涨停池", "只覆盖", "不能证明之外的日子字段没变", "③ 制度:人工登记", "还没登记制度事件", "系统不自动推断", "16 个交易日的涨停结构没取到"]) {
      assert.ok(text.includes(sq(part)), part);
    }
  });
});

test("可比较时:最近 vs 之前的中位数、变化(计数用相对、占比用点数)、移位标记;字段变过会列出新增与消失", async () => {
  await withRenderer(async (r) => {
    const { StructureView } = await r.load("/src/mobile/StructurePanel.tsx");
    const data = structuredClone(fixture);
    data.structure = { available: true, reason: null, skipped: [], shifted: [],
      metrics: [
        { metric: "limit_up", label: "涨停家数", kind: "count", recent: 60.5, prior: 40.5, delta: 20, rel_change: 0.494, shifted: true, recent_days: 10, prior_days: 20 },
        { metric: "share_20cm", label: "20% 制度占比", kind: "share", recent: 0.5, prior: 0.35, delta: 0.15, rel_change: null, shifted: true, recent_days: 10, prior_days: 20 },
        { metric: "share_bj", label: "北交所占比", kind: "share", recent: 0.02, prior: 0.03, delta: -0.01, rel_change: null, shifted: false, recent_days: 10, prior_days: 20 },
      ] };
    data.structure.shifted = data.structure.metrics.filter((m: { shifted: boolean }) => m.shifted);
    data.field_drift[0] = { ...data.field_drift[0], changed: true, added: ["zbc"], removed: ["old"], note: "字段清单在 2026-10-14 变过 —— 跨这一天的统计要小心,断点可能来自数据源而不是市场" };
    data.summary = "1 张池子的上游字段变过(涨停池);结构比较:涨停家数、20% 制度占比明显移位;还没登记制度事件(要人工登记,系统不自动推断)。";
    const html = r.html(StructureView, { data, onChanged: noop });
    const text = flat(html);
    for (const part of ["涨停家数60.540.5+49.4%是", "20% 制度占比50%35%+15 个点是", "北交所占比2%3%-1 个点否", "1 张池子的上游字段变过(涨停池)", "涨停家数、20% 制度占比明显移位",
      "新增:zbc;消失:old", "字段清单在 2026-10-14 变过"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /class="up">\+49\.4%</);
    assert.match(html, /class="down">-1 个点</);
  });
});

test("制度事件:列出已登记的(未来日期标「还没施行」),每条有修改入口;编辑器有日期、标题、说明与保存", async () => {
  await withRenderer(async (r) => {
    const { StructureView, EventEditor } = await r.load("/src/mobile/StructurePanel.tsx");
    const data = structuredClone(fixture);
    data.regime_events = [{ id: 1, date: "2026-07-06", title: "主板 ST 涨跌幅放宽到 10%", note: "交易所公告", upcoming: false }, { id: 2, date: "2999-01-01", title: "未来规则", note: "", upcoming: true }];
    data.summary = "已登记 1 条制度事件,另有 1 条还没施行。";
    const html = r.html(StructureView, { data, onChanged: noop });
    const text = flat(html);
    for (const part of ["2026-07-06主板 ST 涨跌幅放宽到 10%交易所公告", "未来规则还没施行", "已登记 1 条制度事件,另有 1 条还没施行", "登记", "修改"]) assert.ok(text.includes(sq(part)), part);
    assert.ok(html.includes('aria-label="修改 主板 ST 涨跌幅放宽到 10%"') && html.includes('aria-label="修改 未来规则"'), "每条事件有带标题的修改入口");
    assert.ok(!text.includes("还没登记制度事件(比如"), "有登记时不再显示空状态");
    const editor = r.html(EventEditor, { event: data.regime_events[0], onDone: noop, onCancel: noop });
    assert.match(editor, /aria-label="施行日期"[^>]*data-value="2026-07-06"/);
    assert.ok(flat(editor).includes("2026-07-06"), "日期按网页控件显示");
    assert.match(editor, /value="主板 ST 涨跌幅放宽到 10%"/);
    const text2 = flat(editor);
    for (const part of ["施行日期(可以是未来)", "标题(最多 60 字)", "说明(选填,最多 300 字)", "保存", "删除", "取消"]) assert.ok(text2.includes(sq(part)), part);
    assert.ok(!flat(r.html(EventEditor, { event: null, onDone: noop, onCancel: noop })).includes("删除"), "新建时没有删除");
  });
});
