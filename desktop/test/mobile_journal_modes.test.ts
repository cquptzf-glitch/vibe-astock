import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/journal/modes 对一份**合成**交易日志的返回(2 张卡:一张两个版本样本够、一张刚建;不含任何真实成交)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-journal-modes-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("模式卡:按版本分段、版本间比较、建卡之前的交易单独列、样本不够标星", async () => {
  await withRenderer(async (r) => {
    const { JournalModesView } = await r.load("/src/mobile/JournalModesPanel.tsx");
    const text = flat(r.html(JournalModesView, { data: fixture, onChanged: () => undefined }));
    for (const part of ["首板早封", "2 个版本 · 对上 14 笔", "v1", "v2", "收紧封板时间", "胜率 +26.2 个点", "中位收益 +2.7%", "不是对下一笔的预测",
      "另有 1 笔发生在建卡之前", "不进任何版本", "低吸试验", "* 不到 6 笔", "建卡当天算第一版,之后的修改从次日生效"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(text.includes(sq("模式卡是你自己的打法,只留在本机、进备份,不进任何 AI")));
    assert.ok(text.includes(sq("不预填任何规则内容")));
  });
});

test("样本不够不比较:有两个版本但样本不足时写明「暂不比较」而不是给一个数", async () => {
  await withRenderer(async (r) => {
    const { JournalModesView } = await r.load("/src/mobile/JournalModesPanel.tsx");
    const data = structuredClone(fixture);
    const card = data.modes.cards[0];
    card.latest_vs_prev = null;
    card.compare_blocked = true;
    card.by_version[1].enough = false;
    const text = flat(r.html(JournalModesView, { data, onChanged: () => undefined }));
    assert.ok(text.includes(sq("版本样本不够或历史不完整,暂不比较")));
    assert.ok(!text.includes("胜率+26.2"));
  });
});

test("没有模式卡:给出原因,不预填任何卡;被排除的非 A 股记录说明", async () => {
  await withRenderer(async (r) => {
    const { JournalModesView } = await r.load("/src/mobile/JournalModesPanel.tsx");
    const data = structuredClone(fixture);
    data.modes = { available: false, reason: "还没有模式卡 —— 先把「我这套打法」写下来,统计才有分段依据" };
    data.excluded_non_stock = 2;
    const html = r.html(JournalModesView, { data, onChanged: () => undefined });
    const text = flat(html);
    assert.ok(text.includes(sq("还没有模式卡")));
    assert.ok(text.includes(sq("另有 2 条记录不是 A 股股票")));
    assert.ok(!html.includes("<textarea"), "没点新建就不出现编辑框,更不预填内容");
  });
});

test("编辑器:新建说明第一版从今天起、编辑说明次日生效且只改名不开新版本;每段字数上限写明", async () => {
  await withRenderer(async (r) => {
    const { ModeEditor } = await r.load("/src/mobile/JournalModesPanel.tsx");
    const blank = { card_id: null, name: "", playbook: "打板", changes: "", text: { setup: "", entry: "", exit: "", sizing: "", phase: "" } };
    const fresh = flat(r.html(ModeEditor, { draft: blank, data: fixture, onSaved: () => undefined, onCancel: () => undefined }));
    for (const part of ["第一版从今天起生效,不回溯到更早的交易", "每段最多 800 字,超长会被拒收、不会被截断", "什么情况下做", "怎么进", "怎么出", "仓位", "适用的情绪阶段"]) assert.ok(fresh.includes(sq(part)), part);
    assert.ok(!fresh.includes(sq("这次改了什么")), "新建没有「改了什么」");
    const edit = flat(r.html(ModeEditor, { draft: { ...blank, card_id: 3, name: "首板早封", text: { ...blank.text, setup: "9:45 前封板" } }, data: fixture, onSaved: () => undefined, onCancel: () => undefined }));
    for (const part of ["这次改了什么", "从明天起生效", "只改名字不开新版本", "9:45 前封板"]) assert.ok(edit.includes(sq(part)), part);
  });
});

test("页面不登记 AI 页面上下文:源码里没有 useAiPage / pageContext", () => {
  const source = readFileSync(new URL("../src/mobile/JournalModesPanel.tsx", import.meta.url), "utf8");
  assert.ok(!/useAiPage|pageContext|registerPage/.test(source), "模式卡是用户自己的打法,不登记上下文(个人交易数据不进 AI)");
});
