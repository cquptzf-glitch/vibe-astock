import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

const row = (symbol: string, over: Record<string, unknown> = {}) => ({
  symbol, name: "N" + symbol, price: 10, change_pct: 1, amount_yuan: 1e9, pe_ttm: 20, pb: 2, turnover_pct: 1, group: "", tags: [], available: true, is_today: true, ...over,
});
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const noop = () => undefined;

test("排序:缺值和亏损(负市盈率)永远排最后,升降序都一样;同值保持原来的先后;自定义顺序不动", async () => {
  await withRenderer(async (r) => {
    const { sortRows } = await r.load("/src/mobile/watchlistView.ts");
    const rows = [row("A", { pe_ttm: 30 }), row("B", { pe_ttm: -3 }), row("C", { pe_ttm: null }), row("D", { pe_ttm: 12 }), row("E", { pe_ttm: 12 })];
    const names = (list: { symbol: string }[]) => list.map((x) => x.symbol).join("");
    assert.equal(names(sortRows(rows, "pe_ttm", "asc")), "DEABC", "升序:12,12,30,然后是亏损与缺值(保持原先后 B C)");
    assert.equal(names(sortRows(rows, "pe_ttm", "desc")), "ADEBC", "降序:30,12,12,亏损与缺值照旧最后");
    assert.equal(names(sortRows(rows, "custom", "desc")), "ABCDE");
    const byChange = [row("A", { change_pct: -1 }), row("B", { change_pct: 3 }), row("C", { change_pct: null })];
    assert.equal(names(sortRows(byChange, "change_pct", "desc")), "BAC");
    assert.equal(names(sortRows(byChange, "change_pct", "asc")), "ABC");
    assert.equal(names(sortRows([row("b", { name: "中信" }), row("a", { name: "阿里" })], "name", "asc")), "ab", "名称按拼音");
  });
});

test("分组筛选与分组清单:未分组单独一类;分组按出现先后", async () => {
  await withRenderer(async (r) => {
    const { filterByGroup, groupsOf, NO_GROUP } = await r.load("/src/mobile/watchlistView.ts");
    const rows = [row("A", { group: "白酒" }), row("B"), row("C", { group: "银行" }), row("D", { group: "白酒" })];
    assert.deepEqual(groupsOf(rows), ["白酒", "银行"]);
    assert.deepEqual(filterByGroup(rows, "白酒").map((x: { symbol: string }) => x.symbol), ["A", "D"]);
    assert.deepEqual(filterByGroup(rows, NO_GROUP).map((x: { symbol: string }) => x.symbol), ["B"]);
    assert.equal(filterByGroup(rows, "").length, 4);
  });
});

test("自定义顺序移动:只在看到的这一批里换位,其它票的位置不动;到头不动", async () => {
  await withRenderer(async (r) => {
    const { moveVisible } = await r.load("/src/mobile/watchlistView.ts");
    const all = ["A", "B", "C", "D", "E"];
    assert.deepEqual(moveVisible(all, all, "C", -1), ["A", "C", "B", "D", "E"]);
    assert.deepEqual(moveVisible(all, ["B", "D", "E"], "D", -1), ["A", "D", "C", "B", "E"], "分组 B/D/E 里 D 上移:B、D 互换位置,A/C/E 不动");
    assert.deepEqual(moveVisible(all, all, "A", -1), all, "第一个不能再上移");
    assert.deepEqual(moveVisible(all, all, "E", 1), all);
    assert.deepEqual(moveVisible(all, ["B"], "B", 1), all);
  });
});

test("估值显示:亏损写「亏损」、净资产为负写明、缺值是 —;标签输入按逗号空格去重", async () => {
  await withRenderer(async (r) => {
    const { peText, pbText, parseTags } = await r.load("/src/mobile/watchlistView.ts");
    assert.deepEqual([peText(20.4), peText(123.4), peText(-3.3), peText(null), peText(0)], ["20.4", "123", "亏损", "—", "亏损"]);
    assert.deepEqual([pbText(6.611), pbText(-14.3), pbText(undefined)], ["6.61", "净资产为负", "—"]);
    assert.deepEqual(parseTags("核心, 高股息、核心  龙头\n"), ["核心", "高股息", "龙头"]);
  });
});

