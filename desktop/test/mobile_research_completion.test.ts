import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

test("新增周报与产业温度计渲染实际数字、口径和缺失",async()=>{
  await withRenderer(async r=>{
    const {JournalWeeklyView}=await r.load("/src/mobile/JournalWeekly.tsx");const data={start:"2026-10-12",end:"2026-10-16",buys:2,sells:1,openings:1,splits:1,buy_amount:12345,sell_amount:54321,closed:1,planned:1,unplanned:0,performance:{net_pnl:200,win_rate:1},note:"清仓后结算"};
    const text=textOf(r.html(JournalWeeklyView,{data}));assert.match(text,/本周我的交易/);assert.match(text,/12,345/);assert.match(text,/期初 1 笔 · 送转 1 笔/);
    const hidden=textOf(r.html(JournalWeeklyView,{data,hide:true}));assert.doesNotMatch(hidden,/12,345|54,321/);assert.match(hidden,/••••/);
    const {IndustryRevenueView}=await r.load("/src/mobile/IndustryRevenue.tsx");const revenue=textOf(r.html(IndustryRevenueView,{data:{companies:[{ticker:"2383",name:"合成台股",period:"2026-06",revenue_twd:1e8,mom_pct:10,yoy_pct:null,cumulative_yoy_pct:null,stale:true,lag_months:4}],errors:[{ticker:"2368",message:"源站限流"}],ccl_pcb_mom_difference_pp:null,note:"金额为新台币",source:"合成源"}}));assert.match(revenue,/已滞后 4 个月/);assert.match(revenue,/累计同比 —/);assert.match(revenue,/同期数据不足/);assert.match(revenue,/源站限流/);
  });
});

test("公开页面助手与分享入口不出现在交易和持仓页",async()=>{
  await withRenderer(async r=>{const {PublicPageActions}=await r.load("/src/mobile/PublicPageActions.tsx");
    assert.match(textOf(r.html(PublicPageActions,{page:"stock",onAsk:()=>{}})),/问 AI 分享图/);
    for(const page of ["hold","records","daily","monitor","settings"])assert.equal(textOf(r.html(PublicPageActions,{page,onAsk:()=>{}})),"");
  });
});

test("研究结果实际渲染阶段、引用、数字核对、缺口和验证入口", async () => {
  await withRenderer(async (r) => {
    const { ResearchTaskView, RESEARCH_MODES } = await r.load("/src/mobile/ResearchWorkflowPanel.tsx");
    assert.equal(RESEARCH_MODES.length, 6);
    const task = { title: "合成研究", status: "active", stages: ["画像", "财务"], results: [{ stage: "画像", paragraphs: [{ text: "收入存在变化，需核对业务范围。", evidence: ["E1"] }], checks: [{ actual: 20, level: "formula", ok: true, claim: { label: "增长" } }], conditions: [{ metric: "broken_rate", direction: "up", reason: "比较炸板率" }], model: "mock", at: "2026-10-18" }], archive_id: 8, gaps: [{ source: "财务", reason: "没有取到" }], error: { message: "引用核对未通过" } };
    const html = r.html(ResearchTaskView, { task, onConditions: () => {} });
    const text = textOf(html);
    for (const expected of ["已完成 1/2 阶段", "画像 ✓", "E1", "增长：20", "算式核对", "没有取到", "引用核对未通过", "将本阶段条件加入明日验证"]) assert.ok(text.includes(expected), expected);
    const stopped = textOf(r.html(ResearchTaskView, { task: { ...task, status: "cancelled", results: [], error: undefined } }));
    assert.match(stopped, /已中止并归档已完成部分/);
    assert.doesNotMatch(stopped, /将本阶段条件加入/);
  });
});

