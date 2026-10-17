import { useEffect, useState } from "react";

export interface ProviderFailure { source: string; message: string }

export function useDiscoveryData<T>(path: string) {
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{ path: string; data: T | null; loading: boolean; error: string }>({
    path, data: null, loading: true, error: "",
  });
  useEffect(() => {
    let current = true;
    const controller = new AbortController();
    // 同一路径刷新时保留旧数据,避免定时刷新时整块闪空
    setState((previous) => ({ path, data: previous.path === path ? previous.data : null, loading: true, error: "" }));
    void fetch("/api" + path, { headers: { Accept: "application/json" }, signal: controller.signal })
      .then(async (response) => {
        const body = await response.json() as T & { message?: string };
        if (!response.ok) throw new Error(body.message || "数据源暂不可用，请重试");
        if (current) setState({ path, data: body, loading: false, error: "" });
      })
      .catch((error: unknown) => {
        if (current) setState({ path, data: null, loading: false, error: error instanceof Error ? error.message : String(error) });
      });
    return () => { current = false; controller.abort(); };
  }, [path, revision]);
  return {
    ...(state.path === path ? state : { path, data: null, loading: true, error: "" }),
    refresh: () => setRevision((value) => value + 1),
  };
}

export function fmtNumber(value: number | null | undefined, digits = 2): string {
  return value == null || !Number.isFinite(value) ? "—" : value.toLocaleString("zh-CN", { maximumFractionDigits: digits });
}

export function fmtPercent(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? "—" : (value > 0 ? "+" : "") + fmtNumber(value) + "%";
}

export function fmtYuan(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? "—"
    : Math.abs(value) >= 100_000_000 ? fmtNumber(value / 100_000_000) + " 亿"
    : Math.abs(value) >= 10_000 ? fmtNumber(value / 10_000) + " 万" : fmtNumber(value);
}

export function formatSampleTime(timestamp: number | null | undefined): string {
  if (timestamp == null || !Number.isFinite(timestamp)) return "时间未提供";
  const date = new Date(timestamp * 1000);
  return Number.isNaN(date.getTime()) ? "时间无效" : date.toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" });
}
