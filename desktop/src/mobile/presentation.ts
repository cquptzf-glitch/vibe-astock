import { useSyncExternalStore } from "react";

export type ThemeMode = "light" | "dark" | "system";
export type Density = "compact" | "comfortable";
export type RefreshSeconds = 0 | 30 | 60 | 120;
export interface Presentation { theme: ThemeMode; resolvedTheme: "light" | "dark"; density: Density; refreshSeconds: RefreshSeconds }
export interface PresentationHost {
  storage: Pick<Storage, "getItem" | "setItem">;
  systemDark(): boolean;
  apply(value: Presentation): void;
  listenSystem(listener: () => void): () => void;
  listenStorage(listener: () => void): () => void;
}
const DEFAULT: Presentation = { theme: "system", resolvedTheme: "dark", density: "compact", refreshSeconds: 60 };
const KEYS = { theme: "vra-theme", density: "vibe.mobile.density", refreshSeconds: "vibe.mobile.refresh" };
export function createPresentationStore(host: PresentationHost | null) {
  let current = DEFAULT;
  const listeners = new Set<() => void>();
  let stopSystem: () => void = () => undefined;
  let stopStorage: () => void = () => undefined;
  function read() {
    const value = { ...DEFAULT };
    if (host) {
      try {
        const theme = host.storage.getItem(KEYS.theme);
        value.theme = theme === "light" || theme === "dark" ? theme : "system";
        value.density = host.storage.getItem(KEYS.density) === "comfortable" ? "comfortable" : "compact";
        const refresh = Number(host.storage.getItem(KEYS.refreshSeconds) ?? 60);
        value.refreshSeconds = ([0, 30, 60, 120].includes(refresh) ? refresh : 60) as RefreshSeconds;
      } catch { /* Local preferences remain usable when persistent storage is unavailable. */ }
      value.resolvedTheme = value.theme === "system" ? (host.systemDark() ? "dark" : "light") : value.theme;
    }
    return value;
  }
  function publish(value: Presentation) {
    const changed = JSON.stringify(current) !== JSON.stringify(value);
    current = value;
    host?.apply(current);
    if (changed) for (const listener of listeners) listener();
  }
  return {
    snapshot: () => current,
    serverSnapshot: () => DEFAULT,
    subscribe(listener: () => void) {
      listeners.add(listener);
      if (listeners.size === 1 && host) {
        publish(read());
        stopSystem = host.listenSystem(() => publish({ ...current, resolvedTheme: current.theme === "system" ? (host.systemDark() ? "dark" : "light") : current.theme }));
        stopStorage = host.listenStorage(() => publish(read()));
      }
      return () => {
        listeners.delete(listener);
        if (!listeners.size) { stopSystem(); stopStorage(); }
      };
    },
    update(patch: Partial<Pick<Presentation, "theme" | "density" | "refreshSeconds">>) {
      const next = { ...current, ...patch };
      if (host) {
        for (const key of Object.keys(patch) as (keyof typeof KEYS)[]) {
          try { host.storage.setItem(KEYS[key], String(next[key])); } catch { /* Memory fallback. */ }
        }
        next.resolvedTheme = next.theme === "system" ? (host.systemDark() ? "dark" : "light") : next.theme;
      } else { next.resolvedTheme = next.theme === "light" ? "light" : "dark"; }
      publish(next);
    },
  };
}
function browserHost(): PresentationHost | null {
  if (typeof window === "undefined" || typeof document === "undefined") return null;
  const media = window.matchMedia?.("(prefers-color-scheme: dark)");
  return {
    storage: { getItem: (key) => window.localStorage.getItem(key), setItem: (key, value) => window.localStorage.setItem(key, value) },
    systemDark: () => media?.matches ?? true,
    apply(value) {
      const root = document.documentElement;
      root.dataset.theme = value.resolvedTheme;
      root.dataset.mobileDensity = value.density;
      root.style.colorScheme = value.resolvedTheme;
      root.classList.toggle("light", value.resolvedTheme === "light");
    },
    listenSystem(listener) { media?.addEventListener("change", listener); return () => media?.removeEventListener("change", listener); },
    listenStorage(listener) { window.addEventListener("storage", listener); return () => window.removeEventListener("storage", listener); },
  };
}
const store = createPresentationStore(browserHost());
export function usePresentation() {
  const value = useSyncExternalStore(store.subscribe, store.snapshot, store.serverSnapshot);
  return { ...value, update: store.update };
}
