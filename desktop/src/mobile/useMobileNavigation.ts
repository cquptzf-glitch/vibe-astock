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
  const [view, setView] = useState(() => ({path:navigation.snapshot(),workspace:navigation.workspace()}));

  useEffect(() => {
    function update() {
      const next = navigation.snapshot();
      const workspace = navigation.workspace();
      setView(previous => previous.path === next && previous.workspace === workspace ? previous : {path:next,workspace});
      try { window.localStorage.setItem(STORAGE_KEY, next); } catch { /* Navigation works without persistence. */ }
    }
    const unsubscribe = navigation.subscribe(update);
    const stopNative = listenReminderNavigation((route) => navigation.navigate(route));
    update();
    return () => { unsubscribe(); stopNative(); };
  }, [navigation]);

  return { route: parseMobileRoute(view.path)!, navigate: navigation.navigate, workspace: view.workspace, switchWorkspace: (group: Parameters<typeof navigation.switchWorkspace>[0]) => navigation.switchWorkspace(group), back: navigation.back };
}
