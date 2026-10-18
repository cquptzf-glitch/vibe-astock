// Runs only in GitHub CI against the built app. All displayed prices are frozen test data.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
const require = createRequire(path.join(process.env.VIBE_UI_RUNTIME, "package.json"));
const { chromium } = require("playwright");
const out = process.env.VIBE_UI_OUTPUT || "dist/mobile-ui";
fs.mkdirSync(out, { recursive: true });
const version = process.env.APP_VERSION || "preview";
const fixture = JSON.parse(fs.readFileSync("core-rust/tests/fixtures/market_kline.json", "utf8"));
const klineRows = fixture.data.klines.map((row) => { const p = row.split(","); return [p[0], ...p.slice(1, 6).map(Number)]; });
const emotionFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-emotion-2026-10-16.json", "utf8"));
const historyFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-history-2026-10-16.json", "utf8"));
const chartFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-chart-600519.json", "utf8"));
const watchFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-watch-2026-10-17.json", "utf8"));
watchFixture.alerts = [
  { ts: "10:11:57", code: "002343", name: "慈文传媒", kind: "封板", change_pct: null, price: 9.33, pct: 10.02, msg: "触及涨停 9.33", sources: ["昨日5板"] },
  { ts: "10:05:12", code: "600519", name: "贵州茅台", kind: "急跌", change_pct: -1.62, price: 1308, pct: -1.6, msg: "3 分钟 -1.6%(现价 1308,今日 -1.6%)", sources: ["自选", "大票"] },
];
const samplesFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-samples-2026-10-16.json", "utf8"));
const journalRiskFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-journal-risk-synthetic.json", "utf8"));
const journalModesFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-journal-modes-synthetic.json", "utf8"));
const journalExcursionFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-journal-excursion-synthetic.json", "utf8"));
const journalEnvFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-journal-env-synthetic.json", "utf8"));
const journalAttributionFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-journal-attribution-synthetic.json", "utf8"));
const journalProblemsFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-journal-problems-synthetic.json", "utf8"));
const marketPanelFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-market-panel-2026-10-16.json", "utf8"));
const themeTreeFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-theme-tree-2026-10-16.json", "utf8"));
const tiersFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-review-tiers-2026-10-16.json", "utf8"));
const structureFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-structure-2026-10-16.json", "utf8"));
const prevBrokenFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-prev-broken-2026-10-16.json", "utf8"));
const portfolioFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-portfolio-synthetic.json", "utf8"));
const tradeImportFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-trade-import-synthetic.json", "utf8"));
const perfFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-portfolio-perf-synthetic.json", "utf8"));
const xrayFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-portfolio-xray-synthetic.json", "utf8"));
const replayFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-portfolio-replay-synthetic.json", "utf8"));
const messagesFixture = { unread: 2, messages: [
  { id: 3, kind: "watch_move", day: "2026-10-16", title: "2026-10-16 自选异动(2 只)", body: "样本光模块 300308 +10.00%(涨停) / 样本新股 688001 -10.00%(跌停) / 阈值:涨跌幅 ≥ 5% 或触及涨跌停;只在打开 App 时取一次行情,不是实时推送。", route: "monitor", read: false, created_at: "a", updated_at: "b" },
  { id: 2, kind: "close_summary", day: "2026-10-16", title: "2026-10-16 收盘摘要", body: "涨停 61 家 · 炸板 49 · 跌停 10 · 最高 11 板 · 炸板率 44.5% / 只列已经发生的读数,不构成买卖建议。", route: "emotion", read: false, created_at: "a", updated_at: "b" }] };
const intradayFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-intraday-2026-10-17.json", "utf8"));
const names = [["600519", "贵州茅台", 1329, 1.22], ["000001", "平安银行", 12.45, -.8], ["300308", "中际旭创", 154.8, 2.1], ["601318", "中国平安", 58.6, -.35], ["600036", "招商银行", 42.8, .61], ["920575", "北交所样本", null, null]];
const watchlist = names.map(([symbol, name]) => ({ symbol, name, created_at: "2026-10-16", updated_at: "2026-10-16" }));
const monitor = {
  status: "partial", source: "腾讯批量快照", fetched_at: "2026-10-16T07:00:00Z", trading_date: "2026-10-16", positions: [{ symbol: "600519" }], missing: ["920575"],
  watchlist: names.map(([symbol, name, price, change_pct], i) => ({ symbol, name, price, change_pct, amount_yuan: price == null ? null : 5075528306, limit_up: null, limit_down: null, quote_date: price == null ? null : "2026-10-16", is_today: price != null, available: price != null, pe_ttm: price == null ? null : [20.4, 5.14, 48.3, 9.2, -3.3][i] ?? 11, pb: price == null ? null : [6.61, 0.48, 9.1, 1.1, 0.9][i] ?? 1.5, turnover_pct: price == null ? null : 0.31 + i / 10, group: i < 2 ? "核心" : i < 4 ? "成长" : "", tags: i === 0 ? ["白酒", "龙头"] : [] })),
};
const overview = {
  status: "ok", source: "eastmoney", fetched_at: "2026-10-16T07:00:00Z", trading_date: "2026-10-16", stale: false,
  indexes: [["000001", "上证指数", 3900.21, .71], ["399106", "深证综指", 2450.18, .83], ["899050", "北证50", 1340.22, -1.1], ["399001", "深证成指", 13200.52, 1.12], ["399006", "创业板指", 2901.66, .92], ["000300", "沪深300", 4522.81, .63]].map(([code, name, price, change_pct]) => ({ code, name, price, change_pct, change: 10, amount_yuan: 10000000000 })),
  breadth: { exchange: { up: 3200, down: 1800, flat: 135, amount_yuan: 1860000000000 }, distribution: [["lt5", "跌幅 >5%", 135], ["lt3", "-5%~-3%", 242], ["lt0", "-3%~0", 1423], ["eq0", "平盘", 135], ["gt0", "0~3%", 2580], ["gt3", "3%~5%", 478], ["gt5", "涨幅 >5%", 142]].map(([key, label, count]) => ({ key, label, count })) },
  sentiment: { limit_up: 82, broken_board: 19, limit_down: 9, seal_rate: 81.2 }, errors: [],
};
const responses = {
  "/health": { ok: true, version: version + "-mobile" },
  "/mobile/research/workflows": {tasks:[]},
  "/mobile/research/calculations": { records: [{calculation_id:"calc-synthetic-audit",record_id:100,node_id:"growth",function:"growth_rate",executed:true,calc_version:"0.5.0",bindings:{current:{evidence:"E1",path:"/current"},base:{evidence:"E1",path:"/base"}},inputs:{current:12,base:10},inputs_refs:{current:{type:"evidence",id:"E1",path:"/current"},base:{type:"evidence",id:"E1",path:"/base"}},output:{status:"ok",value:0.2,unit:"小数",display:"20.00%",reason:""}}] },
  "/mobile/cache": {offline:false,cached_responses:2,scope:"公开数据缓存"},
  "/mobile/watchlist": { items: watchlist },
  "/mobile/market-overview": overview,
  "/mobile/monitor": monitor,
  "/mobile/data/summary": { source: "mobile_sqlite", storage: { total_bytes: 270336, database_bytes: 262144, wal_bytes: 8192, shm_bytes: 0 }, counts: { watchlist: 6, journal_entries: 28, journal_fills: 68, daily_reviews: 12, ai_messages: 18, open_positions: 3 }, ai: { configured: false, key_present: false, storage: "android_keystore", key_echoed: false } },
  "/mobile/research/profile": { symbol: "600519", market: "SH", name: "贵州茅台", industry: "白酒", ipo_date: "2001-08-27", total_shares: 1256000000, float_shares: 1256000000, total_market_cap: 1669224000000, float_market_cap: 1669224000000, source: "eastmoney" },
  "/mobile/research/financials": { rows: [], note: "测试样本", status: "partial" },
  "/mobile/research/announcements": { items: [], count: 0 },
  "/mobile/research/estimates": { reports: [], note: "测试样本", status: "partial" },
  "/mobile/stock/observations": { status: "ok", date: "2026-10-16", bars: 65, close: 1329, ma5: 1298.17, ma10: 1280.2, ma20: 1266, ma60: 1291, return5: 5.22, return20: 1.8, return60: 10.2, ma20_bias_pct: 4.97, ma_cross: "本根未发生 MA5/MA20 交叉", range_observation: "收盘高于此前20根最高价", prior20_high: 1317.76, prior20_low: 1228.1, volume_ratio_prev5: 1.1, same_day: false, stale: false, note: "冻结样本，仅用于界面检查", source: "eastmoney" },
  "/mobile/ai/config": { configured: true, model: "vision-sample", base_url: "https://example.invalid/v1", key_present: true, vision: true },
  "/mobile/ai/recognize": { status: "ok", target: "watch", model: "vision-sample", images: ["shot.png"], probe: false, warnings: [], note: "这是模型读图的结果,可能读错数字或名称。", drafts: [
    { code: "300750", name: "样本电池", shares: 100, cost: 250, uncertain: [], candidates: [], official_name: "样本电池" },
    { code: null, name: "样本银行", shares: null, cost: null, uncertain: ["没有读到代码,只有名称"], candidates: [{ symbol: "000001", name: "样本银行" }], official_name: null }] },
  "/mobile/ai/chat": { thread: 1, messages: [], count: 0 },
  "/mobile/ai/threads": { threads: [{ id: 1, title: "默认对话", updated_at: "a", count: 0 }, { id: 2, title: "估值讨论", updated_at: "b", count: 4 }], default_thread: 1 },
  "/mobile/shortline/emotion": emotionFixture.emotion,
  "/mobile/shortline/cycle": emotionFixture.cycle,
  "/mobile/shortline/history": historyFixture.history,
  "/mobile/shortline/weekly": historyFixture.weekly,
  "/mobile/shortline/verify": historyFixture.verify,
  "/mobile/stock/kline": chartFixture.kline,
  "/mobile/stock/intraday": chartFixture.intraday,
  "/mobile/stock/snapshot": chartFixture.snapshot,
  "/mobile/watch/poll": watchFixture,
  "/mobile/intraday": intradayFixture,
  "/mobile/review/market": marketPanelFixture,
  "/mobile/review/tiers": tiersFixture,
  "/mobile/messages": messagesFixture,
  "/mobile/messages/generate": { created: [], skipped: [], list: messagesFixture },
  "/mobile/portfolio/summary": portfolioFixture,
  "/mobile/portfolio/perf": perfFixture,
  "/mobile/portfolio/xray": xrayFixture,
  "/mobile/portfolio/replay": replayFixture,
  "/mobile/journal/import/preview": tradeImportFixture,
  "/mobile/review/structure": structureFixture,
  "/mobile/shortline/theme-tree": themeTreeFixture,
  "/mobile/shortline/prev-broken": prevBrokenFixture,
  "/mobile/journal/risk": journalRiskFixture,
  "/mobile/journal/modes": journalModesFixture,
  "/mobile/journal/problems": journalProblemsFixture,
  "/mobile/journal/attribution": journalAttributionFixture,
  "/mobile/journal/environment": journalEnvFixture,
  "/mobile/journal/inbox": journalExcursionFixture.inbox,
  "/mobile/journal/excursion": journalExcursionFixture.excursion,
  "/mobile/shortline/samples": samplesFixture,
};
const quote = { symbol: "600519", market: "SH", status: "ok", primary_source: "tencent", extra: { name: "贵州茅台", quote_date: "2026-10-16" }, evidence: [["price", 1329, "元"], ["change_pct", 1.22, "%"], ["pe_ttm", 19.81, "倍"], ["pb", 7.1, "倍"], ["turnover_pct", .31, "%"], ["total_market_cap", 16692.24, "亿元"]].map(([field, value, unit]) => ({ field, value, unit })) };
const browser = await chromium.launch({ headless: true, ...(process.env.VIBE_UI_CHANNEL ? { channel: process.env.VIBE_UI_CHANNEL } : {}) });
const errors = [];
try {
  for (const width of [360, 390, 768]) {
    for (const theme of ["light", "dark"]) {
      const context = await browser.newContext({ viewport: { width, height: 850 }, deviceScaleFactor: 1, colorScheme: theme });
      const page = await context.newPage();
      // 排查慢机器上的时序问题:VIBE_UI_THROTTLE=6 把 CPU 降速 6 倍
      if (process.env.VIBE_UI_THROTTLE) await (await context.newCDPSession(page)).send("Emulation.setCPUThrottlingRate", { rate: Number(process.env.VIBE_UI_THROTTLE) });
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.addInitScript(({ responses, quote, klineRows, theme }) => {
        if (!localStorage.getItem("vibe.ui.seeded")) {
          localStorage.setItem("vra-theme", theme);
          localStorage.setItem("vibe.mobile.density", "compact");
          localStorage.setItem("vibe.mobile.refresh", "0");
          localStorage.setItem("vibe.ui.seeded", "1");
        }
        // 自绘控件的事件轨迹:面板关不掉时,失败信息里带上点击与面板出现/消失的先后
        window.__trace = [];
        const mark = (what) => { if (window.__trace.length < 60) window.__trace.push(Math.round(performance.now()) + " " + what); };
        document.addEventListener("click", (event) => mark("click " + (event.target?.tagName ?? "?") + "." + (event.target?.className?.baseVal ?? event.target?.className ?? "") + " [" + String(event.target?.textContent ?? "").slice(0, 12) + "]"), true);
        new MutationObserver((records) => { for (const r of records) for (const n of [...r.addedNodes, ...r.removedNodes]) if (n.classList?.contains("wc-layer")) mark((r.addedNodes.length && [...r.addedNodes].includes(n) ? "layer+" : "layer-")); }).observe(document, { childList: true, subtree: true });
        window.__TAURI__ = { core: { invoke: async (command, args) => {
          if (command === "take_reminder_navigation") return null;
          if (command === "mobile_api") {
            const request = args.request;
            const route = request.path.split("?")[0];
            if(route==="/mobile/ai/workflow") {
              if(request.body.action==="advance") {
                window.__researchTask.results.push({stage:window.__researchTask.stages[window.__researchTask.results.length],paragraphs:[{text:"仅为合成的浏览器测试结论，存在资料缺口。",evidence:["E1"]}],checks:[{ok:true,level:"calculation",actual:0.2,display:"20.00%",claim:{label:"合成数值"}}],calculations:responses["/mobile/research/calculations"].records,conditions:[],verified:true,model:"fixture",at:"2026-10-18"});
                if(window.__researchTask.results.length===2)window.__researchTask.status="complete";
                window.__researchTask.archive_id=99;
              }else window.__researchTask={id:88,title:"合成流程测试",mode:"deep",date:"2026-10-18",status:"active",stages:["合成阶段甲","合成阶段乙"],results:[],revision:0,gaps:[{source:"合成源",reason:"未取得完整财务"}]};
              return {status:200,body:structuredClone(window.__researchTask)};
            }
            if(route==="/mobile/research/workflows")return {status:200,body:{tasks:window.__researchTask?[structuredClone(window.__researchTask)]:[]}};
            if(route==="/mobile/research/calculations")return {status:200,body:new URL(request.path,"https://mobile.invalid").searchParams.has("calculation_id")?responses[route].records[0]:responses[route]};
            if (route === "/fetch") return { status: 200, body: { envelope: request.body.endpoint === "kline" ? { ...quote, extra: { rows: klineRows } } : quote } };
            return { status: responses[route] ? 200 : 501, body: responses[route] || { message: "未提供界面测试样本" } };
          }
          throw new Error("Unexpected command: " + command);
        } }, notification: { isPermissionGranted: async () => false, requestPermission: async () => "granted", sendNotification() {} } };
      }, { responses, quote, klineRows, theme });
      await page.goto("http://127.0.0.1:4173", { waitUntil: "networkidle" });
      await page.getByRole("button", { name: "贵州茅台", exact: false }).first().waitFor();
      await page.getByText("沪深北成交", { exact: false }).waitFor();
      const compactHeight = await page.locator(".mo-wrap").evaluate((el) => el.getBoundingClientRect().height);
      // 点完选项后面板必须收起;收不起来时把事件轨迹写进错误
      const layerGone = async (what) => {
        try { await page.locator(".wc-layer").waitFor({ state: "detached", timeout: 8000 }); }
        catch (error) { throw new Error(what + ": 面板没有收起。事件轨迹:" + JSON.stringify(await page.evaluate(() => window.__trace)) + " | " + String(error.message).split("\n")[0]); }
      };
      const assertWidth = async () => {
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
        assert.equal(overflow, false, width + " " + theme + " horizontal overflow");
      };
      await assertWidth();
      const navTops = await page.locator(".mobile-nav button").evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().top));
      assert.ok(navTops.length === 6 && Math.max(...navTops) - Math.min(...navTops) < 2, "bottom navigation must keep five groups and settings in one row");
      await page.getByRole("region", { name: "情绪速览" }).getByText("炸板率", { exact: true }).waitFor();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-home.png"), fullPage: true });
      // 消息(我的):启动时生成的消息列表 + 导航角标
      await page.getByLabel("2 条未读消息").waitFor();
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: /^我的/ }).click();
      await page.locator(".directory-grid button", { hasText: "盘前提醒、收盘摘要、自选异动" }).click();
      await page.getByRole("list", { name: "消息列表" }).waitFor();
      await page.getByText("收盘摘要", { exact: true }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-messages.png"), fullPage: true });
      // AI 对话:对话栏(多会话)与停止按钮所在的发送区
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: /^研究/ }).click();
      await page.locator(".group-tabs").getByRole("tab", { name: "AI 助手", exact: true }).click();
      await page.getByLabel("对话列表").waitFor();
      await page.getByRole("button", { name: "新对话", exact: true }).waitFor();
      await assertWidth();
      await page.getByLabel("允许公开数据与计算工具",{exact:false}).check();
      await page.getByLabel("检索本机资料库",{exact:false}).check();
      await page.getByRole("tab",{name:"研究 / 辩论 / 复盘",exact:true}).click();
      await page.getByLabel("研究股票代码",{exact:true}).fill("600519");
      await page.getByLabel("研究问题",{exact:true}).fill("仅测试合成流程");
      await page.getByRole("button",{name:"创建新研究",exact:true}).click();
      await page.getByRole("heading",{name:"合成流程测试",exact:true}).waitFor();
      await page.getByRole("button",{name:"执行下一阶段",exact:true}).click();
      await page.getByText("已完成 1/2 阶段",{exact:false}).waitFor();
      await assertWidth();
      await page.screenshot({path:path.join(out,width+"-"+theme+"-research-partial.png"),fullPage:true});
      await page.getByRole("button",{name:"连续完成剩余阶段",exact:true}).click();
      await page.getByText("全部阶段已完成",{exact:false}).waitFor();
      await page.getByText("仅为合成的浏览器测试结论",{exact:false}).first().waitFor();
      await assertWidth();
      await page.screenshot({path:path.join(out,width+"-"+theme+"-research-complete.png"),fullPage:true});
      await page.getByRole("button",{name:"查看计算依赖与输入审计",exact:true}).click();
      await page.getByRole("button",{name:/^growth_rate ·/}).click();
      await page.getByRole("article",{name:"计算输入与依赖审计"}).waitFor();
      await page.getByText("实际参与计算的输入",{exact:true}).click();
      await page.getByText("证据和前序计算的来源路径",{exact:true}).click();
      await assertWidth();
      await page.screenshot({path:path.join(out,width+"-"+theme+"-calculation-audit.png"),fullPage:true});
      // 研报文件导入:选文本文件 → 预览 → 取消
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: /^我的/ }).click();
      await page.locator(".directory-grid button", { hasText: "保存、查阅与检索报告正文" }).click();
      await page.getByRole("button", { name: "从文件导入研报", exact: false }).click();
      await page.locator('.report-import input[type=file]').setInputFiles({ name: "样本报告.md", mimeType: "text/markdown", buffer: Buffer.from("# 样本报告 这是合成的测试正文。", "utf8") });
      await page.getByLabel("研报导入预览").waitFor();
      assert.equal(await page.getByLabel("研报标题", { exact: true }).inputValue(), "样本报告");
      await assertWidth();
      await page.getByRole("button", { name: "取消", exact: true }).click();
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: /^首页/ }).click();
      await page.locator(".mo-wrap").waitFor();
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "设置", exact: true }).click();
      await page.getByRole("button", { name: "白色模式", exact: true }).waitFor();
      assert.equal((await page.locator(".appearance-refresh .wc-select").innerText()).trim(), "仅手动刷新");
      assert.equal(await page.locator("html").getAttribute("data-theme"), theme);
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-settings.png"), fullPage: true });
      // 备份口令(可选加密)与统一清除:点清除只弹出自绘确认框,取消后什么都不发生
      await page.getByLabel("备份口令", { exact: true }).waitFor();
      await page.getByRole("button", { name: "清除全部本地数据", exact: true }).click();
      await page.getByRole("alertdialog").waitFor();
      assert.match(await page.getByRole("alertdialog").innerText(), /Android Keystore 里的 API Key/);
      await page.getByRole("button", { name: "取消", exact: true }).click();
      await page.getByRole("alertdialog").waitFor({ state: "detached" });
      await page.locator(".appearance-refresh .wc-select").click();
      await page.getByRole("option", { name: "每 30 秒", exact: true }).click();
      await layerGone("选完下拉选项");
      await page.waitForFunction(() => document.querySelector(".appearance-refresh .wc-select")?.textContent?.trim() === "每 30 秒");
      await page.locator(".appearance-refresh .wc-select").click();
      await page.getByRole("option", { name: "仅手动刷新", exact: true }).click();
      await layerGone("选完下拉选项");
      await page.getByRole("button", { name: "模型 API 设置", exact: true }).click();
      await page.getByPlaceholder("https://your-provider.example/v1").waitFor();
      await page.getByRole("button", { name: "模型 API 设置", exact: true }).click();
      const other = theme === "dark" ? "light" : "dark";
      await page.getByRole("button", { name: other === "light" ? "白色模式" : "深色模式", exact: true }).click();
      assert.equal(await page.locator("html").getAttribute("data-theme"), other);
      await page.reload({ waitUntil: "networkidle" });
      assert.equal(await page.locator("html").getAttribute("data-theme"), other);
      await page.getByRole("button", { name: "跟随系统", exact: true }).click();
      assert.equal(await page.locator("html").getAttribute("data-theme"), theme);
      await page.emulateMedia({ colorScheme: other });
      await page.waitForFunction((mode) => document.documentElement.dataset.theme === mode, other);
      await page.emulateMedia({ colorScheme: theme });
      await page.waitForFunction((mode) => document.documentElement.dataset.theme === mode, theme);
      await page.getByRole("button", { name: "舒适", exact: false }).click();
      assert.equal(await page.locator("html").getAttribute("data-mobile-density"), "comfortable");
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "首页", exact: true }).click();
      await page.locator(".mo-wrap").waitFor();
      await page.getByText("沪深北成交", { exact: false }).waitFor();
      const comfortableHeight = await page.locator(".mo-wrap").evaluate((el) => el.getBoundingClientRect().height);
      assert.ok(comfortableHeight > compactHeight, "comfortable layout must show a real spacing difference");
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "设置", exact: true }).click();
      await page.getByRole("button", { name: "紧凑", exact: false }).click();
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "首页", exact: true }).click();
      await page.getByRole("button", { name: "贵州茅台", exact: false }).first().click();
      await page.getByText("量价与资金观察", { exact: true }).waitFor();
      for (const tabs of await page.locator(".mobile-stock-tabs").all()) {
        const tops = await tabs.locator("button").evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().top));
        assert.ok(Math.max(...tops) - Math.min(...tops) < 2, "stock tabs must remain in one row");
      }
      assert.equal(await page.locator(".stock-detail-grid").evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(" ").length), 3);
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-stock.png"), fullPage: true });
      // K 线:点按出十字线、拖动平移后可回到最新、切换到分时
      await page.getByText("展开五档盘口", { exact: true }).waitFor();
      const chart = page.locator(".kc-svg").first();
      await chart.scrollIntoViewIfNeeded();
      const box = await chart.boundingBox();
      assert.ok(box && box.width <= width, "chart fits the viewport");
      await page.mouse.click(box.x + box.width * 0.5, box.y + 60);
      await page.locator(".kc-cross").waitFor();
      await page.mouse.move(box.x + box.width * 0.4, box.y + 80);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width * 0.8, box.y + 80, { steps: 8 });
      await page.mouse.up();
      assert.equal(await page.getByRole("button", { name: "回到最新" }).isEnabled(), true, "dragging pans back in time");
      await page.getByRole("button", { name: "MACD", exact: true }).click();
      await page.getByText("MACD(12,26,9) DIF", { exact: false }).waitFor();
      await assertWidth();
      await page.locator(".kc-card").screenshot({ path: path.join(out, width + "-" + theme + "-chart.png") });
      await page.getByRole("tab", { name: "分时", exact: true }).click();
      await page.getByRole("img", { name: "分时图" }).waitFor();
      await page.getByRole("tab", { name: "日K", exact: true }).click();
      // 盯盘栏目:实时动态的异动流与名单页签,名单页签单行横向滚动
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "盯盘", exact: true }).click();
      await page.getByRole("heading", { name: "实时动态", exact: true }).waitFor();
      await page.getByText("触及涨停 9.33", { exact: true }).waitFor();
      await page.getByRole("tab", { name: "昨日梯队", exact: false }).click();
      await page.getByText("慈文传媒", { exact: true }).first().waitFor();
      const watchTabTops = await page.locator(".watch-tabs [role=tab]").evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().top));
      assert.ok(watchTabTops.length === 5 && Math.max(...watchTabTops) - Math.min(...watchTabTops) < 2, "watch list tabs must stay in one row");
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-watch.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "我的自选", exact: true }).click();
      await page.getByRole("region", { name: "我的自选" }).waitFor();
      await page.getByRole("tab", { name: "核心 2", exact: true }).click();
      await page.getByText("PE 20.4", { exact: false }).first().waitFor();
      assert.equal(await page.locator(".wl-row").count(), 2, "按分组筛选后只剩核心里的 2 只");
      await page.getByRole("button", { name: "编辑 贵州茅台", exact: true }).click();
      await page.getByLabel("标签", { exact: true }).waitFor();
      assert.equal(await page.getByLabel("标签", { exact: true }).inputValue(), "白酒 龙头");
      await page.getByRole("button", { name: "取消", exact: true }).click();
      await page.locator(".wl-toolbar .wc-select").click();
      await page.getByRole("option", { name: "按市盈率", exact: true }).click();
      await layerGone("选完下拉选项");
      await page.locator(".wl-dir").click();
      await page.waitForFunction(() => document.querySelector(".wl-row .wl-name strong")?.textContent === "平安银行");
      await page.getByRole("tab", { name: /^全部/ }).click();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-watchlist.png"), fullPage: true });
      // 截图识别(AI):选图 → 识别 → 草稿逐条核对 → 写入
      await page.getByRole("button", { name: "从截图识别(自选)", exact: false }).click();
      await page.locator('.screenshot-import input[type=file]').setInputFiles({ name: "shot.png", mimeType: "image/png", buffer: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64") });
      await page.getByRole("button", { name: "识别", exact: true }).click();
      await page.getByRole("list", { name: "识别草稿" }).waitFor();
      await page.getByLabel("草稿 1 已核对", { exact: true }).check();
      await page.getByRole("button", { name: "写入 1 条到自选", exact: false }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-screenshot-import.png"), fullPage: true });
      await page.getByRole("button", { name: "写入 1 条到自选", exact: false }).click();
      await page.getByText("已写入 1 条", { exact: false }).waitFor();
      await page.locator(".group-tabs").getByRole("tab", { name: "我的持仓", exact: true }).click();
      await page.getByRole("region", { name: "已实现盈亏与现金分红" }).waitFor();
      await page.getByText("已实现盈亏", { exact: true }).first().waitFor();
      await page.getByRole("button", { name: "记一笔", exact: false }).click();
      await page.getByLabel("分红日期", { exact: false }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-portfolio-income.png"), fullPage: true });
      // 隐私模式:开 → 金额和股数遮成 ••••;关 → 恢复
      await page.getByRole("button", { name: /^开启隐私模式/ }).click();
      await page.waitForFunction(() => document.querySelector(".portfolio-income")?.textContent?.includes("••••"));
      assert.ok(!(await page.locator(".portfolio-income").innerText()).includes("1,560.5"), "已实现金额被遮住");
      await page.getByRole("button", { name: "关闭隐私模式", exact: true }).click();
      await page.waitForFunction(() => !document.querySelector(".portfolio-income")?.textContent?.includes("••••"));
      await page.getByRole("button", { name: "开始计算", exact: true }).click();
      await page.getByRole("img", { name: "时间加权收益走势" }).waitFor();
      await page.getByRole("button", { name: "开始体检", exact: true }).click();
      await page.getByRole("table", { name: "持仓权重与行业" }).waitFor();
      await page.getByRole("button", { name: "开始回放", exact: true }).click();
      await page.getByRole("article", { name: "2020 年 3 月全球疫情冲击" }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-portfolio-analysis.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "实时动态", exact: true }).click();
      await page.getByRole("heading", { name: "实时动态", exact: true }).waitFor();
      await page.locator(".group-tabs").getByRole("tab", { name: "竞价与盘中", exact: true }).click();
      await page.getByRole("heading", { name: "竞价与盘中", exact: true }).waitFor();
      await page.getByText("周末休市,没有竞价", { exact: true }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-intraday.png"), fullPage: true });
      // 复盘栏目:底部一级导航 → 今日情绪,栏目内子页签单行横向滚动 → 连板天梯。
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "复盘", exact: true }).click();
      await page.getByRole("heading", { name: "今日情绪", exact: true }).waitFor();
      await page.getByText("连续两日走强", { exact: true }).waitFor();
      // 图片分享卡:画出预览(不触发原生分享)
      await page.getByRole("button", { name: /^生成 .* 的分享图$/ }).click();
      await page.getByRole("img", { name: /情绪复盘分享图预览/ }).waitFor();
      assert.ok(await page.getByRole("img", { name: /情绪复盘分享图预览/ }).evaluate((img) => img.naturalWidth === 720), "分享图宽 720");
      const tabTops = await page.locator(".group-tabs [role=tab]").evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().top));
      assert.ok(tabTops.length >= 9 && Math.max(...tabTops) - Math.min(...tabTops) < 2, "review sub-tabs must stay in one scrollable row");
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-emotion.png"), fullPage: true });
      // 问 AI:点页面上的按钮 → 跳到 AI 对话,预填问题并勾选「附带当前页面」(只是勾选,不发送)
      await page.getByRole("button", { name: /问 AI:解读今天的情绪读数/ }).click();
      await page.getByLabel("附带当前页面").waitFor();
      assert.equal(await page.getByLabel("附带当前页面").isChecked(), true, "从页面问 AI 时默认附带该页");
      assert.match(await page.locator(".ai-chat-composer textarea").inputValue(), /赚钱效应/);
      await assertWidth();
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "复盘", exact: true }).click();
      await page.getByRole("heading", { name: "今日情绪", exact: true }).waitFor();
      await page.locator(".group-tabs").getByRole("tab", { name: "连板天梯", exact: true }).click();
      await page.getByText("新华传媒", { exact: true }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-ladder.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "情绪历史", exact: true }).click();
      await page.getByText("历史统计位置", { exact: true }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-history.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "市场面板", exact: true }).click();
      await page.getByRole("heading", { name: "市场面板", exact: true }).waitFor();
      await page.getByText("主要指数", { exact: true }).first().waitFor();
      await page.getByRole("table", { name: "主要指数中长线" }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-market-panel.png"), fullPage: true });
      await page.getByRole("region", { name: "涨停分档卡片" }).waitFor();
      await page.locator(".group-tabs").getByRole("tab", { name: "结构变化", exact: true }).click();
      await page.getByRole("heading", { name: "结构变化", exact: true }).waitFor();
      await page.getByText("结构移位不等于制度变化", { exact: false }).first().waitFor();
      await page.getByRole("button", { name: "登记", exact: false }).click();
      await page.getByLabel("施行日期", { exact: false }).waitFor();
      await page.getByLabel("施行日期", { exact: false }).click();
      await page.getByRole("dialog", { name: "施行日期" }).waitFor();
      await page.getByRole("grid").waitFor();
      await assertWidth();
      await page.getByRole("button", { name: "今天", exact: true }).click();
      await page.waitForFunction(() => /^\s*\d{4}-\d{2}-\d{2}\s*$/.test(document.querySelector('[aria-label^="施行日期"]')?.textContent ?? ""));
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-structure.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "行业题材", exact: true }).click();
      await page.getByRole("region", { name: "题材事件树卡片" }).waitFor();
      await page.getByText("题材归因来自源站编辑", { exact: false }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-theme-tree.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "盘面事实", exact: true }).click();
      await page.getByRole("region", { name: "昨日炸板股修复卡片" }).waitFor();
      await page.getByRole("table", { name: "昨日炸板反馈矩阵" }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-prev-broken.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "涨停样本", exact: true }).click();
      await page.getByText("首板 · 最后封板时间曲线", { exact: false }).waitFor();
      await page.getByText("事后名单", { exact: false }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-samples.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "交易日志", exact: true }).click();
      await page.getByRole("button", { name: "从表格导入成交与分红", exact: false }).click();
      await page.getByRole("button", { name: "填入示例", exact: true }).click();
      await page.getByRole("button", { name: "预览", exact: true }).click();
      await page.getByRole("table", { name: "逐行预览" }).waitFor();
      await page.getByText("手机端暂不支持「送转」", { exact: false }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-trade-import.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "风险与纪律", exact: true }).click();
      await page.getByRole("heading", { name: "风险与纪律", exact: true }).waitFor();
      await page.getByText("权益曲线与回撤", { exact: true }).waitFor();
      await page.getByRole("img", { name: "权益曲线", exact: false }).waitFor();
      await page.getByRole("button", { name: "修改", exact: true }).click();
      await page.getByLabel("账户规模", { exact: true }).waitFor();
      assert.equal(await page.getByLabel("单笔最大亏损", { exact: true }).inputValue(), "4");
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-risk.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "模式卡", exact: true }).click();
      await page.getByRole("heading", { name: "模式卡", exact: true }).waitFor();
      await page.getByText("收紧封板时间", { exact: false }).first().waitFor();
      await page.getByRole("button", { name: "新建模式卡", exact: false }).click();
      await page.getByLabel("模式名", { exact: true }).waitFor();
      assert.equal(await page.getByLabel("什么情况下做", { exact: true }).inputValue(), "", "new cards are never prefilled");
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-modes.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "复查", exact: true }).click();
      await page.getByRole("heading", { name: "复查", exact: true }).waitFor();
      await page.getByRole("list", { name: "复查清单" }).waitFor();
      await page.getByRole("table", { name: "逐笔最大浮盈浮亏" }).waitFor();
      await page.getByText("日线看不出最高点发生在盘中买入之前还是之后", { exact: false }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-excursion.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "入场环境", exact: true }).click();
      await page.getByRole("heading", { name: "入场环境", exact: true }).waitFor();
      await page.getByRole("table", { name: "按当时板位" }).waitFor();
      await page.getByRole("table", { name: "逐笔入场环境" }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-env.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "看错还是没执行好", exact: true }).click();
      await page.getByRole("heading", { name: "看错了,还是没执行好", exact: true, level: 1 }).waitFor();
      await page.getByRole("group", { name: "四格" }).waitFor();
      await page.getByText("判错 + 赚钱 · 运气", { exact: false }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-attribution.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "问题记录", exact: true }).click();
      await page.getByRole("heading", { name: "问题记录", exact: true, level: 1 }).waitFor();
      await page.getByText("卖在买之前", { exact: false }).first().waitFor();
      await page.getByRole("button", { name: "改 卖出 2026-10-13", exact: true }).first().click();
      await page.getByLabel("成交价", { exact: true }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-problems.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "明日验证", exact: true }).click();
      await page.getByText("写给下一场的条件", { exact: true }).waitFor();
      await page.getByRole("button", { name: "加一条", exact: false }).click();
      await page.getByLabel("第 1 条读数").waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-verify.png"), fullPage: true });
      await context.close();
    }
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(path.join(out, "verification.json"), JSON.stringify({ fixture_data: true, widths: [360, 390, 768], themes: ["light", "dark"], screenshots: 180, page_errors: errors }, null, 2));
  console.log("Mobile UI smoke passed: 6 viewport/theme combinations, theme persistence/system switch, density, navigation and review pages, stock chart interactions, live watch, intraday, samples, journal risk, journal modes, journal review (inbox and excursion), journal environment, journal attribution, journal problems, market panel, structure, theme tree, previous broken boards, research calculation audit; 180 screenshots.");
} catch (error) {
  // 注解(::error::)匿名可读;日志要登录才能看
  const text = String(error && error.stack ? error.stack : error).split("\n").slice(0, 12).join("%0A").slice(0, 3500);
  console.log(`::error title=Mobile UI smoke failed::${text}`);
  throw error;
} finally {
  await browser.close();
}
