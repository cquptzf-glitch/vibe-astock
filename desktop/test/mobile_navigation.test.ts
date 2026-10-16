import assert from "node:assert/strict";
import test from "node:test";
import { withRenderer } from "./helpers/render.ts";

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

test("移动发现路由和工作台子页渲染选中的栏目", async () => {
  await withRenderer(async (r) => {
    const { DiscoveryWorkspace } = await r.load("/src/mobile/DiscoveryWorkspace.tsx");
    const html = r.html(DiscoveryWorkspace, { view: "backtest", onViewChange: () => undefined });
    assert.match(html, /aria-selected="true"[^>]*>[\s\S]*?回测/);
    assert.match(html, /SMA/);
    const { JournalWorkspace } = await r.load("/src/mobile/JournalWorkspace.tsx");
    const daily = r.html(JournalWorkspace, { view: "daily", onViewChange: () => undefined, onOpenSymbol: () => undefined });
    assert.match(daily, /日复盘/);
    assert.match(daily, /今日观察/);
  });
});
