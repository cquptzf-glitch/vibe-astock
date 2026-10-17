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

export interface ShareFileRequest { name: string; mime: "image/png" | "image/jpeg" | "text/markdown" | "text/plain" | "application/json"; base64: string; title?: string; chooserTitle?: string }

/** UTF-8 文本 → base64(btoa 只认 Latin-1,中文要先转字节) */
export function textToBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/** 分享一个文件(图片分享卡、报告 .md)。Android 走原生分享面板(FileProvider);浏览器里走 Web Share,再不行就下载 */
export async function shareFile(request: ShareFileRequest): Promise<"android" | "web" | "download"> {
  if (!request.base64) throw new Error("没有可分享的内容。");
  if (request.base64.length > 11_000_000) throw new Error("文件太大(最多 8 MB)。");
  const tauri = window.__TAURI__ as unknown as { core?: TauriCore } | undefined;
  if (tauri?.core?.invoke) {
    await tauri.core.invoke<{ ok: boolean }>("share_file", {
      request: { title: request.title?.trim() || undefined, name: request.name, mime: request.mime, data_base64: request.base64, chooser_title: request.chooserTitle?.trim() || "分享到" },
    });
    return "android";
  }
  const bytes = Uint8Array.from(atob(request.base64), (c) => c.charCodeAt(0));
  const file = new File([bytes], request.name, { type: request.mime });
  if (navigator.canShare?.({ files: [file] })) {
    await navigator.share({ files: [file], title: request.title });
    return "web";
  }
  const url = URL.createObjectURL(file);
  const link = document.createElement("a");
  link.href = url;
  link.download = request.name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  return "download";
}
