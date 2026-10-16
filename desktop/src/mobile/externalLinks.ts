export function publicUrl(raw: string): string | null {
  try {
    const url = new URL(raw);
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password ? url.href : null;
  } catch { return null; }
}

export async function openPublicUrl(raw: string): Promise<void> {
  const url = publicUrl(raw);
  if (!url) throw new Error("这条资讯没有可打开的网页地址");
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) {
    await invoke("plugin:opener|open_url", { url });
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
