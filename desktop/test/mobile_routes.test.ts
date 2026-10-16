import assert from "node:assert/strict";
import test from "node:test";
import { createMobileNavigation, mobilePath, parseMobileRoute, type NavigationHost } from "../src/mobile/mobileRoutes.ts";

function browser(hash = "") {
  const entries: { hash: string; state: unknown }[] = [{ hash, state: null }];
  const events = new Map<string, Set<() => void>>();
  let index = 0;
  const host: NavigationHost = {
    location: { get hash() { return entries[index]!.hash; } },
    history: {
      get state() { return entries[index]!.state; },
      pushState(state, _unused, hash) { entries.splice(++index); entries.push({ hash, state }); },
      replaceState(state, _unused, hash) { entries[index] = { hash, state }; },
      back() { if (index > 0) { index--; emit("popstate"); } },
    },
    addEventListener(type, fn) { if (!events.has(type)) events.set(type, new Set()); events.get(type)!.add(fn); },
    removeEventListener(type, fn) { events.get(type)?.delete(fn); },
  };
  function emit(type: string) { for (const listener of events.get(type) ?? []) listener(); }
  return {
    host, entries,
    forward() { if (index < entries.length - 1) { index++; emit("popstate"); } },
    hashChange(hash: string) { entries[index]!.hash = hash; emit("hashchange"); },
    listenerCount() { return [...events.values()].reduce((n, set) => n + set.size, 0); },
  };
}

test("移动路由仅接受应用页面和股票代码，不解析外部 URL 或任意路径", () => {
  assert.deepEqual(parseMobileRoute("#/mobile/journal/daily"), { page: "daily" });
  assert.deepEqual(parseMobileRoute("/mobile/stock/bj430047"), { page: "stock", symbol: "BJ430047" });
  assert.deepEqual(parseMobileRoute("/mobile/stock/000001.SZ"), { page: "stock", symbol: "000001.SZ" });
  for (const path of ["https://example.com/mobile/settings", "//example.com", "/mobile/stock/AAPL",
    "/mobile/stock/%2e%2e%2fsettings", "/mobile/stock/%ZZ", "/mobile/settings?key=secret",
    "/mobile/unknown", "/mobile/stock/600519/extra", "x".repeat(161)]) {
    assert.equal(parseMobileRoute(path), null, path);
  }
  // Contradictory market spellings must still be rejected by Rust, not a second JS normalizer.
  assert.deepEqual(parseMobileRoute("/mobile/stock/SZ600519"), { page: "stock", symbol: "SZ600519" });
});

test("候选池打开个股再返回，恢复候选页；支持前进和多级返回", () => {
  const b = browser();
  const nav = createMobileNavigation(b.host);
  const seen: string[] = [];
  const stop = nav.subscribe(() => seen.push(nav.snapshot()));
  nav.navigate({ page: "candidate" });
  nav.navigate({ page: "stock", symbol: "600519.SH" });
  nav.back();
  assert.equal(nav.snapshot(), "/mobile/discovery/candidate");
  b.forward();
  assert.equal(nav.snapshot(), "/mobile/stock/600519.SH");
  nav.back(); nav.back(); nav.back();
  assert.equal(nav.snapshot(), "/mobile/market");
  assert.deepEqual(seen, [
    "/mobile/discovery/candidate", "/mobile/stock/600519.SH", "/mobile/discovery/candidate",
    "/mobile/stock/600519.SH", "/mobile/discovery/candidate", "/mobile/market",
  ]);
  stop();
  assert.equal(b.listenerCount(), 0);
});

test("冷启动深链优先于保存页面，个股/日复盘各有可返回的首页", () => {
  for (const path of ["/mobile/stock/BJ920002", "/mobile/journal/daily", "/mobile/settings"]) {
    const b = browser("#" + path);
    const nav = createMobileNavigation(b.host, "/mobile/discovery/backtest");
    const stop = nav.subscribe(() => undefined);
    assert.equal(nav.snapshot(), path);
    assert.equal(b.entries.length, 2);
    nav.back();
    assert.equal(nav.snapshot(), "/mobile/market");
    stop();
  }
  const b = browser();
  const nav = createMobileNavigation(b.host, "/mobile/journal/daily");
  assert.equal(nav.snapshot(), "/mobile/journal/daily");
});

test("刷新保留路由及返回栈，重复点击不新增记录，未知 hash 安全回首页", () => {
  const b = browser();
  const nav = createMobileNavigation(b.host);
  nav.navigate({ page: "hold" });
  nav.navigate({ page: "hold" });
  assert.equal(b.entries.length, 2);
  const recreated = createMobileNavigation(b.host);
  const stop = recreated.subscribe(() => undefined);
  assert.equal(recreated.snapshot(), "/mobile/my/hold");
  assert.equal(b.entries.length, 2);
  b.hashChange("#/not-an-app-page");
  assert.equal(recreated.snapshot(), "/mobile/market");
  stop();
  assert.equal(b.listenerCount(), 0);
  assert.equal(mobilePath({ page: "stock", symbol: "<script>" }), "/mobile/market");
});
