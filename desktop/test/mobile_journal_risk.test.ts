import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/journal/risk 对一份**合成**交易日志(13 笔已平仓、2 笔没卖完、1 笔补记,不含任何真实成交)的返回。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-journal-risk-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("权益曲线与回撤:累计、峰值、最大回撤起点、去掉最好几笔后的结果,累计为负不给「靠一笔」占比", async () => {
  await withRenderer(async (r) => {
    const { JournalRiskView } = await r.load("/src/mobile/JournalRiskPanel.tsx");
    const html = r.html(JournalRiskView, { data: fixture, onSaved: () => undefined });
    const text = flat(html);
    for (const part of ["权益曲线与回撤", "13 笔已平仓、按平仓日排列", "累计净盈亏-4,099.26", "峰值1,844.07 · 09-28", "当前回撤5,943.33", "最大回撤6,328.97 · 自 09-29",
      "未创新高9 笔", "最长水下9 笔", "盈亏比0.61", "去掉最好 1 笔后-5,401.72", "去掉最好 3 笔后-7,129.38", "最好一笔占累计—(累计不为正)"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /<svg class="jr-chart"[^>]*aria-label="权益曲线:13 笔,累计 -4,099.26 元"/);
    assert.ok(html.includes('class="peak"') && html.includes('class="cum"'));
  });
});

test("近 10/20/50 笔:样本不够的窗口标「样本不足」,终身对照,漂移带符号", async () => {
  await withRenderer(async (r) => {
    const { JournalRiskView } = await r.load("/src/mobile/JournalRiskPanel.tsx");
    const text = flat(r.html(JournalRiskView, { data: fixture, onSaved: () => undefined }));
    for (const part of ["近 10 笔", "近 20 笔样本不足", "近 50 笔样本不足", "终身", "终身统计会把最近的退化藏起来"]) assert.ok(text.includes(sq(part)), part);
    const w = fixture.risk.rolling;
    assert.equal(w.windows["10"].enough, true);
    assert.equal(w.windows["20"].enough, false);
    assert.equal(w.windows["20"].trades, 13, "不够 20 笔时给的是全部,但标明不足");
    assert.ok(typeof w.win_rate_drift === "number");
  });
});

test("规矩检查:每条规矩标「已查 / 没查」及原因,违反记录列出日期和实际值", async () => {
  await withRenderer(async (r) => {
    const { JournalRiskView } = await r.load("/src/mobile/JournalRiskPanel.tsx");
    const text = flat(r.html(JournalRiskView, { data: fixture, onSaved: () => undefined }));
    for (const part of ["按你自己设的规矩逐条对照历史", "单笔最大亏损4已查", "单日最大亏损8已查", "最多同时持仓2没查:有没有成交明细的补记", "计划外交易占比上限0.2已查",
      "2026-10-15单笔最大亏损样本十二 亏 -10.09%规矩 -4 · 实际 -10.09", "2026-09-23计划外交易占比上限2026-09-23 计划外 1/1 笔", "连亏 3 笔之后又做的"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const dflt = structuredClone(fixture);
    dflt.risk.violations.is_default_rules = true;
    assert.ok(flat(r.html(JournalRiskView, { data: dflt, onSaved: () => undefined })).includes(sq("现在用的是默认规矩(只是能跑起来的初值,不是推荐值)")));
  });
});

test("在险资金:没写止损的单独说、总数是下限;分组统计带笔数,持平不进胜率", async () => {
  await withRenderer(async (r) => {
    const { JournalRiskView } = await r.load("/src/mobile/JournalRiskPanel.tsx");
    const text = flat(r.html(JournalRiskView, { data: fixture, onSaved: () => undefined }));
    for (const part of ["没卖完的 2 笔", "总在险(下限)1,500", "占账户1.5%", "没写止损1 笔", "另有 1 笔没写计划止损、占用本金 28,500 元", "下限",
      "样本十五", "未知", "按持有周期", "同日买卖(底仓待核)", "隔日", "持有10天", "还没卖", "未填明细", "对你自己历史行为的统计,不是对市场的预测", "按打法", "按是否按计划", "计划外"]) {
      assert.ok(text.includes(sq(part)), part);
    }
  });
});

test("设置:默认值标「默认」,修改入口展开后有费率、规矩、账户规模输入;个人数据不进 AI 的说明在场", async () => {
  await withRenderer(async (r) => {
    const { SettingsCard, JournalRiskView } = await r.load("/src/mobile/JournalRiskPanel.tsx");
    const collapsed = flat(r.html(SettingsCard, { s: fixture.settings, onSaved: () => undefined }));
    for (const part of ["费率用默认值", "规矩是你填的", "账户规模100,000 元", "默认值只是能跑起来的初值,不是推荐值"]) assert.ok(collapsed.includes(sq(part)), part);
    const all = flat(r.html(JournalRiskView, { data: fixture, onSaved: () => undefined }));
    assert.ok(all.includes(sq("个人成交、持仓、金额只留在本机,不进任何 AI")));
    assert.ok(all.includes(sq("没卖完的交易只出现在在险资金里")));
  });
});

test("页面不登记 AI 页面上下文:源码里没有 useAiPage / pageContext", () => {
  const source = readFileSync(new URL("../src/mobile/JournalRiskPanel.tsx", import.meta.url), "utf8");
  assert.ok(!/useAiPage|pageContext|registerPage/.test(source), "交易日志各页不登记上下文(个人交易数据不进 AI)");
});
