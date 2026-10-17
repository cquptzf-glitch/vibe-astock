/**
 * 常用模型厂商预设(地址与默认模型名来自仓库 providers/*.json,2026-10-07 核实过的官方信息)。
 * 选一个只是把 Base URL 和模型名填进表单,不保存;要不要保存、API Key 填什么都由用户决定。
 * 「能不能看图」不在预设里写死:同一厂商的不同模型差别很大,请以厂商文档为准,并用「测试图片识别」实测。
 */
export interface AiPreset { id: string; name: string; base_url: string; model: string }

export const AI_PRESETS: AiPreset[] = [
  { id: "deepseek", name: "DeepSeek(官方 API)", base_url: "https://api.deepseek.com", model: "deepseek-flash" },
  { id: "glm", name: "智谱 GLM(官方开放平台)", base_url: "https://open.bigmodel.cn/api/paas/v4", model: "glm-5.3" },
  { id: "kimi", name: "Kimi(月之暗面官方 API)", base_url: "https://api.moonshot.cn/v1", model: "kimi-k3" },
  { id: "mimo", name: "小米 MiMo(官方 API)", base_url: "https://api.xiaomimimo.com/v1", model: "mimo-v2.6-flash" },
  { id: "openai-api", name: "OpenAI(官方 API)", base_url: "https://api.openai.com/v1", model: "gpt-5.4" },
  { id: "qwen", name: "通义千问(阿里云百炼)", base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen3.8-max" },
  { id: "selfhosted", name: "自托管(Ollama / vLLM / LM Studio 等)", base_url: "http://localhost:11434/v1", model: "" },
];

/** 当前填的地址对应哪个预设(没有就是自定义) */
export function presetOf(baseUrl: string): AiPreset | undefined {
  const norm = baseUrl.trim().replace(/\/+$/, "");
  return AI_PRESETS.find((p) => p.base_url === norm);
}

/** 测试图片识别用的图:几行大字(代码 + 名称 + 数量 + 成本),模型读对 600519 才算能读图。浏览器里才画得出来 */
export const PROBE_CODE = "600519";
export function renderProbeImage(): { data: string; mime: string } | null {
  if (typeof document === "undefined") return null;
  const canvas = document.createElement("canvas");
  canvas.width = 480;
  canvas.height = 160;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#111111";
  ctx.font = "bold 34px sans-serif";
  ctx.fillText(`贵州茅台  ${PROBE_CODE}`, 24, 62);
  ctx.font = "26px sans-serif";
  ctx.fillText("持仓 100 股   成本 1450.00", 24, 116);
  const url = canvas.toDataURL("image/png");
  const comma = url.indexOf(",");
  return comma > 0 ? { data: url.slice(comma + 1), mime: "image/png" } : null;
}
