import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

import { withRenderer } from "./helpers/render.ts";

const emotion = JSON.parse(readFileSync(new URL("./fixtures/mobile-emotion-2026-10-16.json", import.meta.url), "utf8")).emotion;

test("今日情绪页文字:只写页面上有的读数,取不到的写原因", async () => {
  await withRenderer(async (r) => {
    const { emotionContext } = await r.load("/src/mobile/aiPageContext.ts");
    const text = emotionContext(emotion);
    assert.ok(text.includes(`复盘日 ${emotion.date}`) && text.includes(`涨停 ${emotion.summary.limit_up}`), text);
    assert.ok(text.includes("赚钱效应") && text.includes("晋级率") && text.includes("梯队"));
    assert.ok(text.includes("只描述已经发生的盘面现象"));
    const broken = { ...emotion, money_effect: { available: false, reason: "东财池子取不到" } };
    const withReason = emotionContext(broken);
    assert.ok(withReason.includes("赚钱效应:暂不可用(东财池子取不到)"), "取不到写原因,不写 0");
  });
});

test("首板名单页文字:逐只一行、带涨停原因;超过 40 只写明;没有原因时叮嘱不要凭印象补", async () => {
  await withRenderer(async (r) => {
    const { firstBoardContext } = await r.load("/src/mobile/aiPageContext.ts");
    assert.equal(firstBoardContext({ ...emotion, first_board: { available: false, reason: "x" } }), null);
    const stocks = Array.from({ length: 45 }, (_, i) => ({ code: String(600000 + i), name: `股${i}`, board: "10cm", boards: 1, label: "", rebound: i === 0, ret: 10, close: 10, amount: 3e8, float_cap: 5e9, turnover: 5, seal_fund: 1e7, first_seal: "09:31:02", last_seal: "09:31:02", broken_times: 0, sector: "机器人" }));
    const d = { ...emotion, first_board: { available: true, total_limit_up: 60, first_count: 45, rebound_count: 1, stocks } };
    const text = firstBoardContext(d, { "600001": "订单增长" });
    assert.ok(text.includes("首板 45 只") && text.includes("另有 5 只未列出"));
    assert.ok(text.includes("股1 600001 | 机器人 | 首封 09:31") && text.includes("原因:订单增长"));
    assert.ok(!text.includes("不要凭印象补原因"), "有原因覆盖时不加这句");
    assert.ok(firstBoardContext(d, {}).includes("不要凭印象补原因"), "没有原因覆盖时叮嘱不要编");
    assert.ok(firstBoardContext(d).includes("不要凭印象补原因"));
  });
});

test("登记表:同样内容不重复通知、超长截断、进入个人页面会清掉", async () => {
  await withRenderer(async (r) => {
    const m = await r.load("/src/mobile/aiPageContext.ts");
    m.clearAiPage();
    let calls = 0;
    // 订阅通过 useSyncExternalStore 内部完成;这里只验证读写与队列
    m.setAiPage("甲", "内容");
    assert.deepEqual([m.getAiPage().title, m.getAiPage().text], ["甲", "内容"]);
    const first = m.getAiPage();
    m.setAiPage("甲", "内容");
    assert.equal(m.getAiPage(), first, "同样内容不换对象");
    m.setAiPage("乙", "字".repeat(9000));
    assert.ok(m.getAiPage().text.length <= 7_520 && m.getAiPage().text.endsWith("(已截断)"));
    m.setAiPage("丙", "   ");
    assert.equal(m.getAiPage().title, "乙", "空内容不登记");
    m.clearAiPage();
    assert.equal(m.getAiPage(), null);
    assert.equal(calls, 0);
    m.queueAiPrompt("  问题  ");
    assert.equal(m.takeQueuedAiPrompt(), "问题");
    assert.equal(m.takeQueuedAiPrompt(), null, "队列取一次就空");
    for (const page of ["hold", "records", "daily", "review", "journalRisk", "journalModes", "journalExcursion", "journalEnv", "journalAttribution", "journalProblems", "monitor", "intraday"]) {
      assert.ok(m.PERSONAL_PAGES.has(page), page);
    }
    assert.ok(!m.PERSONAL_PAGES.has("emotion") && !m.PERSONAL_PAGES.has("firstBoard"));
  });
});

test("个人页面的源码不碰 AI 页面登记(成交、持仓、盈亏不进 AI)", () => {
  const dir = new URL("../src/mobile/", import.meta.url);
  const personal = readdirSync(dir).filter((f) => /^(Journal.*|Portfolio.*|TradeImport|MonitorPanel|IntradayPanel|DailyReviewPanel|AiReviewPanel)\.tsx$/.test(f));
  assert.ok(personal.length >= 8, personal.join(","));
  for (const file of personal) {
    const source = readFileSync(new URL(file, dir), "utf8");
    assert.ok(!/aiPageContext|useRegisterAiPage|setAiPage|queueAiPrompt/.test(source), `${file} 不得登记页面给 AI`);
  }
  const ctx = readFileSync(new URL("aiPageContext.ts", dir), "utf8");
  assert.ok(!/positions|holdings|realized_pnl|fills|avg_cost|equity_base/.test(ctx), "登记模块的文字构造里不出现个人数据字段");
});
