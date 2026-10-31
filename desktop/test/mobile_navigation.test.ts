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

test("页面导航只保留当前标题，盯盘使用三个相关视图，复盘和研究通过列表进入", async () => {
  await withRenderer(async (r) => {
    const { GroupTabs, WorkspaceNav, FunctionDirectory, WorkspaceHub } = await r.load("/src/mobile/WorkspaceNavigation.tsx");
    const tabs = r.html(GroupTabs, { page: "watch", onNavigate: () => undefined });
    assert.equal((tabs.match(/role="tab"/g) ?? []).length, 3);
    for (const label of ["自选", "持仓", "异动"]) assert.ok(textOf(tabs).includes(label));
    const myHeader = textOf(r.html(GroupTabs, {page:"my",onNavigate:()=>undefined}));
    assert.ok(myHeader.includes("我的"));
    assert.ok(!myHeader.includes("研究资料"),"root headings describe the selected workspace");
    const detail = r.html(GroupTabs, { page: "ladder", onNavigate: () => undefined });
    assert.ok(textOf(detail).includes("连板天梯"));
    assert.equal((detail.match(/role="tab"/g) ?? []).length, 0);
    const review = textOf(r.html(WorkspaceHub, { group: "review", onNavigate: () => undefined }));
    for (const label of ["市场盘面", "复盘记录", "交易复盘", "今日情绪", "复查", "风险与纪律"]) assert.ok(review.includes(label), label);
    const nav = r.html(WorkspaceNav, { page: "stock", activeGroup: "watch", onNavigate: () => undefined });
    assert.match(nav, /aria-current="page"[^>]*>(?:<svg[\s\S]*?<\/svg>)?<span>盯盘/);
    const my = textOf(r.html(FunctionDirectory, { onNavigate: () => undefined, group: "my" }));
    for (const label of ["事件日历", "提醒规则", "应用设置"]) assert.ok(my.includes(label));
    for (const label of ["研究记录", "我的研报", "收件箱"]) assert.ok(!my.includes(label));
    const research = textOf(r.html(WorkspaceHub, { group: "research", onNavigate: () => undefined }));
    for (const label of ["研究记录", "我的研报", "收件箱"]) assert.ok(research.includes(label));
  });
});

test("功能分组覆盖全部实际页面且不重复，目录可按栏目筛选并保留设置入口", async () => {
  await withRenderer(async (r) => {
    const { WORKSPACE_SECTIONS, WORKSPACE_ENTRIES } = await r.load("/src/mobile/workspace.ts");
    const pages = WORKSPACE_SECTIONS.flatMap((section: { pages: string[] }) => section.pages);
    assert.equal(new Set(pages).size, pages.length);
    assert.deepEqual([...pages].sort(), WORKSPACE_ENTRIES.filter((entry: { page: string }) => entry.page !== "functions").map((entry: { page: string }) => entry.page).sort());
    const { WorkspaceHub, FunctionDirectory } = await r.load("/src/mobile/WorkspaceNavigation.tsx");
    const journal = textOf(r.html(WorkspaceHub, { group: "review", onNavigate: () => undefined }));
    for (const title of ["交易统计", "风险与纪律", "模式卡"]) assert.ok(journal.includes(title), title);

    const filtered = textOf(r.html(FunctionDirectory, { onNavigate: () => undefined, initialGroup: "research" }));
    for (const title of ["个股深挖", "历史回测", "板块轮动", "产业研究"]) assert.ok(filtered.includes(title), title);
    assert.ok(!filtered.includes("风险与纪律"));
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
    assert.equal(workspaceGroup("library"), "research");
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
