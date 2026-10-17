import assert from "node:assert/strict";
import test from "node:test";
import { withRenderer } from "./helpers/render.ts";

test("AI 复盘归档成笔记:标题带日期、分类固定、正文末尾写明模型/范围/时间和免责说明;含未平仓明细要写出来", async () => {
  await withRenderer(async (r) => {
    const { reviewNote, readArchivePref } = await r.load("/src/mobile/AiReviewPanel.tsx");
    const base = { response: "  整体纪律不错。\n计划外交易偏多。  ", model: "sample-model", scope: "aggregate", include_positions: false, sent_characters: 1234, generated_at: "2026-10-17T09:30:12+00:00" };
    const note = reviewNote(base);
    assert.equal(note.title, "AI 复盘 2026-10-17");
    assert.equal(note.category, "AI 复盘");
    assert.ok(note.body.startsWith("整体纪律不错。\n计划外交易偏多。\n\n---\n来源:AI 交易复盘 · 模型 sample-model · 发送范围 aggregate · 发送 1234 个提示字符 · 2026-10-17 09:30"));
    assert.ok(note.body.endsWith("不构成买卖建议。"));
    assert.ok(!note.body.includes("含未平仓明细"));
    assert.ok(reviewNote({ ...base, include_positions: true }).body.includes("发送范围 aggregate(含未平仓明细)"));
    assert.equal(readArchivePref(), true, "没有存储(或没设过)默认开");
  });
});
