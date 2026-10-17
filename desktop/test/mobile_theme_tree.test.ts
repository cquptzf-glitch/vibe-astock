import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core /mobile/shortline/theme-tree 在 2026-10-17(周六)对真实上游的返回(复盘日 2026-10-16,同花顺涨停揭秘)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-theme-tree-2026-10-16.json", import.meta.url), "utf8"));
const emotion = JSON.parse(readFileSync(new URL("./fixtures/mobile-emotion-2026-10-16.json", import.meta.url), "utf8")).emotion;
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const open = () => undefined;

test("题材事件树:题材、状态标签、涨停/首板/连板/最高板、昨日延续、覆盖率与归因声明", async () => {
  await withRenderer(async (r) => {
    const { ThemeTreeView } = await r.load("/src/mobile/ThemeTree.tsx");
    const html = r.html(ThemeTreeView, { data: fixture, onOpenSymbol: open });
    const text = flat(html);
    const first = fixture.tree.themes[0];
    for (const part of ["低空经济", "今日新出现", `涨停${first.limit_up}(首板${first.first_boards}/连板${first.consec_boards})`, "商业航天", "扩散中", "昨日 1 只 → 今仍涨停 1(延续 100%)",
      `题材串覆盖 ${fixture.tree.covered}/${fixture.tree.total_limit_up} 只涨停`, "题材归因来自源站编辑,不是公司确认", "同花顺涨停揭秘"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /class="emo-state new"/);
    assert.match(html, /class="emo-state strong"/);
    // 红涨绿跌:延续率 < 50% 的昨日反馈为绿(down),≥ 50% 为红(up)
    assert.match(html, /<b class="down">0<\/b>/);
    assert.match(html, /<b class="up">1<\/b>/);
  });
});

test("题材串取不到时整棵树标不可用并说明原因,不退回行业分类", async () => {
  await withRenderer(async (r) => {
    const { ThemeTreeView } = await r.load("/src/mobile/ThemeTree.tsx");
    const data = { ...structuredClone(fixture), tree: { available: false, reason: "涨停原因串没取到(同花顺涨停揭秘:同花顺涨停揭秘分页缺失)。不用行业分类顶替:行业不是题材" }, reasons: {} };
    const text = flat(r.html(ThemeTreeView, { data, onOpenSymbol: open }));
    assert.ok(text.includes(sq("题材事件树暂不可用:涨停原因串没取到")) && text.includes(sq("不用行业分类顶替:行业不是题材")));
    assert.ok(!text.includes("今日新出现"));
  });
});

test("首板名单每行带涨停原因,取不到原因时照常列名单", async () => {
  await withRenderer(async (r) => {
    const { FirstBoardView } = await r.load("/src/mobile/EmotionPanels.tsx");
    const stock = emotion.first_board.stocks[0];
    const reasons = { [stock.code]: "数据中心+电网设备" };
    const withReason = flat(r.html(FirstBoardView, { data: emotion, onOpenSymbol: open, reasons }));
    assert.ok(withReason.includes(sq("数据中心+电网设备")));
    assert.ok(withReason.includes(stock.name));
    const without = flat(r.html(FirstBoardView, { data: emotion, onOpenSymbol: open }));
    assert.ok(!without.includes("数据中心+电网设备") && without.includes(stock.name));
  });
});

test("fixture 自洽:原因表只含当天涨停池里的票,题材成员代码都在原因表里", () => {
  const covered = fixture.tree.covered as number;
  assert.ok(Object.keys(fixture.reasons).length >= covered, "原因表覆盖数 ≥ 对上涨停池的只数");
  for (const t of fixture.tree.themes) for (const m of t.members) assert.ok(fixture.reasons[m.code], `${m.code} 应有原因串`);
});
