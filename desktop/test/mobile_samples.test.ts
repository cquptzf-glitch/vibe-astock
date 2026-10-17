import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/shortline/samples?days=30 在 2026-10-16 收盘后对真实上游的返回(东财只留约一个月,实际可用 14 天)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-samples-2026-10-16.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("涨停样本:事后名单警告、窗口说明、基准线、样本偏小、分环境都显示", async () => {
  await withRenderer(async (r) => {
    const { SamplesView, Caveat } = await r.load("/src/mobile/SamplesPanel.tsx");
    const html = r.html(SamplesView, { data: fixture });
    const text = flat(html);
    for (const part of ["2026-09-21 ~ 2026-10-16", "请求 30 个交易日,实际可用 14 天", "≤208.8 算弱,≥337.8 算强", "首板 · 最后封板时间曲线", "开盘秒板", "14:00后", "封板时间缺失",
      "按打法分开看", "首板打板", "全体涨停", "基准", "首板·早封", "比基准 +0.72%", "高标接力", "样本偏小", "分情绪环境(按入场那天)", "情绪强", "环境未覆盖",
      "16 个交易日的涨停样本没取到", "1 天的入场日情绪读数没取到"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    // 高标接力 74 只次 ≥ 60 不标偏小;只有 60 以下的才标
    const card = (name: string) => flat(html.split("<section").find((s) => s.includes(`aria-label="${name}"`)) ?? "");
    assert.ok(!card("高标接力").includes("样本偏小"));
    assert.ok(card("首板·尾盘封").includes("样本偏小") === false, "尾盘封样本 60 只次,正好不算偏小");
    const caveat = flat(r.html(Caveat, { text: fixture.sample_caveat }));
    assert.ok(caveat.includes(sq("事后名单")) && caveat.includes(sq("不是策略回测")));
    assert.ok(flat(r.html(Caveat, {})).includes(sq("冲板失败且未回封的股票不在样本内")), "没取到数据时也有默认警告");
  });
});

test("涨停样本:红涨绿跌 —— 次日均值为负的打法标 down,为正标 up;封板曲线柱在 0 轴两侧", async () => {
  await withRenderer(async (r) => {
    const { SamplesView } = await r.load("/src/mobile/SamplesPanel.tsx");
    const html = r.html(SamplesView, { data: fixture });
    const section = (name: string) => html.split("<section").find((s) => s.includes(`aria-label="${name}"`)) ?? "";
    assert.match(section("首板·尾盘封"), /class="down">-1\.35%</, "尾盘封次日均值 -1.35% 为绿");
    assert.match(section("首板·早封"), /class="up">\+1\.72%</, "早封 +1.72% 为红");
    const curve = section("首板最后封板时间曲线");
    assert.ok(curve.includes('class="neg"') && curve.includes('class="pos"'));
  });
});

test("涨停样本:样本不足时只说原因,不画统计", async () => {
  await withRenderer(async (r) => {
    const { SamplesView } = await r.load("/src/mobile/SamplesPanel.tsx");
    const data = { ...structuredClone(fixture), available: false, reason: "取到样本的交易日不足(3/30,至少 5 天)", strategies: [], seal_curve: [], missing_days: ["2026-09-01", "2026-09-02"], unsettled_days: ["2026-10-16"] };
    const text = flat(r.html(SamplesView, { data }));
    for (const part of ["这个窗口统计不了:取到样本的交易日不足(3/30,至少 5 天)", "没取到、已剔除的 2 个交易日:2026-09-01、2026-09-02", "还没收盘、没算进去的:2026-10-16"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(!text.includes("按打法分开看"));
  });
});
