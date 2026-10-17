export const MOBILE_PATHS = {
  functions: "/mobile/functions",
  my: "/mobile/my",
  research: "/mobile/research",
  emotion: "/mobile/review/emotion",
  ladder: "/mobile/review/ladder",
  firstBoard: "/mobile/review/first-board",
  seal: "/mobile/review/seal",
  themes: "/mobile/review/themes",
  samples: "/mobile/review/samples",
  marketReview: "/mobile/review/market",
  weekly: "/mobile/review/weekly",
  history: "/mobile/review/history",
  verify: "/mobile/review/verify",
  notes: "/mobile/library/notes",
  reports: "/mobile/library/reports",
  inbox: "/mobile/library/inbox",
  market: "/mobile/market",
  stock: "/mobile/stock",
  monitor: "/mobile/shortline",
  intraday: "/mobile/shortline/intraday",
  limitUp: "/mobile/shortline/limit-up",
  risk: "/mobile/shortline/risk",
  rotation: "/mobile/discovery/rotation",
  news: "/mobile/discovery/news",
  signals: "/mobile/discovery/signals",
  board: "/mobile/board",
  sector: "/mobile/discovery/sector",
  candidate: "/mobile/discovery/candidate",
  calendar: "/mobile/discovery/calendar",
  backtest: "/mobile/discovery/backtest",
  watch: "/mobile/my/watch",
  hold: "/mobile/my/hold",
  records: "/mobile/journal",
  daily: "/mobile/journal/daily",
  review: "/mobile/journal/review",
  journalRisk: "/mobile/journal/risk",
  journalModes: "/mobile/journal/modes",
  journalExcursion: "/mobile/journal/excursion",
  journalEnv: "/mobile/journal/environment",
  journalAttribution: "/mobile/journal/attribution",
  journalProblems: "/mobile/journal/problems",
  ai: "/mobile/ai",
  library: "/mobile/library",
  reminders: "/mobile/reminders",
  settings: "/mobile/settings",
} as const;

export type MobilePage = keyof typeof MOBILE_PATHS;
export interface MobileRoute { page: MobilePage; symbol?: string; board?: string }

const SYMBOL = /^(?:\d{6}|(?:SH|SZ|BJ)\d{6}|\d{6}\.(?:SH|SZ|BJ))$/;

// Validate URL shape only. Market normalization remains the Rust SymbolKey contract.
export function parseMobileRoute(input: string): MobileRoute | null {
  if (input.length > 160) return null;
  let path: string;
  try { path = decodeURIComponent(input.replace(/^#/, "")); } catch { return null; }
  for (const [page, candidate] of Object.entries(MOBILE_PATHS)) {
    if (page !== "stock" && page !== "board" && path === candidate) return { page: page as MobilePage };
  }
  const boardPrefix = MOBILE_PATHS.board + "/";
  if (path.startsWith(boardPrefix)) {
    const board = path.slice(boardPrefix.length).toUpperCase();
    return /^BK[0-9]{4}$/.test(board) ? { page: "board", board } : null;
  }
  const prefix = MOBILE_PATHS.stock + "/";
  if (path.startsWith(prefix)) {
    const symbol = path.slice(prefix.length).toUpperCase();
    if (SYMBOL.test(symbol)) return { page: "stock", symbol };
  }
  return null;
}

export function mobilePath(route: MobileRoute): string {
  if (route.page === "board") {
    const board = route.board?.trim().toUpperCase() ?? "";
    return /^BK[0-9]{4}$/.test(board) ? MOBILE_PATHS.board + "/" + board : MOBILE_PATHS.market;
  }
  if (route.page !== "stock") return MOBILE_PATHS[route.page];
  const symbol = route.symbol?.trim().toUpperCase() ?? "";
  return SYMBOL.test(symbol) ? MOBILE_PATHS.stock + "/" + symbol : MOBILE_PATHS.market;
}

export interface NavigationHost {
  location: { hash: string };
  history: {
    state: unknown;
    pushState(data: unknown, unused: string, url: string): void;
    replaceState(data: unknown, unused: string, url: string): void;
    back(): void;
  };
  addEventListener(type: string, listener: () => void): void;
  removeEventListener(type: string, listener: () => void): void;
}

const HOME = MOBILE_PATHS.market;
const STATE_KEY = "vibeMobileDepth";

function depth(state: unknown): number | null {
  const value = (state as Record<string, unknown> | null)?.[STATE_KEY];
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;
}

export function createMobileNavigation(host: NavigationHost, restoredPath = "") {
  const initial: MobileRoute = parseMobileRoute(host.location.hash) ?? parseMobileRoute(restoredPath) ?? { page: "market" };
  const initialPath = mobilePath(initial);
  // A fresh stock/notification launch always has an in-app home to return to.
  if (depth(host.history.state) === null) {
    host.history.replaceState({ [STATE_KEY]: 0 }, "", "#" + HOME);
    if (initialPath !== HOME) host.history.pushState({ [STATE_KEY]: 1 }, "", "#" + initialPath);
  }
  const subscribers = new Set<() => void>();
  function snapshot(): string {
    return mobilePath(parseMobileRoute(host.location.hash) ?? { page: "market" });
  }
  function changed() {
    if (!parseMobileRoute(host.location.hash)) {
      host.history.replaceState({ [STATE_KEY]: depth(host.history.state) ?? 0 }, "", "#" + HOME);
    }
    for (const listener of subscribers) listener();
  }
  return {
    snapshot,
    subscribe(listener: () => void) {
      if (subscribers.size === 0) {
        host.addEventListener("popstate", changed);
        host.addEventListener("hashchange", changed);
      }
      subscribers.add(listener);
      return () => {
        subscribers.delete(listener);
        if (subscribers.size === 0) {
          host.removeEventListener("popstate", changed);
          host.removeEventListener("hashchange", changed);
        }
      };
    },
    navigate(route: MobileRoute) {
      const path = mobilePath(route);
      if (path === snapshot()) return;
      host.history.pushState({ [STATE_KEY]: (depth(host.history.state) ?? 0) + 1 }, "", "#" + path);
      changed();
    },
    back() {
      if ((depth(host.history.state) ?? 0) > 0) host.history.back();
      else if (snapshot() !== HOME) {
        host.history.replaceState({ [STATE_KEY]: 0 }, "", "#" + HOME);
        changed();
      }
    },
    dispose() {
      host.removeEventListener("popstate", changed);
      host.removeEventListener("hashchange", changed);
      subscribers.clear();
    },
  };
}
