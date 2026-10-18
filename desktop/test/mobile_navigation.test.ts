import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

test("通知 IPC 跨 StrictMode 重挂载不丢冷启动入口，后台点击只导航一次并释放监听", async () => {
  const oldWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const oldDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const events = new EventTarget();
  const docEvents = new EventTarget();
  let resolve: (path: string | null) => void = () => undefined;
  let calls = 0;
  const windowStub = {
    __TAURI__: { core: { invoke: async (command: string) => {
      assert.equal(command, "take_reminder_navigation");
      calls++;
      if (calls === 1) return new Promise<string | null>((done) => { resolve = done; });
      return null;
    } } },
    addEventListener: events.addEventListener.bind(events),
    removeEventListener: events.removeEventListener.bind(events),
  };
  Object.defineProperty(globalThis, "window", { configurable: true, value: windowStub });
  Object.defineProperty(globalThis, "document", { configurable: true, value: docEvents });
  let stop = () => undefined;
  try {
    await withRenderer(async (r) => {
      const { listenReminderNavigation } = await r.load("/src/mobile/nativeNavigation.ts");
      const stale: unknown[] = [];
      const current: unknown[] = [];
      const cleanup = listenReminderNavigation((route: unknown) => stale.push(route));
      cleanup();
      stop = listenReminderNavigation((route: unknown) => current.push(route));
      resolve("/mobile/stock/BJ430047");
      await new Promise(setImmediate);
      assert.deepEqual(stale, []);
      assert.deepEqual(current, [{ page: "stock", symbol: "BJ430047" }]);
      windowStub.__TAURI__.core.invoke = async () => "/mobile/journal/daily";
      events.dispatchEvent(new Event("vibe-reminder-open"));
      await new Promise(setImmediate);
      assert.deepEqual(current[1], { page: "daily" });
      assert.equal(current.length, 2);
      stop();
      events.dispatchEvent(new Event("vibe-reminder-open"));
      await new Promise(setImmediate);
      assert.equal(current.length, 2);
    });
  } finally {
    stop();
    if (oldWindow) Object.defineProperty(globalThis, "window", oldWindow); else Reflect.deleteProperty(globalThis, "window");
    if (oldDocument) Object.defineProperty(globalThis, "document", oldDocument); else Reflect.deleteProperty(globalThis, "document");
  }
});

test("栏目子页签标出当前页，同组功能一屏可见，五个一级栏目对应桌面侧栏", async () => {
  await withRenderer(async (r) => {
    const { GroupTabs, WorkspaceNav, FunctionDirectory } = await r.load("/src/mobile/WorkspaceNavigation.tsx");
    const tabs = r.html(GroupTabs, { page: "backtest", onNavigate: () => undefined });
    assert.match(tabs, /aria-selected="true"[^>]*>历史回测/);
    for (const label of ["个股深挖", "板块轮动", "资讯雷达", "产业信号", "候选池", "AI 助手"]) assert.ok(textOf(tabs).includes(label), label);
    const review = textOf(r.html(GroupTabs, { page: "ladder", onNavigate: () => undefined }));
    for (const label of ["今日情绪", "连板天梯", "涨停全表", "首板名单", "盘面事实", "行业题材", "每日复盘", "交易日志", "交易统计"]) assert.ok(review.includes(label), label);
    assert.equal(r.html(GroupTabs, { page: "market", onNavigate: () => undefined }), "");
    const nav = r.html(WorkspaceNav, { page: "firstBoard", onNavigate: () => undefined });
    assert.deepEqual([...textOf(nav).matchAll(/首页|盯盘|复盘|研究|我的/g)].map((m) => m[0]), ["首页", "盯盘", "复盘", "研究", "我的"]);
    assert.match(nav, /aria-current="page"[^>]*>(?:<svg[\s\S]*?<\/svg>)?<span>复盘/);
    const my = textOf(r.html(FunctionDirectory, { onNavigate: () => undefined, group: "my" }));
    for (const label of ["事件日历", "提醒规则", "研究记录", "我的研报", "收件箱", "应用设置"]) assert.ok(my.includes(label), label);
    assert.ok(!my.includes("连板天梯"));
  });
});

test("新栏目路由可解析与反解析，工作台每个条目都有对应路由", async () => {
  await withRenderer(async (r) => {
    const { parseMobileRoute, mobilePath, MOBILE_PATHS } = await r.load("/src/mobile/mobileRoutes.ts");
    const { WORKSPACE_ENTRIES, workspaceGroup } = await r.load("/src/mobile/workspace.ts");
    for (const entry of WORKSPACE_ENTRIES) {
      assert.ok(MOBILE_PATHS[entry.page], entry.page);
      assert.deepEqual(parseMobileRoute(mobilePath({ page: entry.page })), { page: entry.page });
    }
    assert.deepEqual(parseMobileRoute("#/mobile/review/first-board"), { page: "firstBoard" });
    assert.equal(workspaceGroup("stock"), "research");
    assert.equal(workspaceGroup("library"), "my");
    assert.equal(workspaceGroup("records"), "review");
  });
});

test("每日复盘与回测页面仍可独立渲染", async () => {
  await withRenderer(async (r) => {
    const { BacktestPanel } = await r.load("/src/mobile/BacktestPanel.tsx");
    assert.match(textOf(r.html(BacktestPanel, {})), /用对话完成历史假设核对/);
    const { DailyReviewPanel } = await r.load("/src/mobile/DailyReviewPanel.tsx");
    const daily = r.html(DailyReviewPanel, { onOpenSymbol: () => undefined });
    assert.match(daily, /日复盘/);
    assert.match(daily, /今日观察/);
  });
});
