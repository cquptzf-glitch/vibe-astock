import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/journal/attribution 对一份**合成**交易日志与合成验证条件对账的返回(13 个入场日,不含任何真实成交或复盘)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-journal-attribution-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("四象限:判对/判错 × 赚/亏,判错还赚钱单独点成「运气」,执行问题与判断问题分开", async () => {
  await withRenderer(async (r) => {
    const { AttributionView } = await r.load("/src/mobile/JournalAttributionPanel.tsx");
    const html = r.html(AttributionView, { data: fixture });
    const text = flat(html);
    for (const part of ["看错了,还是没执行好", "判对 + 赚钱", "判对 + 亏钱 · 执行问题", "判错 + 赚钱 · 运气", "判错 + 亏钱 · 判断问题", "4 天 · +1,415 元", "2 天 · -165 元", "2 天 · +450 元", "3 天 · -930 元",
      "共 11 个有盈亏的交易日(2026-09-18 至 2026-10-16)", "看对了却亏钱和看错了才亏钱是两个病", "盈亏为 0 的日子不进任何一格", "1 笔那天前一晚没写验证条件", "1 笔那天的条件一半成立一半不成立,不算判对也不算判错",
      "1 笔没有成交明细或还没卖光,算不出金额", "2 个交易日没取到读数", "3/3 成立", "1/3 成立"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(html.includes('class="luck"'), "运气那格有单独的样式");
    assert.ok(!/建议|应该改/.test(text.replace("看对了却亏钱和看错了才亏钱是两个病:一个要改执行,一个要改读盘", "")), "不给操作建议");
  });
});

test("样本不到 8 天提示别急着下结论;没有对完账的条件时给原因而不是空四格", async () => {
  await withRenderer(async (r) => {
    const { AttributionView } = await r.load("/src/mobile/JournalAttributionPanel.tsx");
    const few = structuredClone(fixture);
    few.enough_samples = false;
    few.days_counted = 5;
    assert.ok(flat(r.html(AttributionView, { data: few })).includes(sq("不到 8 天,样本太少,别急着下结论")));
    const none = flat(r.html(AttributionView, { data: { available: false, reason: "最近还没有对完账的验证条件(「明日验证」里写给下一场的条件)—— 没有「判对没判对」,就没法做归因", skipped_no_read: 3 } }));
    assert.ok(none.includes(sq("最近还没有对完账的验证条件")));
    assert.ok(none.includes(sq("3 笔那天前一晚没写验证条件")));
    assert.ok(!none.includes("判对 + 赚钱"));
  });
});

test("页面不登记 AI 页面上下文:源码里没有 useAiPage / pageContext", () => {
  const source = readFileSync(new URL("../src/mobile/JournalAttributionPanel.tsx", import.meta.url), "utf8");
  assert.ok(!/useAiPage|pageContext|registerPage/.test(source), "交易日志各页不登记上下文(个人交易数据不进 AI)");
});
