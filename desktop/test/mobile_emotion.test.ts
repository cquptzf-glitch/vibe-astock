import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/shortline/emotion 与 /mobile/shortline/cycle 在 2026-10-16 收盘后的真实返回(裁剪了名单长度)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-emotion-2026-10-16.json", import.meta.url), "utf8"));
const open = () => undefined;
// 标签会变成空格:比较时去掉空白,只看用户看得到的字是否按顺序出现。
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("今日情绪:四项读数、周期、昨日对照与快捷入口按程序算出的值展示", async () => {
  await withRenderer(async (r) => {
    const { EmotionTodayView } = await r.load("/src/mobile/EmotionPanels.tsx");
    const text = flat(r.html(EmotionTodayView, { data: fixture.emotion, cycle: { data: fixture.cycle, loading: false, error: "" }, onNavigate: open }));
    for (const part of ["涨停61", "昨日 36（+25）", "炸板率 44.5%", "跌停10", "最高板11 板", "赚钱效应", "中位-3.3%", "再度涨停18.9%",
      "晋级率16.7%", "1进2", "4/30 · 13.3%", "连板溢价", "5板 缺", "情绪周期", "连续两日走强", "第 3 天", "没炸过31", "连板天梯", "不构成买卖建议"]) {
      assert.ok(text.includes(sq(part)), part);
    }
  });
});

test("连板天梯:断层层级显示晋级 0/2 与没接上的结果，反包标签保留", async () => {
  await withRenderer(async (r) => {
    const { LadderView } = await r.load("/src/mobile/EmotionPanels.tsx");
    const text = flat(r.html(LadderView, { data: fixture.emotion, onOpenSymbol: open }));
    for (const part of ["11 板", "新华传媒", "5进6 0/2 · 0%", "今日没有 6 板", "没接上：炸板 1 · 跌停 1", "慈文传媒 炸板+4.71%", "15天11板", "1进2 4/30 · 13.3%"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(!text.includes("首板1只"), "默认不展开首板层");
  });
});

test("涨停全表:三池切换、昨日板数补全、缺失字段显示为 —", async () => {
  await withRenderer(async (r) => {
    const { LimitUpTableView } = await r.load("/src/mobile/EmotionPanels.tsx");
    const zt = flat(r.html(LimitUpTableView, { data: fixture.emotion, onOpenSymbol: open }));
    for (const part of ["涨停 61", "炸板 49", "跌停 10", "显示 4/4 只", "新华传媒", "昨日 10 板", "封单9.08 亿", "首封09:25"]) assert.ok(zt.includes(sq(part)), part);
    const dt = flat(r.html(LimitUpTableView, { data: fixture.emotion, onOpenSymbol: open, initialTab: "dt" }));
    for (const part of ["莫森泰克", "北交所", "连续跌停1 天", "开板2 次", "-29.97%"]) assert.ok(dt.includes(sq(part)), part);
    const zb = flat(r.html(LimitUpTableView, { data: fixture.emotion, onOpenSymbol: open, initialTab: "zb" }));
    assert.ok(zb.includes("盘中触及涨停但收盘未封住"));
    assert.ok(zb.includes("末封") === false);
  });
});

test("盘面事实与行业题材:反馈矩阵、亏钱效应、分制度、行业延续与事件账本", async () => {
  await withRenderer(async (r) => {
    const { MarketFactsView, ThemesView, FirstBoardView } = await r.load("/src/mobile/EmotionPanels.tsx");
    const facts = flat(r.html(MarketFactsView, { data: fixture.emotion, onOpenSymbol: open }));
    for (const part of ["昨日强势股反馈", "首板30", "13.3%", "跌超5%15", "全市场跌停 10 家", "炸板率44.5%", "北交所涨停 2 · 最高 1 板", "晋级 6/34 · 17.6%"]) {
      assert.ok(facts.includes(sq(part)), part);
    }
    const themes = flat(r.html(ThemesView, { data: fixture.emotion, onOpenSymbol: open }));
    for (const part of ["汽车零部", "出版", "昨日 3 只，今日仍涨停 2（延续率 66.7%）", "头部集中度 23%", "今天钱亏在哪", "高位断板", "分歧最大的", "今天钱赚在哪", "元件(5)"]) {
      assert.ok(themes.includes(sq(part)), part);
    }
    const first = flat(r.html(FirstBoardView, { data: fixture.emotion, onOpenSymbol: open }));
    for (const part of ["首板54", "其中反包4", "3天2板", "09:25-09:35"]) assert.ok(first.includes(sq(part)), part);
  });
});

test("取不到的读数写明原因而不是 0；首页速览给出六项摘要", async () => {
  await withRenderer(async (r) => {
    const { FourReadings, HomeEmotionView } = await r.load("/src/mobile/EmotionPanels.tsx");
    const broken = { ...fixture.emotion, money_effect: { available: false, reason: "昨日涨停表现:数据源请求超时" } };
    const text = flat(r.html(FourReadings, { data: broken }));
    assert.ok(text.includes("暂不可用：昨日涨停表现:数据源请求超时"));
    const home = flat(r.html(HomeEmotionView, { data: fixture.emotion, onOpen: open }));
    for (const part of ["情绪速览", "2026-10-16 · 对照 2026-10-15", "涨停61", "1进213.3%", "赚钱效应-3.3%"]) assert.ok(home.includes(sq(part)), part);
  });
});

test("资料库检索同时匹配标题、正文、分类和代码，多个词须同时命中", async () => {
  await withRenderer(async (r) => {
    const { filterDocuments } = await r.load("/src/mobile/LibraryPanel.tsx");
    const items = [
      { id: 1, kind: "note", category: "复盘", title: "周五高标", body: "出版方向 11 板", status: "active", source: "manual", symbol: "600825", created_at: "", updated_at: "" },
      { id: 2, kind: "inbox", category: "线索", title: "汽车零部", body: "首板扎堆", status: "read", source: "manual", symbol: null, created_at: "", updated_at: "" },
    ];
    assert.deepEqual(filterDocuments(items, { keyword: "出版 600825", category: "", unreadOnly: false }).map((i: { id: number }) => i.id), [1]);
    assert.deepEqual(filterDocuments(items, { keyword: "", category: "线索", unreadOnly: false }).map((i: { id: number }) => i.id), [2]);
    assert.deepEqual(filterDocuments(items, { keyword: "", category: "", unreadOnly: true }).map((i: { id: number }) => i.id), [1]);
  });
});