test("自选行:现价涨跌、PE/PB/换手/成交额、持仓标记、标签;红涨绿跌;缺报价写明", async () => {
  await withRenderer(async (r) => {
    const { WatchRowView } = await r.load("/src/mobile/WatchlistPanel.tsx");
    const base = { held: true, canMove: true, first: true, last: false, onOpen: noop, onMove: noop, onEdit: noop };
    const html = r.html(WatchRowView, { ...base, row: row("600519", { name: "贵州茅台", price: 1329, change_pct: 1.22, pe_ttm: 20.4, pb: 6.61, turnover_pct: 0.31, group: "白酒", tags: ["核心"] }) });
    const text = flat(html);
    for (const part of ["贵州茅台", "持仓", "600519 · 白酒", "1,329", "+1.22%", "PE20.4", "PB6.61", "换手0.31%", "核心"]) assert.ok(text.includes(sq(part)), part);
    assert.match(html, /class="up">\+1\.22%/);
    assert.match(html, /aria-label="上移 贵州茅台"[^>]*disabled/, "第一行不能再上移");
    assert.ok(html.includes('aria-label="下移 贵州茅台"') && html.includes('aria-label="编辑 贵州茅台"'));
    const loss = flat(r.html(WatchRowView, { ...base, held: false, canMove: false, row: row("000001", { name: "亏损股", pe_ttm: -5, pb: -1, available: false, price: null, change_pct: null, turnover_pct: null, amount_yuan: null }) }));
    for (const part of ["暂无报价", "PE亏损", "PB净资产为负", "换手—"]) assert.ok(loss.includes(sq(part)), part);
    assert.ok(!r.html(WatchRowView, { ...base, canMove: false, row: row("000001") }).includes("上移"), "非自定义顺序没有上移下移");
  });
});

test("编辑器:分组与标签输入、已有分组快捷选、保存/取消/移出自选", async () => {
  await withRenderer(async (r) => {
    const { MetaEditor } = await r.load("/src/mobile/WatchlistPanel.tsx");
    const html = r.html(MetaEditor, { row: row("600519", { name: "贵州茅台", group: "白酒", tags: ["核心", "龙头"] }), groups: ["白酒", "银行"], busy: false, message: "分组名最多 12 个字", onSave: noop, onRemove: noop, onCancel: noop });
    const text = flat(html);
    for (const part of ["分组(最多 12 字)", "标签(空格或逗号分隔,最多 5 个、每个 8 字)", "白酒", "银行", "保存", "取消", "移出自选", "分组名最多 12 个字"]) assert.ok(text.includes(sq(part)), part);
    assert.match(html, /aria-label="分组"[^>]*value="白酒"|value="白酒"[^>]*aria-label="分组"/);
    assert.match(html, /value="核心 龙头"/);
    assert.match(html, /class="on"[^>]*>白酒/, "当前分组高亮");
  });
});

test("批量添加:代码和名称混着输,按空格/逗号/顿号/换行拆开去重;名称完全一致优先,多只就列候选不替用户猜", async () => {
  await withRenderer(async (r) => {
    const view = await r.load("/src/mobile/watchlistView.ts");
    assert.deepEqual(view.splitAddTokens("600519，平安银行、600519 贵州茅台;  000001"), ["600519", "平安银行", "贵州茅台", "000001"]);
    assert.deepEqual(view.splitAddTokens("  ,, "), []);
    const pingan = [{ symbol: "000001", name: "平安银行" }, { symbol: "601318", name: "中国平安" }];
    assert.deepEqual(view.pickByName("平安银行", pingan), { kind: "hit", symbol: "000001", name: "平安银行" });
    assert.deepEqual(view.pickByName("茅台", [{ symbol: "600519", name: "贵州茅台" }]), { kind: "hit", symbol: "600519", name: "贵州茅台" }, "只搜到一只就用它");
    const amb = view.pickByName("平安", pingan);
    assert.equal(amb.kind, "ambiguous");
    assert.deepEqual(amb.candidates.map((c: { symbol: string }) => c.symbol), ["000001", "601318"], "名称不完全一致又有多只:全列出来让用户改用代码");
    assert.deepEqual(view.pickByName("不存在", []), { kind: "none" });
    assert.equal(view.pickByName("ST 国华", [{ symbol: "000001", name: "ST国华" }]).kind, "hit", "名称里的空格不影响比对");
  });
});