test("财务三表缺项与同比、一致预期薄样本、估值非正值不伪造分位", async () => {
  await withRenderer(async (r) => {
    const { StatementsView, ConsensusView, ValuationView } = await r.load("/src/mobile/ResearchExtendedPanel.tsx");
    const statements = textOf(r.html(StatementsView, { data: { statements: { income: [{ period: "2026-06-30", items: [{ label: "收入", value: 1200, raw: "1200", unit: "元" }, { label: "净利", value: null, raw: "--", unit: null }], yoy_pct: { 收入: 20 } }] }, errors: [{ statement: "balance", message: "取数失败" }], note: "同比按同报告期" } }));
    assert.match(statements, /收入 1,200 元 \+20%/); assert.match(statements, /净利 -- —/); assert.match(statements, /资产负债表 本表未取到/);
    const consensus = textOf(r.html(ConsensusView, { data: { rows: [{ year: 2026, institutions: 2, mean: 1.2, min: 1, max: 2, thin_sample: true }], missing_years: [2027], note: "源站共识" } }));
    assert.match(consensus, /机构不足三家/); assert.match(consensus, /缺失年度：2027/);
    const valuation = textOf(r.html(ValuationView, { data: { series: { pe_ttm: { rows: [["2026-01-02", -1], ["2026-01-05", -2]], latest: -2, date: "2026-01-05", percentile_pct: null, positive_samples: 0, min: null, max: null } }, errors: [], note: "样本范围" } }));
    assert.match(valuation, /当前估值非正，未计算/); assert.doesNotMatch(valuation, /历史分位 0%/);
  });
});

test("月历真实显示开休市；扫描缺字段不冒充满足条件", async () => {
  await withRenderer(async (r) => {
    const { ExchangeMonthView } = await r.load("/src/mobile/ExchangeCalendar.tsx");
    const calendar = textOf(r.html(ExchangeMonthView, { data: { month: "2026-10", days: [{ date: "2026-10-01", is_open: false }, { date: "2026-10-09", is_open: true }], source: "官方月历", note: "源站定义" } }));
    assert.match(calendar, /1 休市 9 开市/);
    const { ScanResults } = await r.load("/src/mobile/MarketScannerPanel.tsx");
    const scan = textOf(r.html(ScanResults, { data: { total: 3, matched: 1, missing: 2, rows: [{ code: "600001", name: "合成股票", price: null, pct: 2, surge_pct: null }], gaps: [{ reason: "部分页未取到" }], note: "非逐笔", sampled_at: "" }, onOpen: () => {} }));
    assert.match(scan, /已扫描 3 只 · 满足 1 只 · 缺字段 2 只/); assert.match(scan, /今日 \+2% · 采样变化 —/);
  });
});

test("高级回测视图展示区间与滚动样本外，失败基准不画零", async () => {
  await withRenderer(async (r) => {
    const { AdvancedResults } = await r.load("/src/mobile/BacktestPanel.tsx");
    const text = textOf(r.html(AdvancedResults, { result: { sharpe_interval: { available: true, low: -0.2, high: 1.2, valid_samples: 990, positive_share: 0.8 }, benchmark: { available: false, reason: "指数首日缺失" }, walk_forward: { available: true, total_return_pct: 3, folds: [{ train_start: "2024-01-02", train_end: "2024-12-31", test_start: "2025-01-02", test_end: "2025-03-31", selected: "MA10/30", is_return_pct: 8, oos_return_pct: -1 }] } } }));
    assert.match(text, /-0.20 至 1.20/); assert.match(text, /指数首日缺失/); assert.match(text, /各段样本外串联收益 \+3.00%/); assert.match(text, /-1.00%/);
  });
});


test("台股月营收历史保留跨年、缺月和同比基数", async () => {
  await withRenderer(async (r) => {
    const {IndustryRevenueView, revenueHistory} = await r.load("/src/mobile/IndustryRevenue.tsx");
    const series = [["2025-01", 100000000], ["2025-12", 120000000], ["2026-01", 150000000], ["2026-03", 180000000]];
    const history = revenueHistory(series);
    assert.deepEqual(history.slice(0, 3).map((row: {period:string}) => row.period), ["2026-03", "2026-02", "2026-01"]);
    assert.equal(history[0].mom, null);
    assert.equal(history[1].amount, null);
    assert.equal(history[2].mom, 25);
    assert.equal(history[2].yoy, 50);
    const html = r.html(IndustryRevenueView, {data: {companies: [{ticker:"2383",name:"合成公司",period:"2026-03",revenue_twd:180000000,mom_pct:null,yoy_pct:null,cumulative_yoy_pct:null,stale:false,lag_months:0,series}],errors:[],ccl_pcb_mom_difference_pp:null,note:"合成样本",source:"合成来源"}});
    assert.match(html, /<summary>历史月营收/);
    assert.match(html, /aria-label="合成公司历史月营收"/);
    assert.match(textOf(html), /2026-02 — — —/);
    assert.match(textOf(html), /2026-01 1.5 \+25% \+50%/);
  });
});

