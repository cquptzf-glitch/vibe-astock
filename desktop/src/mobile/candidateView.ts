// 候选池的纯计算:条件判断(满足全部 / 满足任一)、排序、条件组合的保存格式。
// 取不到的读数一律算「待补数据」,不当成「不符合」,也不当成 0。

export interface Range { enabled: boolean; min: string; max: string }
export type FilterKey = "pe" | "pb" | "marketCap" | "amount" | "turnover" | "changePct";
export type Filters = Record<FilterKey, Range>;
export type Mode = "all" | "any";
export type Metrics = Record<FilterKey, number | null>;

export const FILTER_KEYS: FilterKey[] = ["pe", "pb", "marketCap", "amount", "turnover", "changePct"];
export const FILTER_LABELS: Record<FilterKey, { label: string; unit: string }> = {
  pe: { label: "PE(TTM)", unit: "倍" }, pb: { label: "PB", unit: "倍" }, marketCap: { label: "总市值", unit: "亿元" },
  amount: { label: "成交额", unit: "亿元" }, turnover: { label: "换手率", unit: "%" }, changePct: { label: "涨跌幅", unit: "%" },
};

export const DEFAULT_FILTERS: Filters = {
  pe: { enabled: true, min: "0", max: "40" },
  pb: { enabled: false, min: "0", max: "6" },
  marketCap: { enabled: false, min: "20", max: "3000" },
  amount: { enabled: false, min: "1", max: "" },
  turnover: { enabled: false, min: "", max: "20" },
  changePct: { enabled: false, min: "-3", max: "" },
};

export type CheckResult = "off" | "pass" | "fail" | "missing";

export function checkRange(value: number | null, range: Range): CheckResult {
  if (!range.enabled) return "off";
  if (value === null || !Number.isFinite(value)) return "missing";
  const min = range.min.trim() === "" ? null : Number(range.min);
  const max = range.max.trim() === "" ? null : Number(range.max);
  if (min !== null && Number.isFinite(min) && value < min) return "fail";
  if (max !== null && Number.isFinite(max) && value > max) return "fail";
  return "pass";
}

export interface Evaluation {
  status: "met" | "unmet" | "missing";
  /** 满足的条件数 / 启用的条件数 */
  hits: number;
  enabled: number;
  failed: FilterKey[];
  missing: FilterKey[];
}

/** 满足全部:有一项不符合就不符合,没有不符合但有缺数据 = 待补数据。满足任一:有一项符合就符合,否则有缺数据 = 待补数据。没启用任何条件 = 全部符合 */
export function evaluate(metrics: Metrics, filters: Filters, mode: Mode): Evaluation {
  const results = FILTER_KEYS.map((key) => [key, checkRange(metrics[key], filters[key])] as const).filter(([, r]) => r !== "off");
  const hits = results.filter(([, r]) => r === "pass").length;
  const failed = results.filter(([, r]) => r === "fail").map(([k]) => k);
  const missing = results.filter(([, r]) => r === "missing").map(([k]) => k);
  let status: Evaluation["status"];
  if (results.length === 0) status = "met";
  else if (mode === "all") status = failed.length > 0 ? "unmet" : missing.length > 0 ? "missing" : "met";
  else status = hits > 0 ? "met" : missing.length > 0 ? "missing" : "unmet";
  return { status, hits, enabled: results.length, failed, missing };
}

export type SortKey = "hits" | "changePct" | "pe" | "pb" | "marketCap" | "amount" | "turnover" | "name";
export const SORT_LABELS: Record<SortKey, string> = {
  hits: "满足条件数", changePct: "涨跌幅", pe: "PE", pb: "PB", marketCap: "总市值", amount: "成交额", turnover: "换手率", name: "名称",
};

export interface SortableRow { name: string; symbol: string; metrics: Metrics; evaluation: Evaluation }

/** 排序:空值永远排在最后,不管升降序;相同的按代码排,结果稳定 */
export function sortCandidates<T extends SortableRow>(rows: T[], key: SortKey, dir: "asc" | "desc"): T[] {
  const sign = dir === "asc" ? 1 : -1;
  const value = (row: T): number | string | null => (key === "hits" ? row.evaluation.hits : key === "name" ? row.name || row.symbol : row.metrics[key]);
  return [...rows].sort((a, b) => {
    const x = value(a), y = value(b);
    if (x === null && y === null) return a.symbol.localeCompare(b.symbol);
    if (x === null) return 1;
    if (y === null) return -1;
    const cmp = typeof x === "string" || typeof y === "string" ? String(x).localeCompare(String(y), "zh-CN") : x - y;
    return cmp !== 0 ? cmp * sign : a.symbol.localeCompare(b.symbol);
  });
}

// ------------------------------------------------------------ 条件组合(存在本机浏览器里,不进备份)

export interface Preset { name: string; mode: Mode; filters: Filters }
export const PRESETS_MAX = 10;
export const PRESET_NAME_MAX = 20;

const isRange = (v: unknown): v is Range => {
  if (typeof v !== "object" || v === null) return false;
  const r = v as Record<string, unknown>;
  return typeof r.enabled === "boolean" && typeof r.min === "string" && typeof r.max === "string" && r.min.length <= 12 && r.max.length <= 12;
};

/** 读回已存的组合:格式不对的整条丢掉,不猜;最多 10 条 */
export function parsePresets(text: string | null): Preset[] {
  if (!text) return [];
  let raw: unknown;
  try { raw = JSON.parse(text); } catch { return []; }
  if (!Array.isArray(raw)) return [];
  const out: Preset[] = [];
  for (const item of raw) {
    if (typeof item !== "object" || item === null) continue;
    const p = item as Record<string, unknown>;
    const filters = p.filters as Record<string, unknown> | undefined;
    if (typeof p.name !== "string" || !p.name.trim() || p.name.length > PRESET_NAME_MAX || (p.mode !== "all" && p.mode !== "any") || !filters) continue;
    if (!FILTER_KEYS.every((k) => isRange(filters[k]))) continue;
    if (out.some((x) => x.name === p.name)) continue;
    out.push({ name: p.name, mode: p.mode, filters: Object.fromEntries(FILTER_KEYS.map((k) => [k, { ...(filters[k] as Range) }])) as Filters });
    if (out.length >= PRESETS_MAX) break;
  }
  return out;
}

/** 保存一个组合:同名覆盖;超过上限返回 null(不悄悄挤掉旧的) */
export function upsertPreset(list: Preset[], name: string, mode: Mode, filters: Filters): Preset[] | null {
  const n = name.trim();
  if (!n || n.length > PRESET_NAME_MAX) return null;
  const next: Preset = { name: n, mode, filters: Object.fromEntries(FILTER_KEYS.map((k) => [k, { ...filters[k] }])) as Filters };
  const at = list.findIndex((p) => p.name === n);
  if (at >= 0) return list.map((p, i) => (i === at ? next : p));
  if (list.length >= PRESETS_MAX) return null;
  return [...list, next];
}
