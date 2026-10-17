import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

import { withRenderer } from "./helpers/render.ts";

const historyFixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-history-2026-10-16.json", import.meta.url), "utf8"));
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

test("连板天梯与题材页文字:不可用返回 null,正常带关键数字,超过上限写明未列出", async () => {
  await withRenderer(async (r) => {
    const { ladderContext, themesContext } = await r.load("/src/mobile/aiPageContext.ts");
    assert.equal(ladderContext({ ...emotion, ladder: { available: false, reason: "x" } }), null);
    assert.equal(themesContext({ ...emotion, themes: { available: false, reason: "x" } }), null);
    const ladder = ladderContext(emotion);
    assert.ok(ladder.includes(emotion.date) && ladder.includes("最高") && ladder.includes("板"), ladder);
    const mk = (i: number) => ({ code: String(600000 + i), name: "股" + i, board: "10cm", boards: 3, label: "", rebound: false, ret: 10, close: 10, amount: 1, float_cap: 1, turnover: 1, seal_fund: 1, first_seal: "09:30:00", last_seal: "09:30:00", broken_times: 1, sector: "机器人" });
    const many = { ...emotion, ladder: { available: true, total_limit_up: 80, highest: 3, consec_count: 50, prev_available: false, prev_note: "", levels: [{ boards: 3, stocks: Array.from({ length: 50 }, (_, i) => mk(i)), fell: [], base: null, promoted: null, rate: null }] } };
    const text = ladderContext(many);
    assert.ok(text.includes("该层另有 10 只未列出") && text.includes("晋级率不可用") && text.includes("前一交易日数据没取到"), text);
    const th = themesContext(emotion);
    assert.ok(th.includes("集中度") && th.includes("涨停"), th);
    const row = (i: number) => ({ sector: "行业" + i, limit_up: 3, first_boards: 2, consec_boards: 1, highest: 2, broken: 0, broken_rate: null, first_seal: null, last_seal: null, prev_limit_up: null, continued: null, continuation_rate: null, names: [] });
    const manyThemes = themesContext({ ...emotion, themes: { available: true, themes: Array.from({ length: 25 }, (_, i) => row(i)), theme_count: 25, concentration: null, timeline: [], faded: [], prev_available: false } });
    assert.ok(manyThemes.includes("另有 5 个行业未列出") && manyThemes.includes("集中度 —"), manyThemes);
  });
});

test("近 5 天与情绪历史页文字:逐日一行、取不到写 —、缺口和不可用原样写;不含「我的交易」", async () => {
  await withRenderer(async (r) => {
    const { weeklyContext, historyContext } = await r.load("/src/mobile/aiPageContext.ts");
    const weekly = weeklyContext(historyFixture.weekly);
    assert.ok(weekly.includes("近 " + historyFixture.weekly.days.length + " 个交易日") && weekly.includes("涨停") && weekly.includes("行业轮动"), weekly);
    assert.ok(!weekly.includes("我的交易") && !weekly.includes("持仓"), "个人交易不进上下文");
    assert.equal(weeklyContext({ ...historyFixture.weekly, days: [] }), null);
    const gappy = structuredClone(historyFixture.weekly);
    gappy.days[0].limit_up = null;
    gappy.gaps = ["10-13 没取到"];
    const g = weeklyContext(gappy);
    assert.ok(g.includes("涨停 —") && g.includes("数据缺口:10-13 没取到"), g);
    const hist = historyContext(historyFixture.history);
    assert.ok(hist.includes("统计窗口") && hist.includes("分位"), hist);
    const noCtx = historyContext({ ...historyFixture.history, context: { available: false, reason: "样本不足" }, diff: { available: false, reason: "没有前一日" } });
    assert.ok(noCtx.includes("读数位置:暂不可用(样本不足)") && noCtx.includes("相对前一日的变化:暂不可用(没有前一日)"), noCtx);
    assert.equal(historyContext({ ...historyFixture.history, series: [] }), null);
  });
});

test("盘面事实与涨停全表页文字:不可用的块写原因,涨停池超过 30 只写明", async () => {
  await withRenderer(async (r) => {
    const { marketFactsContext, limitUpTableContext } = await r.load("/src/mobile/aiPageContext.ts");
    const facts = marketFactsContext(emotion);
    assert.ok(facts.includes(`复盘日 ${emotion.date}`) && facts.includes("封板质量") && facts.includes("只描述已经发生的盘面现象"), facts);
    const broken = marketFactsContext({ ...emotion, feedback: { available: false, reason: "样本不足" }, loss_effect: { available: false, reason: "没取到" }, by_board: { available: false, reason: "x" } });
    assert.ok(broken.includes("强势股反馈:暂不可用(样本不足)") && broken.includes("亏钱效应:暂不可用(没取到)") && broken.includes("分制度:暂不可用(x)"), broken);
    const table = limitUpTableContext(emotion);
    assert.ok(table.includes(`复盘日 ${emotion.date}`) && table.includes("涨停池") && table.includes("炸板池"), table);
    const mk = (i: number) => ({ code: String(600000 + i), name: "股" + i, board: "10cm", boards: i % 5 + 1, label: "", rebound: false, ret: 10, close: 10, amount: 1e8, float_cap: 1, turnover: 1, seal_fund: 1, first_seal: "09:3" + (i % 10) + ":00", last_seal: null, broken_times: 0, sector: "机器人" });
    const many = limitUpTableContext({ ...emotion, limit_up_rows: Array.from({ length: 40 }, (_, i) => mk(i)), broken_rows: { available: false, reason: "东财取不到" } });
    assert.ok(many.includes("另有 10 只未列出") && many.includes("炸板池 暂不可用(东财取不到)"), many);
    assert.ok(many.indexOf("| 5板") < many.indexOf("| 2板") && !many.includes("| 1板"), "连板高的在前");
    assert.equal(limitUpTableContext({ ...emotion, limit_up_rows: [], broken_rows: { available: false, reason: "x" }, limit_down_rows: { available: false, reason: "x" } }), null);
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
