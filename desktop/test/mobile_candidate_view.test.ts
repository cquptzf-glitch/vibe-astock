import assert from "node:assert/strict";
import test from "node:test";

import { textOf, withRenderer } from "./helpers/render.ts";

import {
  DEFAULT_FILTERS, evaluate, parsePresets, PRESETS_MAX, sortCandidates, upsertPreset,
  type Filters, type Metrics,
} from "../src/mobile/candidateView.ts";

const off = (patch: Partial<Filters> = {}): Filters => ({
  pe: { enabled: false, min: "", max: "" }, pb: { enabled: false, min: "", max: "" }, marketCap: { enabled: false, min: "", max: "" },
  amount: { enabled: false, min: "", max: "" }, turnover: { enabled: false, min: "", max: "" }, changePct: { enabled: false, min: "", max: "" }, ...patch,
});
const m = (patch: Partial<Metrics> = {}): Metrics => ({ pe: null, pb: null, marketCap: null, amount: null, turnover: null, changePct: null, ...patch });

test("满足全部:有一项不符合就不符合;没有不符合但缺数据 = 待补数据,不当成不符合也不当成 0", () => {
  const f = off({ pe: { enabled: true, min: "0", max: "40" }, pb: { enabled: true, min: "", max: "5" } });
  assert.equal(evaluate(m({ pe: 20, pb: 3 }), f, "all").status, "met");
  assert.equal(evaluate(m({ pe: 50, pb: 3 }), f, "all").status, "unmet");
  const missing = evaluate(m({ pe: 20 }), f, "all");
  assert.deepEqual([missing.status, missing.missing, missing.hits, missing.enabled], ["missing", ["pb"], 1, 2]);
  assert.equal(evaluate(m({ pe: 50 }), f, "all").status, "unmet", "已经有一项不符合,缺的那项不改变结论");
  assert.equal(evaluate(m({ pe: 0, pb: 5 }), f, "all").status, "met", "边界值算符合");
});

test("满足任一:有一项符合就符合;都不符合才不符合;缺数据且没有符合 = 待补数据", () => {
  const f = off({ pe: { enabled: true, min: "0", max: "10" }, changePct: { enabled: true, min: "5", max: "" } });
  assert.equal(evaluate(m({ pe: 8, changePct: 1 }), f, "any").status, "met");
  assert.equal(evaluate(m({ pe: 80, changePct: 1 }), f, "any").status, "unmet");
  assert.equal(evaluate(m({ pe: 80 }), f, "any").status, "missing");
  assert.equal(evaluate(m({ pe: 80, changePct: 6 }), f, "any").status, "met");
});

test("没启用任何条件 = 全部符合;空的最小/最大表示不设限;非数字输入不当作限制", () => {
  assert.equal(evaluate(m(), off(), "all").status, "met");
  assert.equal(evaluate(m(), off(), "any").status, "met");
  const open = off({ pe: { enabled: true, min: "", max: "" } });
  assert.equal(evaluate(m({ pe: -5 }), open, "all").status, "met", "没设上下限就都算符合");
  assert.equal(evaluate(m(), open, "all").status, "missing", "但值取不到仍是待补数据");
  assert.equal(evaluate(m({ pe: 5 }), off({ pe: { enabled: true, min: "abc", max: "" } }), "all").status, "met");
  assert.equal(evaluate(m({ pe: 30 }), DEFAULT_FILTERS, "all").status, "met", "默认只启用 PE 0–40");
});

test("排序:空值永远在最后(升降序都一样);相同值按代码排;名称按中文排", () => {
  const row = (symbol: string, name: string, pe: number | null, hits = 0) => ({ symbol, name, metrics: m({ pe }), evaluation: { status: "met" as const, hits, enabled: 3, failed: [], missing: [] } });
  const rows = [row("600003", "丙", null), row("600001", "甲", 30), row("600002", "乙", 10), row("600004", "丁", 30)];
  assert.deepEqual(sortCandidates(rows, "pe", "asc").map((r) => r.symbol), ["600002", "600001", "600004", "600003"]);
  assert.deepEqual(sortCandidates(rows, "pe", "desc").map((r) => r.symbol), ["600001", "600004", "600002", "600003"], "降序时空值仍在最后");
  const byHits = [row("a", "x", 1, 1), row("b", "y", 1, 3), row("c", "z", 1, 2)];
  assert.deepEqual(sortCandidates(byHits, "hits", "desc").map((r) => r.symbol), ["b", "c", "a"]);
  assert.deepEqual(sortCandidates(rows, "name", "asc").map((r) => r.name), ["丙", "丁", "甲", "乙"].sort((a, b) => a.localeCompare(b, "zh-CN")));
  assert.equal(rows[0]!.symbol, "600003", "不改原数组");
});

test("条件组合:同名覆盖、超上限拒绝而不是悄悄挤掉旧的、坏数据整条丢掉", () => {
  const f = off({ pe: { enabled: true, min: "1", max: "2" } });
  let list = upsertPreset([], " 低估值 ", "all", f)!;
  assert.deepEqual(list.map((p) => p.name), ["低估值"]);
  list = upsertPreset(list, "低估值", "any", f)!;
  assert.deepEqual([list.length, list[0]!.mode], [1, "any"], "同名覆盖");
  assert.equal(upsertPreset(list, "   ", "all", f), null);
  assert.equal(upsertPreset(list, "字".repeat(21), "all", f), null);
  for (let i = 1; i < PRESETS_MAX; i++) list = upsertPreset(list, "组" + i, "all", f)!;
  assert.equal(list.length, PRESETS_MAX);
  assert.equal(upsertPreset(list, "第十一组", "all", f), null, "满了就拒绝");
  assert.equal(upsertPreset(list, "组1", "all", f)?.length, PRESETS_MAX, "满了仍可覆盖已有的");
  const text = JSON.stringify(list);
  assert.equal(parsePresets(text).length, PRESETS_MAX);
  assert.deepEqual(parsePresets(null), []);
  assert.deepEqual(parsePresets("{坏的"), []);
  const bad = JSON.stringify([{ name: "ok", mode: "all", filters: f }, { name: "缺字段", mode: "all", filters: { pe: f.pe } }, { name: "模式错", mode: "some", filters: f }, { name: "ok", mode: "any", filters: f }]);
  assert.deepEqual(parsePresets(bad).map((p) => p.name), ["ok"], "坏的丢掉,重名只留第一条");
});

test("候选池页面首屏:两种范围、组合方式、六个条件、排序和条件组合入口都在", async () => {
  await withRenderer(async (r) => {
    const { CandidatePanel } = await r.load("/src/mobile/CandidatePanel.tsx");
    const text = textOf(r.html(CandidatePanel, { onOpenSymbol: () => undefined })).replace(/s+/g, "");
    for (const part of ["自选池", "持仓池", "满足全部", "满足任一", "PE(TTM)", "PB", "总市值", "成交额", "换手率", "涨跌幅", "保存组合", "空值排在最后", "条件改动立即重新判断"]) assert.ok(text.includes(part), part);
  });
});
