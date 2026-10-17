import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 合成数据(不是任何真实成交):结构同 Rust Core /mobile/journal/import/preview 的返回。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-trade-import-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("导入预览:三格汇总、逐行状态与原因,已记过和有问题的行不当成可导入", async () => {
  await withRenderer(async (r) => {
    const { ImportPreviewView } = await r.load("/src/mobile/TradeImport.tsx");
    const html = r.html(ImportPreviewView, { preview: fixture });
    const text = flat(html);
    for (const part of ["可导入3", "2 轮交易 · 1 笔分红", "已记过1", "不重复导入", "有问题2", "不导入", "600519 样本白酒", "买入", "现金分红", "300", "可导入", "已记过(同代码、同日、同方向、同价同量),不重复导入",
      "卖出 999 股超过此前买入的 0 股", "手机端暂不支持「送转」", "按日期重放切成「一轮一轮」", "同一天先算买入再算卖出"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /class="ti-duplicate"/);
    assert.match(html, /class="ti-problem"/);
    assert.equal((html.match(/<tr class="ti-ok"/g) ?? []).length, 3);
  });
});

test("导入卡片默认收起;解码:UTF-8 优先,券商常见的 GBK 也能读;示例表头认得三类行", async () => {
  await withRenderer(async (r) => {
    const { TradeImportCard, decodeFile, TEMPLATE } = await r.load("/src/mobile/TradeImport.tsx");
    const collapsed = r.html(TradeImportCard, { onImported: () => undefined });
    assert.ok(flat(collapsed).includes(sq("从表格导入成交与分红")));
    assert.ok(!collapsed.includes("<textarea"), "收起时不渲染输入框");
    assert.match(collapsed, /aria-expanded="false"/);
    const utf8 = new TextEncoder().encode("日期,类型\n2026-01-01,买入").buffer;
    assert.equal(decodeFile(utf8), "日期,类型\n2026-01-01,买入");
    // 「日期,买入」的 GBK 字节
    const gbk = Uint8Array.from([0xC8, 0xD5, 0xC6, 0xDA, 0x2C, 0xC2, 0xF2, 0xC8, 0xEB]).buffer;
    assert.equal(decodeFile(gbk), "日期,买入");
    for (const word of ["日期", "类型", "代码", "买入", "卖出", "现金分红"]) assert.ok(TEMPLATE.includes(word), word);
  });
});
