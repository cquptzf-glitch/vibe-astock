import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;
// 合成消息(不是任何真实持仓或行情)
const list = { unread: 2, messages: [
  { id: 3, kind: "watch_move", day: "2026-10-16", title: "2026-10-16 自选异动(2 只)", body: "样本光模块 300308 +10.00%(涨停)\n样本新股 688001 -10.00%(跌停)\n阈值:涨跌幅 ≥ 5% 或触及涨跌停;只在打开 App 时取一次行情,不是实时推送。", route: "monitor", read: false, created_at: "a", updated_at: "b" },
  { id: 2, kind: "close_summary", day: "2026-10-16", title: "2026-10-16 收盘摘要", body: "涨停 61 家 · 炸板 49 · 跌停 10 · 最高 11 板 · 炸板率 44.5%\n只列已经发生的读数,不构成买卖建议。", route: "emotion", read: false, created_at: "a", updated_at: "b" },
  { id: 1, kind: "pre_open", day: "2026-10-16", title: "2026-10-16 盘前提醒", body: "上一交易日 2026-10-15 的情绪读数:", route: "nowhere", read: true, created_at: "a", updated_at: "b" },
] };

test("消息列表:类型标签、标题、正文原样换行、未读点;只有有效的去处才给「去看看」", async () => {
  await withRenderer(async (r) => {
    const { MessagesView } = await r.load("/src/mobile/MessagesPanel.tsx");
    const html = r.html(MessagesView, { list, onOpen: noop, onRead: noop, onDelete: noop });
    const text = flat(html);
    for (const part of ["自选异动", "收盘摘要", "盘前提醒", "2026-10-16 自选异动(2 只)", "样本光模块 300308 +10.00%(涨停)", "不是实时推送", "炸板率 44.5%", "不构成买卖建议"]) assert.ok(text.includes(sq(part)), part);
    assert.equal((html.match(/class="msg unread"/g) ?? []).length, 2);
    assert.equal((html.match(/aria-label="未读"/g) ?? []).length, 2);
    assert.equal((html.match(/去看看/g) ?? []).length, 2, "route 不认识的那条没有去处按钮");
    assert.equal((html.match(/标为已读/g) ?? []).length, 2, "已读的不再给标为已读");
    assert.ok(html.includes('aria-label="删除 2026-10-16 收盘摘要"'));
    const empty = flat(r.html(MessagesView, { list: { messages: [], unread: 0 }, onOpen: noop, onRead: noop, onDelete: noop }));
    assert.ok(empty.includes(sq("还没有消息")) && empty.includes("不是实时推送"));
  });
});

test("消息页:标题、三个工具按钮、没有后台推送的说明;导航在有未读时显示角标且限 9+", async () => {
  await withRenderer(async (r) => {
    const { MessagesPanel } = await r.load("/src/mobile/MessagesPanel.tsx");
    const page = r.html(MessagesPanel, { onNavigate: noop });
    const text = flat(page);
    for (const part of ["消息", "盘前提醒、收盘摘要、自选异动", "没有后台推送", "不含持仓与盈亏", "不构成买卖建议"]) assert.ok(text.includes(sq(part)), part);
    for (const label of ["全部已读", "清空消息", "刷新消息"]) assert.ok(page.includes(`aria-label="${label}"`), label);
    const { WorkspaceNav } = await r.load("/src/mobile/WorkspaceNavigation.tsx");
    assert.ok(!r.html(WorkspaceNav, { page: "market", onNavigate: noop }).includes("nav-badge"));
    assert.match(r.html(WorkspaceNav, { page: "market", onNavigate: noop, unreadMessages: 3 }), /class="nav-badge" aria-label="3 条未读消息">3</);
    assert.match(r.html(WorkspaceNav, { page: "market", onNavigate: noop, unreadMessages: 25 }), />9\+</);
    const { routeOf, KIND_LABEL } = await r.load("/src/mobile/MessagesPanel.tsx");
    assert.equal(routeOf(list.messages[0]), "monitor");
    assert.equal(routeOf(list.messages[2]), null);
    assert.deepEqual(Object.keys(KIND_LABEL), ["pre_open", "close_summary", "watch_move"]);
  });
});
