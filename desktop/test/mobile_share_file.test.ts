import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const emotion = JSON.parse(readFileSync(new URL("./fixtures/mobile-emotion-2026-10-16.json", import.meta.url), "utf8")).emotion;
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");

test("分享卡的行:只取数据里已有的读数,没有的写 —,不含自选和持仓;不可用的块不出现", async () => {
  await withRenderer(async (r) => {
    const { cardRows, CARD_FOOTER, CARD_WIDTH } = await r.load("/src/mobile/ShareCard.tsx");
    const rows = cardRows(emotion) as { label: string; value: string }[];
    const get = (label: string) => rows.find((x) => x.label === label)?.value;
    assert.equal(get("涨停"), String(emotion.summary.limit_up));
    assert.equal(get("跌停"), String(emotion.summary.limit_down));
    assert.equal(get("最高板"), `${emotion.summary.highest} 板`);
    assert.match(get("炸板率") ?? "", /^\d+(\.\d)?%$/);
    assert.ok(get("昨日涨停翻红率") && get("整体晋级率"));
    assert.ok(!rows.some((x) => /自选|持仓|成本|盈亏/.test(x.label)));
    const thin = structuredClone(emotion);
    thin.money_effect = { available: false, reason: "x" };
    thin.promotion = { available: false, reason: "y" };
    thin.summary.broken_rate = null;
    thin.summary.highest = 0;
    const thinRows = cardRows(thin) as { label: string; value: string }[];
    assert.equal(thinRows.length, 5, "不可用的两块不进卡片");
    assert.equal(thinRows.find((x) => x.label === "炸板率")?.value, "—");
    assert.equal(thinRows.find((x) => x.label === "最高板")?.value, "—");
    assert.ok(CARD_FOOTER.includes("不构成买卖建议") && CARD_WIDTH === 720);
  });
});

test("分享卡片区块默认只给「生成」按钮;没有 canvas 的环境画不出图返回 null", async () => {
  await withRenderer(async (r) => {
    const { EmotionShareCard, renderCard } = await r.load("/src/mobile/ShareCard.tsx");
    const html = r.html(EmotionShareCard, { data: emotion });
    const text = flat(html);
    assert.ok(text.includes(sq("图片分享卡")) && text.includes(sq(`生成 ${emotion.date} 的分享图`)) && text.includes("只含市场读数,不含自选与持仓".replace(/\s+/g, "")));
    assert.ok(!html.includes("<img"));
    assert.equal(renderCard(emotion), null);
  });
});

test("连板天梯分享卡:只取 2 板以上的层、高板在前、最多 6 层;数据不可用时没有行也画不出图;不含个人数据", async () => {
  await withRenderer(async (r) => {
    const { ladderRows, renderLadderCard, LadderShareCard } = await r.load("/src/mobile/ShareCard.tsx");
    const rows = ladderRows(emotion) as { label: string; value: string }[];
    assert.ok(rows.length >= 2 && rows[0].label === "涨停 / 最高板", JSON.stringify(rows));
    const boards = rows.slice(1).map((x) => Number(x.label.split(" ")[0]));
    assert.deepEqual(boards, [...boards].sort((a, b) => b - a), "高板在前");
    assert.ok(boards.every((b) => b >= 2) && boards.length <= 6);
    const mk = (n: number, count: number) => ({ boards: n, stocks: Array.from({ length: count }, (_, i) => ({ name: "股" + n + "_" + i })), fell: [], base: null, promoted: null, rate: null });
    const many = { ...emotion, ladder: { available: true, total_limit_up: 90, highest: 9, consec_count: 30, prev_available: true, prev_note: "", levels: [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => mk(n, n === 9 ? 5 : 1)) } };
    const r2 = ladderRows(many) as { label: string; value: string }[];
    assert.equal(r2.length, 7, "汇总行 + 6 层(首板层不画)");
    assert.equal(r2[1].label, "9 板 · 5 只");
    assert.ok(r2[1].value.endsWith(" 等") && r2[1].value.split("、").length === 3, r2[1].value);
    assert.deepEqual(ladderRows({ ...emotion, ladder: { available: false, reason: "x" } }), []);
    assert.equal(renderLadderCard({ ...emotion, ladder: { available: false, reason: "x" } }), null);
    assert.equal(renderLadderCard(emotion), null, "没有 canvas 的环境画不出图");
    const text = flat(r.html(LadderShareCard, { data: emotion }));
    assert.ok(text.includes(sq("图片分享卡")) && text.includes(sq(`生成 ${emotion.date} 的分享图`)));
  });
});

test("分享成 .md 文件:标题、分类/股票/日期一行、正文;中文能转成 base64 并还原", async () => {
  await withRenderer(async (r) => {
    const { documentMarkdown } = await r.load("/src/mobile/LibraryPanel.tsx");
    const md = documentMarkdown({ title: "样本研报", category: "光模块", symbol: "300308", updated_at: "2026-10-16T09:00:00Z", body: "第一行\n第二行" });
    assert.equal(md, "# 样本研报\n\n> 光模块 · 300308 · 2026-10-16\n\n第一行\n第二行\n");
    assert.equal(documentMarkdown({ title: "t", category: "", symbol: null, updated_at: "2026-10-16T09:00:00Z", body: "" }), "# t\n\n> 2026-10-16\n\n(无正文)\n");
    const { textToBase64 } = await r.load("/src/mobile/nativeShare.ts");
    const b64 = textToBase64(md);
    assert.equal(Buffer.from(b64, "base64").toString("utf8"), md);
  });
});
