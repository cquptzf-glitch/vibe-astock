import { parseMobileRoute, type MobileRoute } from "./mobileRoutes";

const subscribers = new Set<(route: MobileRoute) => void>();
let checking = false;
let again = false;
let pending: MobileRoute | null = null;

// Use current subscribers when IPC resolves: StrictMode's mount/cleanup/remount
// must not consume a cold-start intent into a disposed component.
async function check() {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return;
  if (checking) { again = true; return; }
  checking = true;
  do {
    again = false;
    try {
      if (!pending) {
        const path = await invoke<string | null>("take_reminder_navigation");
        pending = path ? parseMobileRoute(path) : null;
      }
      if (pending && subscribers.size > 0) {
        const route = pending;
        pending = null;
        for (const subscriber of subscribers) subscriber(route);
      }
    } catch { /* A notification must not block normal navigation. */ }
  } while (subscribers.size > 0 && again);
  checking = false;
}

const onWake = () => { void check(); };

// Keep native IPC outside page components. Read the pending intent after React
// subscribes, so a cold-start notification cannot be lost before the UI is ready.
export function listenReminderNavigation(onOpen: (route: MobileRoute) => void): () => void {
  if (!window.__TAURI__?.core?.invoke) return () => undefined;
  if (subscribers.size === 0) {
    window.addEventListener("vibe-reminder-open", onWake);
    window.addEventListener("focus", onWake);
    document.addEventListener("visibilitychange", onWake);
  }
  subscribers.add(onOpen);
  void check();
  return () => {
    subscribers.delete(onOpen);
    if (subscribers.size === 0) {
      window.removeEventListener("vibe-reminder-open", onWake);
      window.removeEventListener("focus", onWake);
      document.removeEventListener("visibilitychange", onWake);
    }
  };
}
