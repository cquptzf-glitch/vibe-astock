import { usePageActive } from "./PageActivity.ts";
import { useEffect, useRef } from "react";
export function useForegroundRefresh(refresh: () => Promise<void>, seconds: number) {
  const active = usePageActive();
  const scheduled = useRef(false);
  const latest = useRef(refresh);
  latest.current = refresh;
  useEffect(() => {
    if (!seconds || !active) return;
    let timer: ReturnType<typeof setInterval> | undefined;
    let busy = false;
    const tick = async () => {
      if (busy || document.visibilityState === "hidden") return;
      busy = true;
      try { await latest.current(); } finally { busy = false; }
    };
    const schedule = () => {
      if (timer) clearInterval(timer);
      if (document.visibilityState !== "hidden") timer = setInterval(() => void tick(), seconds * 1000);
    };
    const visible = () => { schedule(); if (document.visibilityState !== "hidden") void tick(); };
    schedule();
    if (scheduled.current) void tick();
    scheduled.current = true;
    document.addEventListener("visibilitychange", visible);
    return () => { if (timer) clearInterval(timer); document.removeEventListener("visibilitychange", visible); };
  }, [seconds, active]);
}
