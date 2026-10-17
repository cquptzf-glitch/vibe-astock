import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 合成数据(不是任何真实持仓):结构同 Rust Core /mobile/portfolio/summary 的返回。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-portfolio-synthetic.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;

test("已实现与分红:合计三格、在手与已清仓分开列、分红明细可改;红涨绿跌;没有交易记录的代码不编名字", async () => {
  await withRenderer(async (r) => {
    const { IncomeView } = await r.load("/src/mobile/PortfolioIncome.tsx");
    const html = r.html(IncomeView, { data: fixture, onEdit: noop, onAdd: noop });
    const text = flat(html);
    for (const part of ["已实现盈亏+1,560.5", "5 笔清仓 · 赚 3 亏 2", "现金分红+388.5", "合计+1,949", "在手", "样本白酒", "600519 · 100 股 · 成本 1,450", "已实现 +2,100(2 笔)", "分红 +300.5",
      "已清仓 / 只有分红", "样本银行", "已实现 -539.5(3 笔)", "现金分红明细", "2026-06-01", "年度分红", "记一笔", "费率用的是默认值", "成本法:移动加权平均", "不进任何 AI"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /class="down">-539\.5</);
    assert.match(html, /class="up">\+2,100</);
    assert.ok(html.includes('aria-label="修改 2026-06-01 样本白酒 分红"'));
    assert.ok(text.includes("000002"), "只有分红的代码列在已清仓里,用代码代替没有的名字");
  });
});

test("空状态:没有分红时说明不会自动抓取;分红编辑器含代码、日期、金额、备注,新建没有删除", async () => {
  await withRenderer(async (r) => {
    const { IncomeView, DividendEditor } = await r.load("/src/mobile/PortfolioIncome.tsx");
    const empty = { ...fixture, holdings: [], closed: [], dividends: [], totals: { ...fixture.totals, realized_pnl: 0, dividends: 0, combined: 0, closed_trades: 0, wins: 0, losses: 0 } };
    const text = flat(r.html(IncomeView, { data: empty, onEdit: noop, onAdd: noop }));
    assert.ok(text.includes(sq("还没记过分红。分红不会自动抓取")));
    assert.ok(!text.includes("在手") && !text.includes("已清仓"));
    const editor = r.html(DividendEditor, { item: fixture.dividends[1], today: "2026-10-17", busy: false, message: "分红金额必须是数字", onSave: noop, onDelete: noop, onCancel: noop });
    const etext = flat(editor);
    for (const part of ["代码", "到账日期(不能是未来)", "到手现金(元)", "备注(选填,最多 100 字)", "保存", "取消", "删除", "分红金额必须是数字"]) assert.ok(etext.includes(sq(part)), part);
    assert.match(editor, /value="600519"/);
    assert.match(editor, /value="300.5"/);
    assert.ok(!flat(r.html(DividendEditor, { item: null, today: "2026-10-17", busy: false, message: "", onSave: noop, onCancel: noop })).includes("删除"));
  });
});
