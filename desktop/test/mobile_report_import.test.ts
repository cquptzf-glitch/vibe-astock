import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;

test("研报文件草稿:文本类才收,标题取文件名,换行统一,二进制/空/超长/PDF 都拒收并说原因", async () => {
  await withRenderer(async (r) => {
    const { draftOf, MAX_CHARS } = await r.load("/src/mobile/ReportImport.tsx");
    const ok = draftOf("样本公司深度报告.md", "﻿# 标题\r\n正文\r第三行");
    assert.deepEqual(ok, { title: "样本公司深度报告", body: "# 标题\n正文\n第三行", chars: 11 });
    assert.equal(draftOf("a.b.c.txt", "x").title, "a.b.c");
    assert.equal(draftOf("很长".repeat(60) + ".txt", "x").title.length, 80);
    assert.match(draftOf("报告.pdf", "x").error, /PDF、Word、Excel/);
    assert.match(draftOf("a.txt", " \n ").error, /没有文字/);
    assert.match(draftOf("a.txt", "ab\u0000cd").error, /二进制/);
    assert.match(draftOf("a.txt", "字".repeat(MAX_CHARS + 1)).error, /超过上限/);
    assert.equal(draftOf("a.txt", "字".repeat(MAX_CHARS)).chars, MAX_CHARS);
  });
});

test("导入预览:标题分类代码可改,显示字符数,只预览开头但说明保存的是全文;卡片默认收起", async () => {
  await withRenderer(async (r) => {
    const { ReportPreview, ReportImport } = await r.load("/src/mobile/ReportImport.tsx");
    const body = "第一行\n" + "字".repeat(2000);
    const html = r.html(ReportPreview, { draft: { title: "样本", body, chars: body.length }, title: "样本", category: "光模块", symbol: "300308", busy: false, onTitle: noop, onCategory: noop, onSymbol: noop, onSave: noop, onCancel: noop });
    const text = flat(html);
    for (const part of ["标题", "分类(选填)", "股票代码(选填)", "共 2,004 个字符", "保存的是全文", "保存为研报", "取消", "…"]) assert.ok(text.includes(sq(part)), part);
    assert.match(html, /value="300308"/);
    assert.ok(!html.includes("字".repeat(801)), "预览只放开头 800 字");
    const blank = r.html(ReportPreview, { draft: { title: "", body: "x", chars: 1 }, title: "  ", category: "", symbol: "", busy: false, onTitle: noop, onCategory: noop, onSymbol: noop, onSave: noop, onCancel: noop });
    assert.match(blank, /<button[^>]*disabled[^>]*>保存为研报/, "没有标题不能保存");
    const card = r.html(ReportImport, { onSaved: noop });
    assert.ok(flat(card).includes(sq("从文件导入研报(先预览)")) && !card.includes('type="file"'));
    assert.match(card, /aria-expanded="false"/);
  });
});
