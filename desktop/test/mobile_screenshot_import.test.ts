import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;
// 合成的识别草稿(不是任何真实持仓)
const drafts = [
  { code: "600519", name: "样本白酒", shares: 100, cost: 1450, uncertain: [], candidates: [], official_name: "样本白酒" },
  { code: null, name: "样本银行", shares: null, cost: null, uncertain: ["没有读到代码,只有名称", "没有读到持有数量", "没有读到成本价"], candidates: [{ symbol: "000001", name: "样本银行" }, { symbol: "600000", name: "样本浦发" }], official_name: null },
  { code: "000002", name: "样本地产", shares: 200, cost: 8.5, uncertain: ["截图里的名称「样本地产」和代码 000002 对应的「样本万科」对不上,请核对是不是读错了代码"], candidates: [], official_name: "样本万科" },
];

test("缩放尺寸:长边超过 1600 才按比例缩小,不放大;最小 1 像素", async () => {
  await withRenderer(async (r) => {
    const { fitSize } = await r.load("/src/mobile/ScreenshotImport.tsx");
    assert.deepEqual(fitSize(1080, 2400), { width: 720, height: 1600 });
    assert.deepEqual(fitSize(800, 600), { width: 800, height: 600 });
    assert.deepEqual(fitSize(3200, 1600), { width: 1600, height: 800 });
    assert.deepEqual(fitSize(10000, 1), { width: 1600, height: 1 });
  });
});

test("能不能写入:代码 6 位、已在自选/已有持仓不叠加、持仓要数量成本和不在未来的日期;未核对的不算 ready", async () => {
  await withRenderer(async (r) => {
    const { problemOf, initialState } = await r.load("/src/mobile/ScreenshotImport.tsx");
    const s = initialState(drafts[0]);
    assert.equal(s.name, "样本白酒");
    assert.equal(problemOf(s, "watch", [], ""), null);
    assert.match(problemOf(s, "watch", ["600519"], ""), /已在自选里/);
    assert.match(problemOf(s, "position", ["600519"], "2026-01-01"), /已有持仓.*不能直接叠加/);
    assert.equal(problemOf(s, "position", [], "2026-01-01"), null);
    assert.match(problemOf(s, "position", [], "2999-01-01"), /不能是未来/);
    assert.match(problemOf(s, "position", [], ""), /建仓日期/);
    assert.match(problemOf({ ...s, code: "6005" }, "watch", [], ""), /6 位数字/);
    assert.match(problemOf({ ...s, shares: "" }, "position", [], "2026-01-01"), /数量/);
    assert.match(problemOf({ ...s, shares: "-3" }, "position", [], "2026-01-01"), /数量/);
    assert.match(problemOf({ ...s, cost: "abc" }, "position", [], "2026-01-01"), /成本/);
    const empty = initialState(drafts[1]);
    assert.deepEqual([empty.code, empty.shares, empty.cost], ["", "", ""], "读不到的是空,不是 0");
  });
});

test("草稿列表:读到的值可改、候选只给不替选、对不上的名称标出来;有问题的不给核对勾选", async () => {
  await withRenderer(async (r) => {
    const { DraftsView, initialState } = await r.load("/src/mobile/ScreenshotImport.tsx");
    const states = drafts.map(initialState);
    const html = r.html(DraftsView, { drafts, states, target: "position", existing: ["000002"], date: "2026-01-02", onChange: noop });
    const text = flat(html);
    for (const part of ["草稿 1", "代码对应:样本白酒", "已对照截图核对这条(100 股 · 成本 1,450)", "草稿 2", "按名称搜到:样本银行 000001", "样本浦发 600000", "没有读到代码,只有名称",
      "现在还不能写入:代码要是 6 位数字", "草稿 3", "对不上,请核对是不是读错了代码", "现在还不能写入:该代码已有持仓"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /aria-label="草稿 1 代码"[^>]*value="600519"|value="600519"[^>]*aria-label="草稿 1 代码"/);
    assert.match(html, /aria-label="草稿 1 已核对"/);
    assert.ok(!html.includes('aria-label="草稿 2 已核对"') && !html.includes('aria-label="草稿 3 已核对"'), "有问题的没有核对勾选");
    const watch = flat(r.html(DraftsView, { drafts, states, target: "watch", existing: [], date: "", onChange: noop }));
    assert.ok(watch.includes("已对照截图核对这条") && !watch.includes("数量(股)"), "自选不要数量成本");
  });
});

test("截图识别卡片默认收起;模型预设来自仓库 providers 模板,测试图只在浏览器里画", async () => {
  await withRenderer(async (r) => {
    const { ScreenshotImport } = await r.load("/src/mobile/ScreenshotImport.tsx");
    const html = r.html(ScreenshotImport, { target: "watch", existing: [], onSaved: noop });
    assert.ok(flat(html).includes(sq("从截图识别(自选)")) && !html.includes('type="file"'));
    assert.match(html, /aria-expanded="false"/);
    const presets = await r.load("/src/mobile/aiPresets.ts");
    assert.equal(presets.AI_PRESETS.length, 7);
    assert.equal(presets.presetOf("https://api.openai.com/v1/")?.id, "openai-api");
    assert.equal(presets.presetOf("https://example.com"), undefined);
    assert.equal(presets.renderProbeImage(), null, "没有 document 时画不出测试图,返回 null");
    assert.equal(presets.PROBE_CODE, "600519");
  });
});
