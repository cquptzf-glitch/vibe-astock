import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";

// 冻结样本:Rust Core 情绪历史 / 近 5 天 / 明日验证在 2026-10-16 收盘后的真实返回(裁剪了窗口)。
const fixture = JSON.parse(readFileSync(new URL("./fixtures/mobile-history-2026-10-16.json", import.meta.url), "utf8"));
const flat = (html: string) => textOf(html).replace(/\s+/g, "");
const sq = (value: string) => value.replace(/\s+/g, "");
const open = () => undefined;

test("情绪历史:分位、两头标记、噪声门槛以上的变化,缺数的日子合并成一行说明", async () => {
  await withRenderer(async (r) => {
    const { HistoryView } = await r.load("/src/mobile/HistoryPanels.tsx");
    const data = structuredClone(fixture.history);
    data.series.unshift(...["2026-09-17", "2026-09-18"].map((date) => ({ date, values: {}, status: "missing", settled: null, note: "超过东财保留期(约一个月),当时也没缓存", cached: false })));
    const text = flat(r.html(HistoryView, { data }));
    for (const part of ["历史统计位置", "最高连板高度11 板偏热", "分位 100%", "赚钱效应中位数-3.3%偏冷", "炸板率44.5%偏冷", "3 项处在历史两头",
      "复盘日 vs 前一交易日", "2026-10-15 → 2026-10-16", "炸板率16.3% → 44.5%+28 个点转冷", "涨停家数36 → 61+25转热", "3 项变化不超过噪声门槛",
      "逐日读数", "10-1661", "2026-09-17 ~ 2026-09-18 共 2 天没取到", "16 个交易日没取到读数"]) {
      assert.ok(text.includes(sq(part)), part);
    }
  });
});

test("近 5 天:热度、行业轮动矩阵(前三高亮、缺数为 —)、龙头谱系", async () => {
  await withRenderer(async (r) => {
    const { WeeklyView } = await r.load("/src/mobile/HistoryPanels.tsx");
    const html = r.html(WeeklyView, { data: fixture.weekly, onOpenSymbol: open });
    const text = flat(html);
    for (const part of ["近 5 天情绪热度", "10-1661", "最高 11 板 · 炸板率 44.5% · 1进2 13.3%", "新经典、慈文传媒、龙版传媒、丽人丽妆、百洋股份",
      "行业轮动", "汽车零部", "隔1天回流", "出版", "连续5天", "龙头谱系", "新华传媒当前最高标", "10-12 登顶 10 板", "自登顶+10.01%", "距高点-9.68%"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    assert.match(html, /class="top"[^>]*>14</);
    const gappy = structuredClone(fixture.weekly);
    gappy.rotation.themes[0].series[3] = null;
    gappy.rotation.missing_days = ["2026-10-15"];
    assert.ok(flat(r.html(WeeklyView, { data: gappy, onOpenSymbol: open })).includes(sq("2026-10-15 没取到，显示为 —（不是 0）")));
  });
});

test("明日验证:对账标成立/不成立/判不了与事后,编辑器显示阈值与今晚读数,锁定后只读", async () => {
  await withRenderer(async (r) => {
    const { VerifyView } = await r.load("/src/mobile/HistoryPanels.tsx");
    const data = structuredClone(fixture.verify);
    const row = (id: number, metric: string, label: string, kind: string, direction: string, actual: string | null, verified: boolean | null, late: boolean, prev: number | null, cur: number | null) => ({
      id, metric, label, kind, unit: kind === "count" ? "家" : "", direction, direction_label: { up: "上升", down: "下降", flat: "持平" }[direction], reason: id === 1 ? "高标断层后承接看分歧" : "",
      prev_value: prev, cur_value: cur, actual, actual_label: actual ? { up: "上升", down: "下降", flat: "持平" }[actual] : null, verified, late,
      baseline: id === 1 ? 0.25 : null, baseline_note: id === 1 ? "少见方向,判对一次含量高" : "", baseline_reason: id === 1 ? null : "只有 3 对可比日,样本太少,基准发生率会误导", edge: id === 1 ? 0.75 : null,
    });
    data.check.results = [
      row(1, "broken_rate", "炸板率", "rate", "up", "up", true, false, 0.163, 0.445),
      row(2, "limit_up_count", "涨停家数", "count", "down", "up", false, true, 36, 61),
      row(3, "market_limit_down", "全市场跌停家数", "count", "flat", null, null, false, 26, null),
    ];
    data.check.summary = { total: 3, decided: 2, hit: 1, rate: 0.5, undecidable: 1, baseline_covered: 1, expected_rate: 0.25, edge: 0.75 };
    data.plan.items = [{ id: 9, metric: "promotion_1to2", label: "1进2 晋级率", kind: "rate", direction: "down", reason: "首板扎堆汽车零部", base_value: 0.133, baseline: null, baseline_note: "", baseline_reason: "x" }];
    data.plan.next_date = "2026-10-19";
    const text = flat(r.html(VerifyView, { data, onSave: async () => undefined }));
    for (const part of ["2026-10-15 晚写的条件 → 2026-10-16 对账", "炸板率成立", "写的「上升」· 实际 上升（16.3% → 44.5%）", "理由：高标断层后承接看分歧",
      "这个方向历史发生率 25%；少见方向,判对一次含量高", "涨停家数不成立事后写的，不计入", "全市场跌停家数判不了", "判定 2/3 条，成立 1 条（50%）", "超额 +75%", "1 条数据取不到，不算判错",
      "写给下一场的条件2026-10-19", "下一场 09:15 起锁定", "今晚 13.3%", "变化超过 5 个点 才算变", "加一条", "累计战绩"]) {
      assert.ok(text.includes(sq(part)), part);
    }
    const locked = structuredClone(data);
    locked.plan.locked = true;
    locked.plan.lock_note = "2026-10-19 已经开盘竞价,这晚的条件不能再写或改(看过盘再写的判断不算数)";
    const lockedHtml = r.html(VerifyView, { data: locked, onSave: async () => undefined });
    assert.ok(flat(lockedHtml).includes(sq("2026-10-19 已经开盘竞价")));
    assert.ok(!flat(lockedHtml).includes("加一条"));
    assert.match(lockedHtml, /<button[^>]*role="combobox"[^>]*disabled/);
  });
});

test("复盘功能列表保留近 5 天、情绪历史、明日验证,且都有路由", async () => {
  await withRenderer(async (r) => {
    const { WorkspaceHub } = await r.load("/src/mobile/WorkspaceNavigation.tsx");
    const tabs = flat(r.html(WorkspaceHub, { group: "review", onNavigate: open }));
    for (const label of ["近5天", "情绪历史", "明日验证"]) assert.ok(tabs.includes(label), label);
    const { parseMobileRoute } = await r.load("/src/mobile/mobileRoutes.ts");
    assert.deepEqual(parseMobileRoute("#/mobile/review/verify"), { page: "verify" });
    assert.deepEqual(parseMobileRoute("#/mobile/review/history"), { page: "history" });
    assert.deepEqual(parseMobileRoute("#/mobile/review/weekly"), { page: "weekly" });
  });
});
