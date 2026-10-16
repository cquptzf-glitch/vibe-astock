import assert from "node:assert/strict";
import test from "node:test";
import { createPresentationStore, type Presentation, type PresentationHost } from "../src/mobile/presentation.ts";
import { textOf, withRenderer } from "./helpers/render.ts";

function host(seed: Record<string, string> = {}) {
  const values = new Map(Object.entries(seed));
  let dark = false;
  let systemListener = () => undefined;
  let storageListener = () => undefined;
  const applied: Presentation[] = [];
  const instance: PresentationHost = {
    storage: { getItem: (key) => values.get(key) ?? null, setItem: (key, value) => { values.set(key, value); } },
    systemDark: () => dark,
    apply: (value) => { applied.push(value); },
    listenSystem: (listener) => { systemListener = listener; return () => { systemListener = () => undefined; }; },
    listenStorage: (listener) => { storageListener = listener; return () => { storageListener = () => undefined; }; },
  };
  return { instance, values, applied, system(value: boolean) { dark = value; systemListener(); }, sync(key: string, value: string) { values.set(key, value); storageListener(); } };
}

test("白色/深色/跟随系统和紧凑偏好可保存恢复，系统变化不会覆盖手动选择", () => {
  const h = host();
  const store = createPresentationStore(h.instance);
  const stop = store.subscribe(() => undefined);
  assert.equal(store.snapshot().resolvedTheme, "light");
  assert.equal(store.snapshot().density, "compact");
  h.system(true);
  assert.equal(store.snapshot().resolvedTheme, "dark");
  store.update({ theme: "light", density: "comfortable", refreshSeconds: 30 });
  h.system(true);
  assert.equal(store.snapshot().resolvedTheme, "light");
  assert.equal(h.values.get("vra-theme"), "light");
  assert.equal(h.values.get("vibe.mobile.density"), "comfortable");
  stop();
  const restored = createPresentationStore(h.instance);
  const unsubscribe = restored.subscribe(() => undefined);
  assert.equal(restored.snapshot().theme, "light");
  assert.equal(restored.snapshot().refreshSeconds, 30);
  h.sync("vra-theme", "dark");
  assert.equal(restored.snapshot().resolvedTheme, "dark");
  unsubscribe();
});

test("存储受限时仍能切换外观和手动刷新，损坏值回退到有效偏好", () => {
  const h = host({ "vra-theme": "invalid", "vibe.mobile.density": "tiny", "vibe.mobile.refresh": "NaN" });
  const store = createPresentationStore(h.instance);
  const stop = store.subscribe(() => undefined);
  assert.equal(store.snapshot().theme, "system");
  assert.equal(store.snapshot().density, "compact");
  assert.equal(store.snapshot().refreshSeconds, 60);
  h.instance.storage.setItem = () => { throw new Error("blocked"); };
  store.update({ theme: "light", refreshSeconds: 0 });
  assert.equal(store.snapshot().resolvedTheme, "light");
  assert.equal(store.snapshot().refreshSeconds, 0);
  stop();
});

test("顶部持续显示设置入口和白色模式切换，快捷功能覆盖八个真实栏目", async () => {
  await withRenderer(async (r) => {
    const { MobileHeader, HomeShortcuts } = await r.load("/src/mobile/MobileHeader.tsx");
    const html = r.html(MobileHeader, { version: "0.1.17", ready: true, theme: "dark", onToggleTheme: () => undefined, onOpenSettings: () => undefined });
    assert.match(html, /aria-label="打开设置"/);
    assert.match(html, /aria-label="切换白色模式"/);
    assert.match(textOf(html), /设置/);
    const links = textOf(r.html(HomeShortcuts, { onNavigate: () => undefined }));
    for (const label of ["实时动态", "今日情绪", "连板天梯", "涨停全表", "板块轮动", "个股深挖", "交易日志", "每日复盘", "全部功能"]) assert.ok(links.includes(label), label);
  });
});

test("设置无需等待核心数据即可操作主题/密度/刷新，并直接进入模型及提醒", async () => {
  await withRenderer(async (r) => {
    const { SettingsPanel } = await r.load("/src/mobile/SettingsPanel.tsx");
    const text = textOf(r.html(SettingsPanel, { onOpenAi: () => undefined, onOpenReminders: () => undefined }));
    for (const label of ["白色模式", "深色模式", "跟随系统", "紧凑", "舒适", "仅手动刷新", "模型 API 设置", "提醒规则设置"]) assert.ok(text.includes(label), label);
  });
});

test("自选密集行直接呈现价格、涨跌幅、成交额，旧行情和缺值可辨认", async () => {
  await withRenderer(async (r) => {
    const { WatchQuoteRows } = await r.load("/src/mobile/WatchQuotesPanel.tsx");
    const html = r.html(WatchQuoteRows, { items: [
      { symbol: "600519", name: "贵州茅台", price: 1329, change_pct: 1.22, amount_yuan: 100000000, quote_date: "2026-10-16", available: true, is_today: true },
      { symbol: "000001", name: "平安银行", price: null, change_pct: null, amount_yuan: null, quote_date: null, available: false, is_today: false },
      { symbol: "920575", name: "样本", price: 8.5, change_pct: -1, amount_yuan: 100, quote_date: "2026-10-15", available: true, is_today: false },
    ], onOpenSymbol: () => undefined });
    const text = textOf(html);
    assert.match(text, /1,329/);
    assert.match(text, /\+1.22%/);
    assert.match(text, /1 亿/);
    assert.match(text, /暂无报价/);
    assert.match(text, /2026-10-15/);
    assert.match(html, /negative/);
    assert.match(text, /—/);
  });
});

test("行情概览显示六指数和成交额，部分涨跌数据不画虚假的比例", async () => {
  await withRenderer(async (r) => {
    const { MarketOverviewView } = await r.load("/src/mobile/MarketOverview.tsx");
    const data = { trading_date: "2026-10-16", fetched_at: "2026-10-16T07:00:00Z", stale: false, status: "partial",
      indexes: Array.from({ length: 6 }, (_, i) => ({ code: String(i), name: "指数" + i, price: 3000 + i, change_pct: 1 })),
      breadth: { exchange: { up: null, down: 10, flat: 2, amount_yuan: 100000000 }, distribution: [] },
      sentiment: { limit_up: 10, broken_board: 2, limit_down: null, seal_rate: 83.3 }, errors: [] };
    const html = r.html(MarketOverviewView, { data, loading: false, error: "", onRefresh: () => undefined });
    assert.match(textOf(html), /沪深北成交 1 亿 元/);
    assert.match(textOf(html), /样本 — 家/);
    assert.match(html, /涨跌比例缺少完整数据/);
    assert.match(textOf(html), /指数5/);
  });
});

test("个股卡片完整交易日窗口不足时保留缺值，不把缺失成交额显示为零", async () => {
  await withRenderer(async (r) => {
    const { StockDetailPanel } = await r.load("/src/mobile/StockDetailPanel.tsx");
    const rows = Array.from({ length: 5 }, (_, i) => [`2026-10-${10 + i}`, 10 + i, 11 + i, 12 + i, 9 + i, 100]);
    const text = textOf(r.html(StockDetailPanel, { quote: { symbol: "600519", market: "SH", evidence: [{ field: "amount", value: null }], primary_source: "eastmoney" }, rows }));
    assert.match(text, /近 5 日 —/);
    assert.match(text, /近 60 日 —/);
    assert.match(text, /MA20 —/);
    assert.doesNotMatch(text, /当前行情成交额字段/);
  });
});
