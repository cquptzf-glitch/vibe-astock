import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 合成数据(不是任何真实持仓):由 Rust Core 的 portfolio_perf / portfolio_xray 的 compute 对合成价格序列算出。
const perf = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-perf-synthetic.json", import.meta.url), "utf8"));
const replay = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-replay-synthetic.json", import.meta.url), "utf8"));
const xray = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-xray-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("收益表现:四个读数、走势线、区间与投入取出、逐只盈亏、缺口与说明;红涨绿跌", async () => {
  await withRenderer(async (r) => {
    const { PerfView } = await r.load("/src/mobile/PortfolioAnalysis.tsx");
    const html = r.html(PerfView, { data: perf });
    const text = flat(html);
    for (const part of ["时间加权", "+9.33%", "资金加权", "+7.86%", "年化(XIRR)", "+8.07%", "累计盈亏", "样本白酒", "样本银行", "样本北交所", "688001 行情源未返回价格", "北交所使用东财不复权日线", "已记录送转按股数换算", "只给数字,不评好坏"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /<svg[^>]*aria-label="时间加权收益走势"/);
    assert.match(html, /class="up">\+9\.33%/);
    const short = flat(r.html(PerfView, { data: { ...perf, span_days: 40, xirr_pct: null, xirr_note: "区间 40 天不满一年,年化会把短期涨跌放大,不给年化数", warnings: ["600519 在 2026-01-06 收盘价比 2026-01-05 变了 -50%:可能是送转拆股没有记录"] } }));
    for (const part of ["不满一年不给", "区间 40 天不满一年", "可能是送转拆股没有记录"]) assert.ok(short.includes(sq(part)), part);
    const noIrr = flat(r.html(PerfView, { data: { ...perf, span_days: 400, xirr_pct: null, xirr_note: "现金流解不出年化收益率" } }));
    assert.ok(noIrr.includes("现金流无法求解") && !noIrr.includes("不满一年不给"));
    const bad = flat(r.html(PerfView, { data: { ok: false, reason: "还没有买入成交,算不出收益。", gaps: [{ symbol: "000001", why: "取不到价格" }] } }));
    assert.ok(bad.includes(sq("算不了:还没有买入成交")) && bad.includes("000001取不到价格"));
  });
});

test("持仓体检:集中度、权重与行业(没取到的单列)、风险读数、每只风险贡献、缺口;提醒线只查用户设了的", async () => {
  await withRenderer(async (r) => {
    const { XrayView, NO_THRESHOLDS } = await r.load("/src/mobile/PortfolioAnalysis.tsx");
    const html = r.html(XrayView, { data: xray, thresholds: NO_THRESHOLDS });
    const text = flat(html);
    for (const part of ["单只最大", "等效只数", "最大行业", "还没设提醒线", "样本光模块", "行业没取到", "白酒", "年化波动", "最大回撤", "历史 VaR95", "下行波动", "平均相关", "分散化比", "风险贡献",
      "688001 日线历史太短", "920575 取不到日线", "不出综合分、不评好坏"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.ok(!text.includes("超过你设的"), "没设线不查");
    const hit = flat(r.html(XrayView, { data: xray, thresholds: { top1: 30, industry: 30, vol: 10, drawdown: 5 } }));
    for (const part of ["单只最大占 37.26%,超过你设的 30%", "最大行业占 32.87%,超过你设的 30%", "超过你设的 10%", "最大回撤"]) assert.ok(hit.includes(sq(part)), part);
    const ok = flat(r.html(XrayView, { data: xray, thresholds: { top1: 90, industry: 90, vol: 900, drawdown: 90 } }));
    assert.ok(ok.includes("没有超过你设的提醒线"));
    const noRisk = flat(r.html(XrayView, { data: { ...xray, risk: null, risk_reason: "共同交易日只有 30 个,不足 60 个" }, thresholds: { top1: null, industry: null, vol: 1, drawdown: 1 } }));
    assert.ok(noRisk.includes(sq("风险读数算不了:共同交易日只有 30 个")) && !noRisk.includes(sq("超过你设的 1%")), "风险没算出来时不查波动和回撤线");
  });
});

test("提醒线:非法值当没设;超线判断;编辑器带四项;卡片默认不联网只给「开始」按钮", async () => {
  await withRenderer(async (r) => {
    const m = await r.load("/src/mobile/PortfolioAnalysis.tsx");
    assert.deepEqual(m.parseThresholds(JSON.stringify({ top1: 30, industry: -1, vol: "x", drawdown: 0 })), { top1: 30, industry: null, vol: null, drawdown: null });
    assert.deepEqual(m.parseThresholds("坏的"), m.NO_THRESHOLDS);
    assert.deepEqual(m.parseThresholds(null), m.NO_THRESHOLDS);
    const editor = flat(r.html(m.ThresholdEditor, { value: { top1: 30, industry: null, vol: null, drawdown: 20 }, onSave: () => undefined }));
    for (const part of ["单只占比不超过", "单个行业不超过", "年化波动不超过", "最大回撤不超过", "保存提醒线", "只存在本机浏览器"]) assert.ok(editor.includes(sq(part)), part);
    const card = flat(r.html(m.PortfolioAnalysis, {}));
    for (const part of ["收益表现", "开始计算", "点「开始计算」才会联网取价", "持仓体检", "开始体检", "提醒线"]) assert.ok(card.includes(sq(part)), part);
    assert.ok(!card.includes("正在取"), "没点之前不取数");
  });
});

test("历史回放:五个时期各自的组合与沪深 300 读数、逐只涨跌、缺口与说明;样本不够时只写原因", async () => {
  await withRenderer(async (r) => {
    const { ReplayView } = await r.load("/src/mobile/PortfolioAnalysis.tsx");
    const html = r.html(ReplayView, { data: replay });
    const text = flat(html);
    for (const part of ["2015 年 6–8 月大跌", "2016 年初熔断", "2018 年下跌", "2020 年 3 月全球疫情冲击", "2021–2024 年长期回撤", "组合涨跌", "最大回撤", "最差单日", "沪深 300", "覆盖 90% 市值", "样本白酒", "权重", "510300 取不到日线",
      "不是当时的真实持仓", "不预测下一次"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.equal((html.match(/<article/g) ?? []).length, 5);
    const thin = structuredClone(replay);
    thin.scenarios[0].portfolio = null;
    thin.scenarios[0].index = null;
    thin.scenarios[0].portfolio_reason = "这段时间有价格的只占现在持仓市值的 30%,不到一半,不给组合数字(样本不够就不下结论),只列各只的涨跌";
    const t2 = flat(r.html(ReplayView, { data: thin }));
    assert.ok(t2.includes(sq("不到一半,不给组合数字")));
    const bad = flat(r.html(ReplayView, { data: { ok: false, reason: "现在没有在手持仓", gaps: [] } }));
    assert.ok(bad.includes("算不了:现在没有在手持仓"));
  });
});
