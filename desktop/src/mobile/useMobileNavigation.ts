import { useEffect, useState } from "react";
import { createMobileNavigation, parseMobileRoute } from "./mobileRoutes";
import { listenReminderNavigation } from "./nativeNavigation";

const STORAGE_KEY = "vibe.mobile.route.v1";

export function useMobileNavigation() {
  const [navigation] = useState(() => {
    let restored = "";
    try { restored = window.localStorage.getItem(STORAGE_KEY) ?? ""; } catch { /* Storage is optional. */ }
    return createMobileNavigation(window, restored);
  });
  const [path, setPath] = useState(navigation.snapshot);

  useEffect(() => {
    function update() {
      const next = navigation.snapshot();
      setPath(next);
      try { window.localStorage.setItem(STORAGE_KEY, next); } catch { /* Navigation works without persistence. */ }
    }
    const unsubscribe = navigation.subscribe(update);
    const stopNative = listenReminderNavigation((route) => navigation.navigate(route));
    update();
    return () => { unsubscribe(); stopNative(); };
  }, [navigation]);

  return { route: parseMobileRoute(path)!, navigate: navigation.navigate, workspace: navigation.workspace(), switchWorkspace: (group: Parameters<typeof navigation.switchWorkspace>[0]) => navigation.switchWorkspace(group), back: navigation.back };
}
