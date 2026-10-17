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
const intradayFixture = JSON.parse(fs.readFileSync("desktop/test/fixtures/mobile-intraday-2026-10-17.json", "utf8"));
const names = [["600519", "贵州茅台", 1329, 1.22], ["000001", "平安银行", 12.45, -.8], ["300308", "中际旭创", 154.8, 2.1], ["601318", "中国平安", 58.6, -.35], ["600036", "招商银行", 42.8, .61], ["920575", "北交所样本", null, null]];
const watchlist = names.map(([symbol, name]) => ({ symbol, name, created_at: "2026-10-16", updated_at: "2026-10-16" }));
const monitor = {
  status: "partial", source: "腾讯批量快照", fetched_at: "2026-10-16T07:00:00Z", trading_date: "2026-10-16", positions: [], missing: ["920575"],
  watchlist: names.map(([symbol, name, price, change_pct]) => ({ symbol, name, price, change_pct, amount_yuan: price == null ? null : 5075528306, limit_up: null, limit_down: null, quote_date: price == null ? null : "2026-10-16", is_today: price != null, available: price != null })),
};
const overview = {
  status: "ok", source: "eastmoney", fetched_at: "2026-10-16T07:00:00Z", trading_date: "2026-10-16", stale: false,
  indexes: [["000001", "上证指数", 3900.21, .71], ["399106", "深证综指", 2450.18, .83], ["899050", "北证50", 1340.22, -1.1], ["399001", "深证成指", 13200.52, 1.12], ["399006", "创业板指", 2901.66, .92], ["000300", "沪深300", 4522.81, .63]].map(([code, name, price, change_pct]) => ({ code, name, price, change_pct, change: 10, amount_yuan: 10000000000 })),
  breadth: { exchange: { up: 3200, down: 1800, flat: 135, amount_yuan: 1860000000000 }, distribution: [["lt5", "跌幅 >5%", 135], ["lt3", "-5%~-3%", 242], ["lt0", "-3%~0", 1423], ["eq0", "平盘", 135], ["gt0", "0~3%", 2580], ["gt3", "3%~5%", 478], ["gt5", "涨幅 >5%", 142]].map(([key, label, count]) => ({ key, label, count })) },
  sentiment: { limit_up: 82, broken_board: 19, limit_down: 9, seal_rate: 81.2 }, errors: [],
};
const responses = {
  "/health": { ok: true, version: version + "-mobile" },
  "/mobile/watchlist": { items: watchlist },
  "/mobile/market-overview": overview,
  "/mobile/monitor": monitor,
  "/mobile/data/summary": { source: "mobile_sqlite", storage: { total_bytes: 270336, database_bytes: 262144, wal_bytes: 8192, shm_bytes: 0 }, counts: { watchlist: 6, journal_entries: 28, journal_fills: 68, daily_reviews: 12, ai_messages: 18, open_positions: 3 }, ai: { configured: false, key_present: false, storage: "android_keystore", key_echoed: false } },
  "/mobile/research/profile": { symbol: "600519", market: "SH", name: "贵州茅台", industry: "白酒", ipo_date: "2001-08-27", total_shares: 1256000000, float_shares: 1256000000, total_market_cap: 1669224000000, float_market_cap: 1669224000000, source: "eastmoney" },
  "/mobile/research/financials": { rows: [], note: "测试样本", status: "partial" },
  "/mobile/research/announcements": { items: [], count: 0 },
  "/mobile/research/estimates": { reports: [], note: "测试样本", status: "partial" },
  "/mobile/stock/observations": { status: "ok", date: "2026-10-16", bars: 65, close: 1329, ma5: 1298.17, ma10: 1280.2, ma20: 1266, ma60: 1291, return5: 5.22, return20: 1.8, return60: 10.2, ma20_bias_pct: 4.97, ma_cross: "本根未发生 MA5/MA20 交叉", range_observation: "收盘高于此前20根最高价", prior20_high: 1317.76, prior20_low: 1228.1, volume_ratio_prev5: 1.1, same_day: false, stale: false, note: "冻结样本，仅用于界面检查", source: "eastmoney" },
  "/mobile/ai/config": { configured: false, model: "", base_url: "", key_present: false },
  "/mobile/ai/chat": { messages: [], count: 0 },
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
  "/mobile/journal/risk": journalRiskFixture,
  "/mobile/shortline/samples": samplesFixture,
};
const quote = { symbol: "600519", market: "SH", status: "ok", primary_source: "tencent", extra: { name: "贵州茅台", quote_date: "2026-10-16" }, evidence: [["price", 1329, "元"], ["change_pct", 1.22, "%"], ["pe_ttm", 19.81, "倍"], ["pb", 7.1, "倍"], ["turnover_pct", .31, "%"], ["total_market_cap", 16692.24, "亿元"]].map(([field, value, unit]) => ({ field, value, unit })) };
const browser = await chromium.launch({ headless: true });
const errors = [];
try {
  for (const width of [360, 390, 768]) {
    for (const theme of ["light", "dark"]) {
      const context = await browser.newContext({ viewport: { width, height: 850 }, deviceScaleFactor: 1, colorScheme: theme });
      const page = await context.newPage();
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.addInitScript(({ responses, quote, klineRows, theme }) => {
        if (!localStorage.getItem("vibe.ui.seeded")) {
          localStorage.setItem("vra-theme", theme);
          localStorage.setItem("vibe.mobile.density", "compact");
          localStorage.setItem("vibe.mobile.refresh", "0");
          localStorage.setItem("vibe.ui.seeded", "1");
        }
        window.__TAURI__ = { core: { invoke: async (command, args) => {
          if (command === "take_reminder_navigation") return null;
          if (command === "mobile_api") {
            const request = args.request;
            const route = request.path.split("?")[0];
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
      const assertWidth = async () => {
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
        assert.equal(overflow, false, width + " " + theme + " horizontal overflow");
      };
      await assertWidth();
      const navTops = await page.locator(".mobile-nav button").evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().top));
      assert.ok(navTops.length === 5 && Math.max(...navTops) - Math.min(...navTops) < 2, "bottom navigation must keep five groups in one row");
      await page.getByRole("region", { name: "情绪速览" }).getByText("炸板率", { exact: true }).waitFor();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-home.png"), fullPage: true });
      await page.getByRole("button", { name: "打开设置", exact: true }).click();
      await page.getByRole("button", { name: "白色模式", exact: true }).waitFor();
      assert.equal(await page.locator(".appearance-refresh select").inputValue(), "0");
      assert.equal(await page.locator("html").getAttribute("data-theme"), theme);
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-settings.png"), fullPage: true });
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
      await page.getByRole("button", { name: "返回", exact: true }).click();
      await page.locator(".mo-wrap").waitFor();
      const comfortableHeight = await page.locator(".mo-wrap").evaluate((el) => el.getBoundingClientRect().height);
      assert.ok(comfortableHeight > compactHeight, "comfortable layout must show a real spacing difference");
      await page.getByRole("button", { name: "打开设置", exact: true }).click();
      await page.getByRole("button", { name: "紧凑", exact: false }).click();
      await page.getByRole("button", { name: "返回", exact: true }).click();
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
      await page.locator(".group-tabs").getByRole("tab", { name: "竞价与盘中", exact: true }).click();
      await page.getByRole("heading", { name: "竞价与盘中", exact: true }).waitFor();
      await page.getByText("周末休市,没有竞价", { exact: true }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-intraday.png"), fullPage: true });
      // 复盘栏目:底部一级导航 → 今日情绪,栏目内子页签单行横向滚动 → 连板天梯。
      await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "复盘", exact: true }).click();
      await page.getByRole("heading", { name: "今日情绪", exact: true }).waitFor();
      await page.getByText("连续两日走强", { exact: true }).waitFor();
      const tabTops = await page.locator(".group-tabs [role=tab]").evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().top));
      assert.ok(tabTops.length >= 9 && Math.max(...tabTops) - Math.min(...tabTops) < 2, "review sub-tabs must stay in one scrollable row");
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-emotion.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "连板天梯", exact: true }).click();
      await page.getByText("新华传媒", { exact: true }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-ladder.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "情绪历史", exact: true }).click();
      await page.getByText("历史统计位置", { exact: true }).waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-history.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "涨停样本", exact: true }).click();
      await page.getByText("首板 · 最后封板时间曲线", { exact: false }).waitFor();
      await page.getByText("事后名单", { exact: false }).first().waitFor();
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-samples.png"), fullPage: true });
      await page.locator(".group-tabs").getByRole("tab", { name: "风险与纪律", exact: true }).click();
      await page.getByRole("heading", { name: "风险与纪律", exact: true }).waitFor();
      await page.getByText("权益曲线与回撤", { exact: true }).waitFor();
      await page.getByRole("img", { name: "权益曲线", exact: false }).waitFor();
      await page.getByRole("button", { name: "修改", exact: true }).click();
      await page.getByLabel("账户规模", { exact: true }).waitFor();
      assert.equal(await page.getByLabel("单笔最大亏损", { exact: true }).inputValue(), "4");
      await assertWidth();
      await page.screenshot({ path: path.join(out, width + "-" + theme + "-journal-risk.png"), fullPage: true });
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
  fs.writeFileSync(path.join(out, "verification.json"), JSON.stringify({ fixture_data: true, widths: [360, 390, 768], themes: ["light", "dark"], screenshots: 72, page_errors: errors }, null, 2));
  console.log("Mobile UI smoke passed: 6 viewport/theme combinations, theme persistence/system switch, density, navigation and review pages, stock chart interactions, live watch, intraday, samples, journal risk; 72 screenshots.");
} finally {
  await browser.close();
}