test("月营收走势图保留缺月断点、原始金额和公开图片分享入口", async () => {
  await withRenderer(async r => {
    const {RevenueTrend,revenueHistory} = await r.load("/src/mobile/IndustryRevenue.tsx");
    const company = {ticker:"2383",name:"合成公司",period:"2026-03",revenue_twd:180000000,mom_pct:null,yoy_pct:null,cumulative_yoy_pct:null,stale:true,lag_months:4,series:[["2025-01",100000000],["2025-12",120000000],["2026-01",150000000],["2026-03",180000000]]};
    const html = r.html(RevenueTrend,{company,source:"合成来源",note:"不能推出 A 股公司业绩"});
    const text = textOf(html);
    assert.match(html,/role="img" aria-label="合成公司月营收走势图，单位亿新台币"/);
    assert.equal((html.match(/class="revenue-line"/g)??[]).length,3,"三个连续片段，不能跨越缺月连线");
    assert.equal((html.match(/class="revenue-point"/g)??[]).length,4,"只绘制实际四个月的数值");
    for(const value of ["2025-01 至 2026-03", "缺失 11 个月", "2026-03 1.8 亿新台币", "近 12 月", "近 24 月", "全部", "生成当前图表分享图"]) assert.ok(text.includes(value),value);
    const empty = r.html(RevenueTrend,{company:{...company,series:[["2026-03",180000000]]},source:"合成来源",note:""});
    assert.match(textOf(empty),/不足两个月/);assert.doesNotMatch(empty,/<svg|生成当前图表分享图/);
    const sparse = r.html(RevenueTrend,{company:{...company,series:[["2024-01",100000000],["2026-03",180000000]]},source:"合成来源",note:""});
    assert.match(textOf(sparse),/请切换至更长区间/);
    assert.match(textOf(sparse),/近 12 月 近 24 月 全部/);
    assert.doesNotMatch(sparse,/<svg/);
    assert.deepEqual(revenueHistory([["1900-01",1],["2200-01",2],["2026-03",180000000]]).map((x:{period:string})=>x.period),["2026-03"]);
  });
});


test("日历日期窗口使用北京时间跨日和跨年", async () => {
  await withRenderer(async r => {
    const {calendarDate} = await r.load("/src/mobile/CalendarPanel.tsx");
    const now = new Date("2026-12-31T16:30:00Z");
    assert.equal(calendarDate(0, now), "2027-01-01");
    assert.equal(calendarDate(-7, now), "2026-12-25");
    assert.equal(calendarDate(30, now), "2027-01-31");
  });
});


test("日历同步逐批覆盖全部自选并保留来源缺口", async () => {
  await withRenderer(async r => {
    const {syncCalendarBatches} = await r.load("/src/mobile/CalendarPanel.tsx");
    const calls: Record<string, unknown>[] = []; const updates: string[] = [];
    const request = async (body: Record<string, unknown>) => {
      calls.push(body); const offset = Number(body.offset); const processed = Math.min(20, 43 - offset);
      return {count:processed, gaps:offset === 20 ? [{source:"测试源",message:"限流"}] : [], note:"", processed_symbols:processed,total_symbols:43,next_offset:offset+processed<43?offset+processed:null,watchlist_revision:"same"};
    };
    const result = await syncCalendarBatches("", request, (s: string) => updates.push(s));
    assert.deepEqual(calls.map(c => c.offset), [0, 20, 40]);
    assert.equal(calls[1]!.watchlist_revision, "same");
    assert.equal(result.count, 43); assert.equal(result.gaps[0].message, "限流");
    assert.match(updates.at(-1)!, /43 \/ 43/);
    assert.match(updates.at(-1)!, /测试源：限流/);
    await assert.rejects(() => syncCalendarBatches("", async (body: Record<string, unknown>) => ({count:0,gaps:[],note:"",processed_symbols:20,total_symbols:43,next_offset:20,watchlist_revision:body.offset ? "changed" : "original"}), () => {}), /变化/);
  });
});
