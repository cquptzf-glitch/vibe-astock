import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/journal/problems 对一份**合成**交易日志的返回(2 笔卖超 / 卖在买之前、1 条空记录、1 条没填盈亏的补记、1 条 ETF;不含任何真实成交)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-journal-problems-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;

test("问题概览与逐条问题:卖在买之前、卖超、空记录、没填盈亏的补记,各给原因", async () => {
  await withRenderer(async (r) => {
    const { ProblemsView } = await r.load("/src/mobile/JournalProblemsPanel.tsx");
    const text = flat(r.html(ProblemsView, { data: fixture, onChanged: noop }));
    for (const part of ["共 6 条记录、7 笔成交", "需要处理 4 处", "卖出比持有多 2", "没有成交 1", "补记没填盈亏 1", "样本B", "样本C", "卖在买之前,或买入日期写晚了", "当时只持有 800 股,超出的部分结算时会被忽略",
      "这条记录没有任何成交,算不出盈亏", "补记没填盈亏 %,不进任何统计", "没填费用的成交 6 笔(按费率估,不算问题)", "删除整条记录", "修复只动你点的那一笔;改完不能比改之前更糟"]) {
      assert.ok(text.includes(sq(part)), part);
    }
  });
});

test("不是 A 股股票的记录只列出来、不算错误,也不给删除入口", async () => {
  await withRenderer(async (r) => {
    const { ProblemsView } = await r.load("/src/mobile/JournalProblemsPanel.tsx");
    const html = r.html(ProblemsView, { data: fixture, onChanged: noop });
    const text = flat(html);
    for (const part of ["不是 A 股股票的记录", "沪深300ETF样本", "510300", "只计入持仓,不进交易日志的任何统计", "这不是错误,不用处理"]) assert.ok(text.includes(sq(part)), part);
    const nonStockSection = html.slice(html.indexOf("不是 A 股股票的记录"));
    assert.ok(!nonStockSection.includes("删除整条记录"), "ETF 记录属于持仓,这里不提供删除");
  });
});

test("成交行:有问题的那笔标出原因,改 / 删各有入口;没问题时给明确说法", async () => {
  await withRenderer(async (r) => {
    const { FillRow, ProblemsView } = await r.load("/src/mobile/JournalProblemsPanel.tsx");
    const fill = { id: 7, type: "sell", date: "2026-10-13", price: 21, shares: 500, fee: null };
    const flagged = { kind: "oversell", fill_id: 7, why: "2026-10-13 卖出 500 股,但那天之前没有买入 —— 卖在买之前,或买入日期写晚了" };
    const html = r.html(FillRow, { fill, flagged, act: async () => true });
    const text = flat(html);
    assert.ok(text.includes(sq("卖 500 股 @ 21")) && text.includes(sq("费用 没填(按费率估)")) && text.includes(sq("卖在买之前")));
    assert.ok(html.includes('aria-label="改 卖出 2026-10-13"') && html.includes('aria-label="删除 卖出 2026-10-13"'));
    const clean = flat(r.html(ProblemsView, { data: { ...fixture, problem_count: 0, counts: {}, entries: [], non_stock: [] }, onChanged: noop }));
    assert.ok(clean.includes("没有发现算不进统计的记录"));
  });
});

test("页面不登记 AI 页面上下文:源码里没有 useAiPage / pageContext", () => {
  const source = readFileSync(new URL("../src/mobile/JournalProblemsPanel.tsx", import.meta.url), "utf8");
  assert.ok(!/useAiPage|pageContext|registerPage/.test(source), "交易日志各页不登记上下文(个人交易数据不进 AI)");
});
