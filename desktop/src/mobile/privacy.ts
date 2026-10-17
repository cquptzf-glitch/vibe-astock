/**
 * 隐私模式(移动版):金额、股数、自己写的价位遮成 ••••;输入框用密码样式;权益曲线不画(曲线高低就是盈亏大小)。
 * 只遮「个人交易数据」(持仓、成交、成本、盈亏、分红、账户规模);市场行情、涨跌幅百分比、情绪读数不遮。
 * 开关存在本机浏览器(localStorage),在设置页、持仓页、交易日志页都能切。个人交易数据本来就不进 AI,隐私模式只管屏幕上看得见什么。
 */
import { useSyncExternalStore } from "react";

export const MASK = "••••";
const KEY = "vibe.privacy";
const listeners = new Set<() => void>();

function read(): boolean {
  try { return typeof window !== "undefined" && window.localStorage.getItem(KEY) === "1"; } catch { return false; }
}
let current = read();

export function isPrivate(): boolean {
  return current;
}

export function setPrivate(next: boolean): void {
  current = next;
  try { window.localStorage.setItem(KEY, next ? "1" : "0"); } catch { /* 存储不可用:本次会话内仍生效 */ }
  for (const listener of listeners) listener();
}

/** 开着就换成 ••••,关着原样返回 */
export function priv(text: string): string {
  return current ? MASK : text;
}

export function usePrivacyMode(): { on: boolean; toggle: () => void } {
  const on = useSyncExternalStore((listener) => { listeners.add(listener); return () => listeners.delete(listener); }, () => current, () => false);
  return { on, toggle: () => setPrivate(!current) };
}
