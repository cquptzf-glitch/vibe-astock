import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const noop = () => undefined;
const threads = [{ id: 1, title: "默认对话", updated_at: "a", count: 4 }, { id: 2, title: "估值讨论", updated_at: "b", count: 0 }];

test("对话栏:当前对话标题带消息数、有新建 / 重命名 / 删除;默认对话不能删;忙时全部禁用", async () => {
  await withRenderer(async (r) => {
    const { ThreadBar } = await r.load("/src/mobile/AiChatPanel.tsx");
    const html = r.html(ThreadBar, { threads, current: 2, busy: false, onSelect: noop, onCreate: noop, onRename: noop, onDelete: noop });
    assert.ok(flat(html).includes("估值讨论(0)"), "当前对话显示标题和消息数");
    for (const label of ["新对话", "重命名对话", "删除对话"]) assert.ok(html.includes(`aria-label="${label}"`), label);
    assert.doesNotMatch(html, /aria-label="删除对话"[^>]*disabled/);
    const def = r.html(ThreadBar, { threads, current: 1, busy: false, onSelect: noop, onCreate: noop, onRename: noop, onDelete: noop });
    assert.match(def, /<button[^>]*disabled[^>]*aria-label="删除对话"|<button[^>]*aria-label="删除对话"[^>]*disabled/, "默认对话不能删除");
    const busy = r.html(ThreadBar, { threads, current: 2, busy: true, onSelect: noop, onCreate: noop, onRename: noop, onDelete: noop });
    assert.equal((busy.match(/disabled/g) ?? []).length >= 4, true, "生成中不能切换或改对话");
  });
});

test("流式需要 Tauri 事件通道:没有就退回一次性回复;请求号每次不同且不超过 64 字符", async () => {
  await withRenderer(async (r) => {
    const { eventApi, newRequestId } = await r.load("/src/mobile/AiChatPanel.tsx");
    assert.equal(eventApi(), null);
    const a = newRequestId() as string, b = newRequestId() as string;
    assert.notEqual(a, b);
    assert.ok(a.length <= 64 && /^chat-[a-z0-9]+-[a-z0-9-]+$/i.test(a), a);
  });
});
