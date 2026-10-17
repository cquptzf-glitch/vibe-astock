import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/journal/{inbox,excursion} 对一份**合成**交易日志与合成日线的返回(12 笔已平仓 + 1 笔没卖完 + 1 笔北交所 + 1 笔补记;不含任何真实成交或行情)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-journal-excursion-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;

test("复查清单:每笔列出为什么进来,按日期倒序;基准来自用户自己,不给建议", async () => {
  await withRenderer(async (r) => {
    const { InboxCard } = await r.load("/src/mobile/JournalExcursionPanel.tsx");
    const text = flat(r.html(InboxCard, { inbox: fixture.inbox }));
    for (const part of ["复查清单", "查了最近 15 笔", "值得回头看 11 笔", "你的中位仓位 14,600 元、中位持有 1 天", "亏超自己的上限", "仓位偏大", "持有偏久", "没写止损", "扛过大幅浮亏", "没吃到行情",
      "占用 48,000 元,是你中位仓位(14,600)的 3.3 倍", "还在手上,但没写计划止损 —— 最坏情况无从估计", "自己标了「计划外」", "这里只说哪里不寻常,不说该怎么做"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const dates = fixture.inbox.items.map((i: { date: string }) => i.date);
    assert.deepEqual(dates, [...dates].sort().reverse());
    assert.ok(!/建议买入|建议卖出|应该卖/.test(text));
  });
});

test("复查清单:样本不足 8 笔不做相对自己习惯的判定;行情没缓存时提示去取", async () => {
  await withRenderer(async (r) => {
    const { InboxCard } = await r.load("/src/mobile/JournalExcursionPanel.tsx");
    const data = structuredClone(fixture.inbox);
    data.baseline = { median_capital: null, median_hold_days: null, history_enough: false, min_history: 8 };
    data.excursion_hint = "3 笔的行情还没缓存,扛大幅浮亏 / 没吃到行情这两类判定暂时跳过了";
    data.rules_is_default = true;
    const text = flat(r.html(InboxCard, { inbox: data }));
    assert.ok(text.includes(sq("不到 8 笔,不做「相对你自己习惯」的判定")));
    assert.ok(text.includes(sq("3 笔的行情还没缓存")));
    assert.ok(text.includes(sq("默认规矩(不是推荐值)")));
    const empty = flat(r.html(InboxCard, { inbox: { available: false, reason: "还没有交易记录" } }));
    assert.ok(empty.includes("还没有交易记录"));
  });
});

test("最大浮盈浮亏:汇总用中位数,偏差说明与读数摆在一起,同日进出标上界,涨过却亏着走有说明", async () => {
  await withRenderer(async (r) => {
    const { ExcursionCard } = await r.load("/src/mobile/JournalExcursionPanel.tsx");
    const text = flat(r.html(ExcursionCard, { data: fixture.excursion, busy: false, onFetch: noop }));
    for (const part of ["捕获率中位数83.3%", "算了几笔12 笔", "赚钱但扛过 ≥5% 浮亏1 笔", "亏钱且没怎么涨过3 笔", "同日进出(只给上界)1 笔", "涨过却亏着走1 笔",
      "最大浮盈取的是日线高点,含「买入之前」的那部分", "系统性低估", "日线看不出最高点发生在盘中买入之前还是之后", "遇到除息,低点会偏低", "疑似大比例送转", "几乎没吃到这段行情",
      "这段涨过,但最后是亏损离场", "1 笔是北交所", "确定 +12%", "同日进出"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(text.includes(sq("没算进来的:2 笔还没卖光或没有成交明细")));
    // 行按入场日新的在前
    const dates = fixture.excursion.items.map((i: { date: string }) => i.date);
    assert.deepEqual(dates, [...dates].sort().reverse());
  });
});

test("最大浮盈浮亏:行情没取过时不联网、说明原因并给「取行情并计算」入口;取失败的逐笔列原因", async () => {
  await withRenderer(async (r) => {
    const { ExcursionCard } = await r.load("/src/mobile/JournalExcursionPanel.tsx");
    const html = r.html(ExcursionCard, { data: fixture.cold, busy: false, onFetch: noop });
    const text = flat(html);
    assert.ok(text.includes(sq("取行情并计算")));
    assert.ok(text.includes(sq("12 笔还没取过行情")));
    assert.ok(text.includes(sq("行情还没取过")));
    const failed = structuredClone(fixture.excursion);
    failed.failures = [{ key: "mobile:3", symbol: "600103", name: "样本C", date: "2026-09-23", why: "网络断了" }];
    failed.counts.failed = 1;
    const ft = flat(r.html(ExcursionCard, { data: failed, busy: false, onFetch: noop }));
    assert.ok(ft.includes(sq("行情没取到:网络断了")));
    assert.ok(ft.includes(sq("取行情并计算")), "还有失败的笔,允许再取一次");
    const few = structuredClone(fixture.excursion);
    few.enough_samples = false;
    few.trades = 3;
    assert.ok(flat(r.html(ExcursionCard, { data: few, busy: false, onFetch: noop })).includes(sq("只有 3 笔,不到 6 笔")));
  });
});

test("页面不登记 AI 页面上下文:源码里没有 useAiPage / pageContext", () => {
  const source = readFileSync(new URL("../src/mobile/JournalExcursionPanel.tsx", import.meta.url), "utf8");
  assert.ok(!/useAiPage|pageContext|registerPage/.test(source), "交易日志各页不登记上下文(个人交易数据不进 AI)");
});
