import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/review/tiers 在 2026-10-17(周六)对真实上游的返回(复盘日 2026-10-16)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-review-tiers-2026-10-16.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("涨停分档:四档只数与规则、默认展开连续集中、前两个交易日、最高板口径说明", async () => {
  await withRenderer(async (r) => {
    const { TiersView } = await r.load("/src/mobile/ReviewTiers.tsx");
    const html = r.html(TiersView, { data: fixture, onOpenSymbol: () => undefined });
    const text = flat(html);
    const by = (id: string) => fixture.tiers.find((t: { id: string }) => t.id === id);
    for (const part of ["连续集中", `${by("mainline").stocks}${by("mainline").industries} 个行业`, "今天集中", "零散", "单只", "今天同一行业 ≥ 3 只涨停,前两个交易日每天也 ≥ 2 只",
      "前两个交易日:2026-10-15、2026-10-14", "第一只只是当天的最高板,不是推荐", "显示全部档位"]) assert.ok(text.includes(sq(part)), part);
    const mainline = fixture.groups.find((g: { tier: string }) => g.tier === "mainline");
    assert.ok(text.includes(sq(mainline.industry)), "连续集中的行业默认可见");
    assert.match(html, /class="emo-state strong"/);
    // 默认只展开连续集中,「单只」行业不出现
    const single = fixture.groups.find((g: { tier: string }) => g.tier === "single");
    assert.ok(!text.includes(sq(`${single.industry}单只`)) || single.industry === mainline.industry);
    assert.equal(fixture.tiers.reduce((n: number, t: { stocks: number }) => n + t.stocks, 0), fixture.total, "四档只数加起来 = 当天涨停家数");
  });
});

test("涨停池没取到时说明原因、不分档", async () => {
  await withRenderer(async (r) => {
    const { TiersView } = await r.load("/src/mobile/ReviewTiers.tsx");
    const data = { ...structuredClone(fixture), total: null, groups: [], tiers: fixture.tiers.map((t: object) => ({ ...t, industries: 0, stocks: 0 })), gaps: ["2026-10-16 的涨停池没取到(数据源请求超时),没法分档"] };
    const text = flat(r.html(TiersView, { data, onOpenSymbol: () => undefined }));
    assert.ok(text.includes(sq("这一天的涨停池没取到,没法分档")) && text.includes(sq("没法分档")));
  });
});
