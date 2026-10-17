import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/shortline/prev-broken 在 2026-10-17(周六)对真实上游的返回(复盘日 2026-10-16,前一交易日 2026-10-15 的炸板池 7 只)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-prev-broken-2026-10-16.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const open = () => undefined;

test("真实返回:当天均、翻红、再封板、跌超5%、反馈矩阵「昨日炸板」一行与逐只明细,红涨绿跌", async () => {
  await withRenderer(async (r) => {
    const { PrevBrokenView } = await r.load("/src/mobile/PrevBroken.tsx");
    const html = r.html(PrevBrokenView, { data: fixture, onOpenSymbol: open });
    const text = flat(html);
    for (const part of ["当天均-5.84%", "中位 -5.36%", "翻红0%", "再封板0", "跌超5%4", "昨日炸板7", "晋级涨停", "电魂网络", "炸 2 次 · 游戏Ⅱ", "跌停", "-9.99%",
      "样本 7/7 只(覆盖 100%)", "不在东财「昨日涨停表现」定稿记录里"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /class="down">-5\.84%</);
    assert.ok(!text.includes("还没收盘"), "已收盘的复盘日不带盘中提示");
  });
});

test("盘中、缺行情与不可用:盘中标此刻读数,没取到的票写明,行情不属于复盘日整块说原因而不是 0", async () => {
  await withRenderer(async (r) => {
    const { PrevBrokenView } = await r.load("/src/mobile/PrevBroken.tsx");
    const partial = structuredClone(fixture);
    partial.partial = true;
    partial.result.sample = 5;
    partial.result.missing = 2;
    partial.result.coverage_rate = 0.714;
    const text = flat(r.html(PrevBrokenView, { data: partial, onOpenSymbol: open }));
    for (const part of ["样本 5/7 只(覆盖 71.4%,2 只没取到复盘日行情)", "复盘日还没收盘,这是此刻的读数,收盘前还会变"]) assert.ok(text.includes(sq(part)), part);
    const na = { ...fixture, result: { available: false, reason: "实时行情现在属于 2026-10-16,和复盘日 2026-10-09 不是同一天,今天的价格对不上那天的炸板股,不拿别的日子顶替" } };
    const html = r.html(PrevBrokenView, { data: na, onOpenSymbol: open });
    assert.ok(flat(html).includes(sq("昨日炸板股修复暂不可用:实时行情现在属于 2026-10-16")));
    assert.ok(!html.includes("emo-matrix"), "不可用时不画矩阵");
  });
});
