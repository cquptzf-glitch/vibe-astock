import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "./index.css";
import { PRODUCT_NAME } from "./verticals/finance/lib/brand";

/**
 * **组装根**:唯一一处把垂类接进外壳的地方。
 *
 * 桌面 / Web 继续使用原有 React Router + Node 编排器；Tauri Android 进入独立 MobileApp，
 * 由 mobileBridge.ts 将 `/api/*` 兼容调用收口到 Rust Core IPC。
 */

document.title = PRODUCT_NAME;

const root = document.getElementById("root");
if (!root) throw new Error("找不到 #root");

const isTauriMobile = Boolean(window.__TAURI__?.core?.invoke);

function fatal(message: string, detail: string) {
  root!.innerHTML = "";
  const box = document.createElement("div");
  box.style.cssText =
    "max-width:34rem;margin:18vh auto;padding:1.5rem;font:14px/1.7 system-ui,-apple-system,sans-serif;" +
    "color:#e5e7eb;background:#18181b;border:1px solid #3f3f46;border-radius:12px";
  const h = document.createElement("h1");
  h.textContent = message;
  h.style.cssText = "margin:0 0 .75rem;font-size:1.05rem;font-weight:600";
  const p = document.createElement("p");
  p.textContent = detail;
  p.style.cssText = "margin:0 0 1rem;color:#a1a1aa;white-space:pre-wrap";
  const tip = document.createElement("p");
  tip.textContent = isTauriMobile ? "请重启应用后重试。" : "macOS / Linux 请运行 scripts/start；Windows 请运行 scripts\\start.cmd。";
  tip.style.cssText = "margin:0;color:#a1a1aa";
  const retry = document.createElement("button");
  retry.type = "button";
  retry.textContent = "重新连接";
  retry.style.cssText =
    "margin-top:1rem;padding:.55rem .85rem;color:#fff;background:#e4572e;border:0;border-radius:8px;cursor:pointer";
  retry.addEventListener("click", () => window.location.reload());
  box.append(h, p, tip, retry);
  root!.append(box);
}

// Keep the desktop router and ledger hydration out of the Android startup path.
// Vite still emits desktop chunks for the shared distribution.
async function bootstrap() {
  if (isTauriMobile) {
    const { MobileApp } = await import("./mobile/MobileApp");
    createRoot(root!).render(<StrictMode><MobileApp /></StrictMode>);
    return;
  }
  const [{ RouterProvider }, { router }, { hydrateNotes }, { hydrateWatch }] = await Promise.all([
    import("react-router-dom"),
    import("./verticals/finance/router"),
    import("./verticals/finance/lib/notes"),
    import("./verticals/finance/lib/watchlist"),
  ]);
  await Promise.all([hydrateNotes(), hydrateWatch()]);
  createRoot(root!).render(<StrictMode><RouterProvider router={router} /></StrictMode>);
}

void bootstrap().catch((error: unknown) => {
  fatal("工作台未能加载", error instanceof Error ? error.message : String(error));
});
