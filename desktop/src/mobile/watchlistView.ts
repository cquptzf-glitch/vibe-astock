/** 我的自选的纯计算:排序、分组筛选、自定义顺序移动、估值显示。没有行情或不适用的读数一律排在最后,不当 0。 */
export interface BoardRow {
  symbol: string; name: string; price: number | null; change_pct: number | null; amount_yuan: number | null;
  pe_ttm?: number | null; pb?: number | null; turnover_pct?: number | null; group?: string; tags?: string[]; available: boolean;
  quote_date?: string | null; is_today?: boolean;
}
export type SortKey = "custom" | "change_pct" | "amount_yuan" | "turnover_pct" | "pe_ttm" | "pb" | "price" | "name";
export type SortDir = "asc" | "desc";
export const SORT_LABELS: Record<SortKey, string> = {
  custom: "自定义顺序", change_pct: "涨跌幅", amount_yuan: "成交额", turnover_pct: "换手率", pe_ttm: "市盈率", pb: "市净率", price: "现价", name: "名称",
};
export const NO_GROUP = "未分组";

/** 市盈率/市净率为负 = 亏损,不参与大小比较(排序时和没有数据一起排在最后) */
export function sortValue(row: BoardRow, key: SortKey): number | string | null {
  switch (key) {
    case "name": return row.name || row.symbol;
    case "pe_ttm": return row.pe_ttm != null && row.pe_ttm > 0 ? row.pe_ttm : null;
    case "pb": return row.pb != null && row.pb > 0 ? row.pb : null;
    case "custom": return null;
    default: return row[key] ?? null;
  }
}

export function sortRows(rows: BoardRow[], key: SortKey, dir: SortDir): BoardRow[] {
  if (key === "custom") return rows.slice();
  const sign = dir === "asc" ? 1 : -1;
  const indexed = rows.map((row, index) => ({ row, index, value: sortValue(row, key) }));
  indexed.sort((a, b) => {
    if (a.value == null && b.value == null) return a.index - b.index;
    if (a.value == null) return 1;
    if (b.value == null) return -1;
    const diff = typeof a.value === "string" || typeof b.value === "string" ? String(a.value).localeCompare(String(b.value), "zh-CN") : a.value - (b.value as number);
    return diff === 0 ? a.index - b.index : sign * diff;
  });
  return indexed.map((x) => x.row);
}

export function groupsOf(rows: BoardRow[]): string[] {
  const out: string[] = [];
  for (const row of rows) if (row.group && !out.includes(row.group)) out.push(row.group);
  return out;
}

export function filterByGroup(rows: BoardRow[], group: string): BoardRow[] {
  if (!group) return rows;
  if (group === NO_GROUP) return rows.filter((row) => !row.group);
  return rows.filter((row) => row.group === group);
}

/** 在「当前看到的这一批」里把 symbol 上移/下移一位,再按原来各自占的位置放回完整列表;到头了不动 */
export function moveVisible(all: string[], visible: string[], symbol: string, delta: -1 | 1): string[] {
  const at = visible.indexOf(symbol);
  const to = at + delta;
  if (at < 0 || to < 0 || to >= visible.length) return all.slice();
  const next = visible.slice();
  const moving = next[at] as string;
  next[at] = next[to] as string;
  next[to] = moving;
  const slots = all.map((code, index) => (visible.includes(code) ? index : -1)).filter((index) => index >= 0);
  const out = all.slice();
  slots.forEach((slot, i) => { out[slot] = next[i] as string; });
  return out;
}

export function peText(value: number | null | undefined): string {
  if (value == null) return "—";
  return value > 0 ? value.toFixed(value >= 100 ? 0 : 1) : "亏损";
}
export function pbText(value: number | null | undefined): string {
  if (value == null) return "—";
  return value > 0 ? value.toFixed(2) : "净资产为负";
}

/** 标签输入:逗号、顿号、空格、换行都当分隔;去重;不含超长/太多(服务端也会再拒一次) */
export function parseTags(text: string): string[] {
  const out: string[] = [];
  for (const part of text.split(/[,，、\s]+/)) if (part && !out.includes(part)) out.push(part);
  return out;
}

// ------------------------------------------------------------ 批量添加(代码或名称混着输)

/** 把输入拆成一个个代码或名称(空格、逗号、顿号、分号、换行都算分隔),去重保序 */
export function splitAddTokens(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const token of text.split(/[\s,，、;；]+/)) {
    const t = token.trim();
    if (t && !seen.has(t)) { seen.add(t); out.push(t); }
  }
  return out;
}

export interface NameCandidate { symbol: string; name: string }
export type NamePick = { kind: "hit"; symbol: string; name: string } | { kind: "ambiguous"; candidates: NameCandidate[] } | { kind: "none" };

const squash = (value: string) => value.replace(/\s+/g, "").replace(/[*＊]?ST/i, (m) => m.toUpperCase());

/** 按名称搜出来的结果里挑一只:名称完全一致优先;只搜到一只也算;多只都不完全一致就列候选,绝不替用户猜 */
export function pickByName(name: string, items: NameCandidate[]): NamePick {
  if (items.length === 0) return { kind: "none" };
  const exact = items.filter((item) => squash(item.name) === squash(name));
  const only = exact.length === 1 ? exact[0] : exact.length === 0 && items.length === 1 ? items[0] : undefined;
  if (only) return { kind: "hit", symbol: only.symbol, name: only.name };
  return { kind: "ambiguous", candidates: (exact.length > 1 ? exact : items).slice(0, 5) };
}
