import "./share.css";

type ShareRequest = {
  title?: string;
  text: string;
  chooserTitle?: string;
};

type TauriCore = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

export async function shareText(request: ShareRequest): Promise<"android" | "web" | "clipboard"> {
  const title = request.title?.trim() || undefined;
  const text = request.text.trim();
  if (!text) throw new Error("没有可分享的内容。");
  if (text.length > 100_000) throw new Error("分享内容过长。");

  const tauri = window.__TAURI__ as unknown as { core?: TauriCore } | undefined;
  if (tauri?.core?.invoke) {
    await tauri.core.invoke<{ ok: boolean }>("share_text", {
      request: {
        title,
        text,
        chooser_title: request.chooserTitle?.trim() || "分享到",
      },
    });
    return "android";
  }

  if (navigator.share) {
    await navigator.share({ title, text });
    return "web";
  }

  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return "clipboard";
  }

  throw new Error("当前环境不支持系统分享或剪贴板。");
}
