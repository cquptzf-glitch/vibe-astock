import assert from "node:assert/strict";
import test from "node:test";
import { withRenderer, textOf } from "./helpers/render.ts";

test("图表说明逐码点换行，不丢失来源与缺口，换行及 emoji 完整保留", async () => {
  await withRenderer(async (r) => {
    const { chartTextLines } = await r.load("/src/mobile/chartShare.ts");
    const measure = (s: string) => Array.from(s).length;
    const text = "来源甲：数据缺口🧪不应丢失";
    const lines = chartTextLines(text, measure, 5);
    assert.equal(lines.join(""), text);
    assert.ok(lines.every((line: string) => Array.from(line).length <= 5));
    assert.deepEqual(chartTextLines("第一行\n\n第二行", measure, 20), ["第一行", "", "第二行"]);
  });
});

test("图表只允许片段绘图引用，外部 URL 不会加载到导出图片", async () => {
  await withRenderer(async (r) => {
    const { localPaint } = await r.load("/src/mobile/chartShare.ts");
    assert.equal(localPaint("rgb(12, 20, 30)"), "rgb(12, 20, 30)");
    assert.equal(localPaint('url("#chart-clip")'), "url(#chart-clip)");
    assert.equal(localPaint("url(#gradient)"), "url(#gradient)");
    for (const value of ["url(https://example.com/image)", "url(data:image/png;base64,abc)", "url(//example.com/x)", "url(#local) url(https://example.com)"]) assert.throws(() => localPaint(value), /外部资源/);
  });
});

test("图表分享先生成预览，不直接发送；没有图表时不会生成其他页面内容", async () => {
  await withRenderer(async (r) => {
    const { ChartShareCard } = await r.load("/src/mobile/ChartShareCard.tsx");
    const html = r.html(ChartShareCard, { chartRef: { current: null }, spec: { title: "合成K线", subtitle: "样本区间", source: "合成源", details: [], notes: [] }, identity: "sample", slug: "sample" });
    const text = textOf(html);
    assert.match(text, /生成当前图表分享图/);
    assert.match(text, /数据、区间或指标改变后需重新生成/);
    assert.doesNotMatch(html, /<img/);
    assert.match(html, /data-page-actions="true"/);
    const { serializeChart } = await r.load("/src/mobile/chartShare.ts");
    assert.throws(() => serializeChart({ isConnected: false }), /没有可分享/);
    assert.throws(() => serializeChart({ isConnected: true, closest: () => ({}) }), /没有可分享/);
  });
});
