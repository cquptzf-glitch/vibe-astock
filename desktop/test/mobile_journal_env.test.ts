import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/journal/environment 对一份**合成**交易日志与合成涨停 / 炸板池的返回(12 个入场日 + 1 笔入场日还没收盘;不含任何真实成交或行情)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-journal-env-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("入场环境:情绪强中弱按你自己的日子三等分,板位把炸板和没碰涨停分开", async () => {
  await withRenderer(async (r) => {
    const { JournalEnvView } = await r.load("/src/mobile/JournalEnvPanel.tsx");
    const text = flat(r.html(JournalEnvView, { data: fixture }));
    for (const part of ["查了最近 13 笔、12 个入场日", "情绪分三等分线 163.2 / 223.9", "按入场日情绪(相对你交易过的日子)", "情绪强", "情绪中", "情绪弱", "按当时板位", "首板", "2板", "3板及以上", "炸板", "没碰涨停",
      "这是你自己的行为统计,不是对市场的预测", "1 笔的入场日还没收盘,不分组", "没取到的部分", "不当成「非涨停」", "炸 2 次", "机器人", "算力"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    // 入场日还没收盘的那笔不进逐笔环境
    const rows = fixture.rows.filter((x: { regime: string | null }) => x.regime === null);
    assert.equal(rows.length, 1);
    assert.ok(!text.includes("10-17"));
    const order = fixture.by_board.map((g: { key: string }) => g.key);
    assert.deepEqual(order, ["首板", "2板", "3板及以上", "炸板", "没碰涨停"]);
  });
});

test("入场环境:读数天数不够时不分强中弱;没有交易记录给原因", async () => {
  await withRenderer(async (r) => {
    const { JournalEnvView } = await r.load("/src/mobile/JournalEnvPanel.tsx");
    const data = structuredClone(fixture);
    data.regime_cuts = { low: null, high: null };
    data.gaps = ["取到情绪读数的交易日只有 4 天,不足 6 天,不分强中弱", "2 天的涨停池没取到(本机没存过这天的池子(超过东财保留期约一个月)):2026-08-01、2026-08-02"];
    const text = flat(r.html(JournalEnvView, { data }));
    assert.ok(text.includes(sq("不分强中弱(情绪读数的交易日不够)")));
    assert.ok(text.includes(sq("2 天的涨停池没取到")));
    assert.ok(flat(r.html(JournalEnvView, { data: { available: false, reason: "还没有交易记录" } })).includes("还没有交易记录"));
  });
});

test("页面不登记 AI 页面上下文:源码里没有 useAiPage / pageContext", () => {
  const source = readFileSync(new URL("../src/mobile/JournalEnvPanel.tsx", import.meta.url), "utf8");
  assert.ok(!/useAiPage|pageContext|registerPage/.test(source), "交易日志各页不登记上下文(个人交易数据不进 AI)");
});
