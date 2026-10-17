import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/review/market 在 2026-10-17(周六)对真实上游的返回(复盘日 2026-10-16),
// 短线格子用同一天的情绪样本(和今日情绪同一份数据)。
const panel = JSON.parse(readFileSync(new URL("./fixtures/mobile-market-panel-2026-10-16.json", import.meta.url), "utf8"));
const emotion = JSON.parse(readFileSync(new URL("./fixtures/mobile-emotion-2026-10-16.json", import.meta.url), "utf8")).emotion;
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const open = () => undefined;

test("市场状态八格:成交额与 20/60 日均的比、短线格子复用情绪数据、站上均线的指数个数、两融与国债", async () => {
  await withRenderer(async (r) => {
    const { MarketPanelView } = await r.load("/src/mobile/MarketPanel.tsx");
    const text = flat(r.html(MarketPanelView, { data: panel, emotion, onOpenSymbol: open }));
    for (const part of ["市场状态", "两市成交额 · 10-16", "15,554.14 亿", "比 20 日均-9.8%", "60 日均-24.07%", "涨停 / 跌停", "61", "连板高度11 板", "昨日涨停今日", "站上 20 日线的指数0/8", "站上 60 日线 0/8", "250 日线 0/8",
      "两融余额 · 10-15", "25,031.92 亿", "沪深300 · 10-16", "14.05 倍", "股息率 2.8% · 10 年国债 1.648%"]) {
      assert.ok(text.includes(sq(part)), part);
    }
  });
});

test("主要指数:各周期涨跌、年初至今、离一年高点、均线位置;红涨绿跌", async () => {
  await withRenderer(async (r) => {
    const { MarketPanelView } = await r.load("/src/mobile/MarketPanel.tsx");
    const html = r.html(MarketPanelView, { data: panel, emotion, onOpenSymbol: open });
    const table = html.match(/<table[^>]*aria-label="主要指数中长线"[\s\S]*?<\/table>/)![0];
    const text = flat(table);
    for (const part of ["上证指数", "3,716.28", "+0.24%", "-5.54%", "-2.92%", "-6.36%", "-12.4%", "创业板指", "北证50", "科创50", "+1.07%", "下 / 下 / 下"]) assert.ok(text.includes(sq(part)), part);
    assert.match(table, /class="up">\+0\.24%</);
    assert.match(table, /class="down">-5\.54%</);
    const missing = structuredClone(panel);
    missing.trend.rows[0].chg250 = null;
    missing.trend.rows[0].window_full = false;
    missing.trend.failed = [{ code: "899050", name: "北证50", error: "东财 日线没有数据" }];
    const t2 = flat(r.html(MarketPanelView, { data: missing, emotion, onOpenSymbol: open }));
    assert.ok(t2.includes(sq("没取到:北证50(东财 日线没有数据)")) && t2.includes(sq("不用短窗口顶替")));
  });
});

test("每一块各自标缺口:取不到写原因、不显示 0;盘中累计不和均值比;不是复盘日的快照标出来", async () => {
  await withRenderer(async (r) => {
    const { MarketPanelView } = await r.load("/src/mobile/MarketPanel.tsx");
    const gap = structuredClone(panel);
    gap.margin = { available: false, reason: "两融没取到:数据源请求超时" };
    gap.dragon = { available: false, reason: "2026-10-16 没有龙虎榜数据(非交易日或盘后还没更新),不当成 0 条" };
    gap.amount = { ...gap.amount, partial: true, vs_ma20: null, vs_ma60: null, prev: 1633000000000 };
    gap.board_flow.matches_review_day = false;
    gap.notes = ["复盘日还没收盘:指数最后一根和两市成交额是盘中累计,不和整天的均值比"];
    const text = flat(r.html(MarketPanelView, { data: gap, emotion: null, onOpenSymbol: open }));
    for (const part of ["这次没取到:两融没取到:数据源请求超时", "这次没取到:2026-10-16 没有龙虎榜数据", "两市成交额 · 10-16 · 盘中累计", "还没收完 · 上一个交易日 16,330 亿", "不是复盘日的数据", "复盘日还没收盘", "情绪数据没取到"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(!text.includes("比 20 日均"), "盘中累计不和整天均值比");
  });
});

test("全球市场、行业资金(新浪口径说明)、成交额居前 20、龙虎榜净买与净卖", async () => {
  await withRenderer(async (r) => {
    const { MarketPanelView } = await r.load("/src/mobile/MarketPanel.tsx");
    const html = r.html(MarketPanelView, { data: panel, emotion, onOpenSymbol: open });
    const text = flat(html);
    for (const part of ["全球市场", "道琼斯", "52,002.41", "-0.34%", "纳斯达克综合", "-2.14%", "恒生科技", "+1.16%", "和东财「主力净额」不是一个口径", "净流入居前", "汽车制造", "净流出居前",
      "全市场成交额居前 20", "中际旭创", "龙虎榜 · 2026-10-16", "共 66 条上榜记录", "药明康德", "同一只可能因多条上榜理由重复出现"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.equal((html.match(/aria-label="成交额居前 20"[\s\S]*?<\/div><h3/)?.[0].match(/watch-row/g) ?? []).length, 20);
    assert.ok(panel.global.rows.every((x: { available: boolean }) => x.available));
  });
});

test("沪深 300 估值:市盈率 / 股息率(总股本口径,另列计算用股本)与国债收益率的差;取不到时说明原因、第 8 格回到国债", async () => {
  await withRenderer(async (r) => {
    const { MarketPanelView } = await r.load("/src/mobile/MarketPanel.tsx");
    const text = flat(r.html(MarketPanelView, { data: panel, emotion, onOpenSymbol: open }));
    for (const part of ["沪深 300 市盈率 14.05 倍(计算用股本 15.66)", "股息率 2.8%(计算用股本 2.6%)", "股息率 − 国债 +1.15 个百分点", "盈利收益率(100 ÷ 市盈率)7.12% − 国债 +5.47 个百分点", "不是估值判断", "不含市净率"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const gap = structuredClone(panel);
    gap.valuation = { available: false, reason: "沪深 300 估值没取到:中证官网:数据源请求超时" };
    const t2 = flat(r.html(MarketPanelView, { data: gap, emotion, onOpenSymbol: open }));
    assert.ok(t2.includes(sq("沪深 300 估值这次没取到:沪深 300 估值没取到:中证官网")) && t2.includes(sq("10 年国债 · 10-16")) && t2.includes("1.648%"));
    assert.ok(!t2.includes("14.05 倍"));
    const stale = structuredClone(panel);
    stale.valuation.stale_days = 3;
    assert.ok(flat(r.html(MarketPanelView, { data: stale, emotion, onOpenSymbol: open })).includes(sq("估值文件最新一条是 3 天前的")));
  });
});
