import {
  BrainCircuit,
  Eye,
  KeyRound,
  RefreshCw,
  Save,
  Send,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";

import { AI_PRESETS, presetOf, PROBE_CODE, renderProbeImage } from "./aiPresets";
import { WebSelect } from "./controls";
import "./ai-review.css";

type AiConfig = {
  protocol: string;
  base_url: string;
  model: string;
  vision: boolean;
  key_present: boolean;
  configured: boolean;
  updated_at: string | null;
  storage: string;
  key_echoed: boolean;
};

type Preview = {
  network_used: boolean;
  scope: string;
  include_positions: boolean;
  sent_fields: string[];
  excluded: string[];
  payload: Record<string, unknown>;
  prompt_characters: number;
  custom_focus: string | null;
};

type AiResult = {
  status: string;
  protocol: string;
  model: string;
  base_url: string;
  scope: string;
  include_positions: boolean;
  sent_characters: number;
  response: string;
  generated_at: string;
};

const ARCHIVE_KEY = "vibe.ai.review.archive";
/** 默认开:AI 复盘跑完自动存一份到「研究记录」(本机笔记,可以改、可以删) */
export function readArchivePref(): boolean {
  try { return window.localStorage.getItem(ARCHIVE_KEY) !== "0"; } catch { return true; }
}
/** AI 复盘结果 → 研究记录里的一篇笔记:标题带日期,正文是模型回答加一段来源说明(模型、发送范围、时间) */
export function reviewNote(result: Pick<AiResult, "response" | "model" | "scope" | "include_positions" | "sent_characters" | "generated_at">): { title: string; body: string; category: string } {
  const day = result.generated_at.slice(0, 10);
  const footer = `

---
来源:AI 交易复盘 · 模型 ${result.model} · 发送范围 ${result.scope}${result.include_positions ? "(含未平仓明细)" : ""} · 发送 ${result.sent_characters} 个提示字符 · ${result.generated_at.slice(0, 16).replace("T", " ")}
模型生成的内容,可能有误,仅作复盘参考,不构成买卖建议。`;
  return { title: `AI 复盘 ${day}`, body: result.response.trim() + footer, category: "AI 复盘" };
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as T;
}

export function AiReviewPanel({ settingsOnly = false }: { settingsOnly?: boolean } = {}) {
  const [config, setConfig] = useState<AiConfig | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [vision, setVision] = useState(false);
  const [archive, setArchive] = useState(readArchivePref);
  const [includePositions, setIncludePositions] = useState(false);
  const [focus, setFocus] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [result, setResult] = useState<AiResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  async function loadConfig() {
    const next = await api<AiConfig>("/mobile/ai/config");
    setConfig(next);
    setBaseUrl(next.base_url);
    setModel(next.model);
    setVision(Boolean(next.vision));
    return next;
  }

  /** 选一个常用厂商:只把地址和模型名填进表单(不保存);换了地址要重新填 Key,所以顺手提醒 */
  function applyPreset(id: string) {
    const preset = AI_PRESETS.find((p) => p.id === id);
    if (!preset) return;
    setBaseUrl(preset.base_url);
    setModel(preset.model || model);
    setMessage(`已填入 ${preset.name} 的地址${preset.model ? "和默认模型" : ""}。换了厂商要重新填 API Key,填好后点「保存配置」。`);
  }

  /** 测试图片识别:用本机画的一张小图问模型,读出 600519 才算能读图;成功就把「支持图片识别」打开并保存 */
  async function probeVision() {
    const image = renderProbeImage();
    if (!image) { setError("当前环境画不出测试图,请手动打开「支持图片识别」再到截图导入里试。"); return; }
    setLoading(true);
    setError("");
    setMessage("");
    try {
      const out = await api<{ drafts: { code: string | null }[] }>("/mobile/ai/recognize", {
        method: "POST",
        body: JSON.stringify({ images: [{ name: "probe.png", mime: image.mime, data: image.data }], target: "position", probe: true }),
      });
      if (out.drafts.some((d) => d.code === PROBE_CODE)) {
        const next = await api<AiConfig>("/mobile/ai/config", { method: "POST", body: JSON.stringify({ vision: true }) });
        setConfig(next);
        setVision(true);
        setMessage("测试通过:模型读出了测试图里的代码。已打开「支持图片识别」。");
      } else {
        setVision(false);
        setError("模型回答了,但没读出测试图里的代码 600519:这个模型可能不能看图(或看得不准)。换一个多模态模型再测,或先别打开这个开关。");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadConfig().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  async function saveConfig() {
    setLoading(true);
    setError("");
    setMessage("");
    try {
      const next = await api<AiConfig>("/mobile/ai/config", {
        method: "POST",
        body: JSON.stringify({
          base_url: baseUrl.trim(),
          model: model.trim(),
          vision,
          ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
        }),
      });
      setConfig(next);
      setApiKey("");
      setMessage("模型配置已保存。API Key 已写入 Android Keystore，不会从接口回显。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function clearConfig() {
    setLoading(true);
    setError("");
    setMessage("");
    try {
      const next = await api<AiConfig>("/mobile/ai/config", { method: "DELETE" });
      setConfig(next);
      setBaseUrl("");
      setModel("");
      setApiKey("");
      setPreview(null);
      setResult(null);
      setMessage("模型配置和 Android Keystore 中的 API Key 已清除。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function createPreview() {
    setLoading(true);
    setError("");
    setMessage("");
    try {
      const next = await api<Preview>("/mobile/ai/review/preview", {
        method: "POST",
        body: JSON.stringify({ include_positions: includePositions, focus: focus.trim() }),
      });
      setPreview(next);
      setMessage("预览由手机本地生成，没有联网，也没有调用模型。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function runReview() {
    setLoading(true);
    setError("");
    setMessage("");
    setResult(null);
    try {
      const next = await api<AiResult>("/mobile/ai/review", {
        method: "POST",
        body: JSON.stringify({ include_positions: includePositions, focus: focus.trim() }),
      });
      setResult(next);
      let archived = "";
      if (archive) {
        try {
          const note = reviewNote(next);
          await api("/mobile/documents", { method: "POST", body: JSON.stringify({ kind: "note", ...note, symbol: null, status: "active", source: "ai_review" }) });
          archived = "已自动存到「研究记录」。";
        } catch (e) { archived = `自动归档没成功:${e instanceof Error ? e.message : String(e)}`; }
      }
      setMessage(`已发送 ${next.sent_characters.toLocaleString("zh-CN")} 个提示字符到你配置的模型 API。${archived}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  const dirty = baseUrl.trim() !== (config?.base_url ?? "") || model.trim() !== (config?.model ?? "") || Boolean(apiKey.trim()) || vision !== Boolean(config?.vision);

  return (
    <section className="ai-review">
      <div className="ai-review-title">
        <div>
          <span><BrainCircuit size={15} /> {settingsOnly ? "模型设置" : "AI 交易复盘"}</span>
          <h1>{settingsOnly ? "配置模型 API" : "先预览，再主动发送"}</h1>
          <p>{settingsOnly ? "配置 Base URL、模型名称和设备安全存储的 API Key。" : "默认只发送聚合统计；原始成交明细和交易备注不会发送。"}</p>
        </div>
        <button onClick={() => void loadConfig()} disabled={loading} aria-label="刷新模型配置">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && <div className="ai-review-error">{error}</div>}
      {message && <div className="ai-review-message">{message}</div>}

      <section className="ai-review-card">
        <div className="ai-review-heading">
          <div><KeyRound size={16} /><strong>OpenAI-compatible API</strong></div>
          <span className={config?.configured ? "ready" : ""}>{config?.configured ? "已配置" : "未配置"}</span>
        </div>
        <label>
          <span>常用厂商(选一个自动填地址和模型)</span>
          <WebSelect aria-label="常用厂商" value={presetOf(baseUrl)?.id ?? "custom"} onChange={(event) => applyPreset(event.target.value)}>
            <option value="custom">自定义 / 其它 OpenAI 兼容接口</option>
            {AI_PRESETS.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </WebSelect>
        </label>
        <label>
          <span>API Base URL</span>
          <input
            inputMode="url"
            value={baseUrl}
            onChange={(event) => setBaseUrl(event.target.value)}
            placeholder="https://your-provider.example/v1"
          />
        </label>
        <label>
          <span>模型名称</span>
          <input value={model} onChange={(event) => setModel(event.target.value)} placeholder="model-name" />
        </label>
        <label>
          <span>API Key</span>
          <input
            type="password"
            autoComplete="off"
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
            placeholder={config?.key_present ? "已安全保存；留空保持不变" : "可选；无鉴权的本地兼容接口可留空"}
          />
        </label>
        <label className="ai-review-check">
          <input type="checkbox" checked={vision} onChange={(event) => setVision(event.target.checked)} />
          <span>
            <strong>支持图片识别(多模态)</strong>
            <small>打开后,自选 / 持仓的「从截图识别」才会把你选的截图发给这个模型。同一厂商的不同模型差别很大,是不是多模态以厂商文档为准;不确定就点下面的「测试图片识别」。</small>
          </span>
        </label>
        <div className="ai-review-config-meta">
          <ShieldCheck size={14} />
          <span>
            API Key 使用 Android Keystore 设备密钥进行 AES-256-GCM 加密；SQLite 只保存 Base URL 与模型名称，接口永不回显 Key。
          </span>
        </div>
        <div className="ai-review-actions">
          <button className="primary" onClick={() => void saveConfig()} disabled={loading || !baseUrl.trim() || !model.trim() || (!dirty && Boolean(config))}>
            <Save size={15} /> 保存配置
          </button>
          <button onClick={() => void probeVision()} disabled={loading || !config?.configured || dirty} title={dirty ? "先保存配置,测试用的是已保存的配置" : undefined}>
            <Eye size={15} /> 测试图片识别
          </button>
          <button onClick={() => void clearConfig()} disabled={loading || !config?.configured}>
            <Trash2 size={15} /> 清除配置
          </button>
        </div>
      </section>

      {!settingsOnly && <>
      <section className="ai-review-card">
        <div className="ai-review-heading">
          <div><ShieldCheck size={16} /><strong>发送范围</strong></div>
          <span>默认最小化</span>
        </div>
        <label className="ai-review-check">
          <input type="checkbox" checked={archive} onChange={(event) => { setArchive(event.target.checked); try { window.localStorage.setItem(ARCHIVE_KEY, event.target.checked ? "1" : "0"); } catch { /* 可选 */ } }} />
          <span>
            <strong>复盘结果自动存到「研究记录」</strong>
            <small>跑完后自动存一份笔记(分类「AI 复盘」,带模型、发送范围和时间),在本机,可以改、可以删。</small>
          </span>
        </label>
        <div className="ai-review-scope">
          <div>
            <strong>默认发送</strong>
            <span>整体表现、最近 10 笔统计、按计划/计划外、打法统计、风险汇总。</span>
          </div>
          <div>
            <strong>始终排除</strong>
            <span>原始成交 fills、交易备注 note、API Key。</span>
          </div>
        </div>
        <label className="ai-review-check">
          <input
            type="checkbox"
            checked={includePositions}
            onChange={(event) => {
              setIncludePositions(event.target.checked);
              setPreview(null);
            }}
          />
          <span>
            <strong>额外包含未平仓明细</strong>
            <small>开启后会发送股票代码/名称、剩余股数、估算成本、计划止损/目标和风险金额。</small>
          </span>
        </label>
        <label>
          <span>额外关注点（可选）</span>
          <textarea
            maxLength={1000}
            value={focus}
            onChange={(event) => {
              setFocus(event.target.value);
              setPreview(null);
            }}
            placeholder="例如：重点分析计划外交易为何明显拖累收益，以及下一阶段怎样记录得更可验证。"
          />
        </label>
        <div className="ai-review-actions">
          <button onClick={() => void createPreview()} disabled={loading}>
            <Eye size={15} /> 预览发送数据
          </button>
          <button className="primary" onClick={() => void runReview()} disabled={loading || !config?.configured}>
            {loading ? <RefreshCw size={15} className="spin" /> : <Send size={15} />} 发送汇总数据并复盘
          </button>
        </div>
      </section>

      {preview && (
        <section className="ai-review-card">
          <div className="ai-review-heading">
            <div><Eye size={16} /><strong>发送预览</strong></div>
            <span>{preview.prompt_characters.toLocaleString("zh-CN")} 字符 · 未联网</span>
          </div>
          <div className="ai-review-preview-meta">
            <span>发送字段：{preview.sent_fields.join(" / ")}</span>
            <span>排除：{preview.excluded.join(" / ")}</span>
          </div>
          <pre>{JSON.stringify(preview.payload, null, 2)}</pre>
        </section>
      )}

      {result && (
        <section className="ai-review-card response">
          <div className="ai-review-heading">
            <div><BrainCircuit size={16} /><strong>AI 复盘结果</strong></div>
            <span>{result.model}</span>
          </div>
          <div className="ai-review-response">{result.response}</div>
          <p>仅用于复盘历史交易行为，不构成投资建议。</p>
        </section>
      )}
      </>}
    </section>
  );
}
