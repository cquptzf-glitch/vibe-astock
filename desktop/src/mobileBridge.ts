type TauriCore = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

declare global {
  interface Window {
    __TAURI__?: { core?: TauriCore };
  }
}

interface MobileApiResponse {
  status: number;
  body: unknown;
}

interface MobileApiRequest {
  path: string;
  method: string;
  body: unknown;
}

function parseBody(text: string): unknown {
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

async function requestBody(input: RequestInfo | URL, init?: RequestInit): Promise<unknown> {
  if (typeof init?.body === "string") return parseBody(init.body);
  if (init?.body instanceof URLSearchParams) return init.body.toString();
  if (input instanceof Request && init?.body === undefined) {
    return parseBody(await input.clone().text());
  }
  return init?.body ?? null;
}

function requestMethod(input: RequestInfo | URL, init?: RequestInit): string {
  return (init?.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
}

function requestUrl(input: RequestInfo | URL): string {
  if (typeof input === "string") return input;
  if (input instanceof URL) return input.toString();
  return input.url;
}

const invoke = window.__TAURI__?.core?.invoke;

// Android/Tauri MVP compatibility layer. The desktop/web build never enters this branch,
// so the existing Vite proxy and Node orchestrator remain unchanged.
if (invoke) {
  const browserFetch = window.fetch.bind(window);

  window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = new URL(requestUrl(input), window.location.origin);
    if (url.origin !== window.location.origin || !url.pathname.startsWith("/api")) {
      return browserFetch(input, init);
    }

    init?.signal?.throwIfAborted();
    const request: MobileApiRequest = {
      path: `${url.pathname.slice(4) || "/"}${url.search}`,
      method: requestMethod(input, init),
      body: await requestBody(input, init),
    };

    try {
      const response = await invoke<MobileApiResponse>("mobile_api", { request });
      init?.signal?.throwIfAborted();
      return new Response(JSON.stringify(response.body ?? null), {
        status: response.status,
        headers: { "Content-Type": "application/json; charset=utf-8" },
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return new Response(JSON.stringify({ error: "mobile_bridge", message }), {
        status: 500,
        headers: { "Content-Type": "application/json; charset=utf-8" },
      });
    }
  };
}

export {};
