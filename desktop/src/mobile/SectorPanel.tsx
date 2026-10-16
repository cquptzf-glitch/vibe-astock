import { useMemo, useState } from "react";
import { Check, Copy, RefreshCw, Send } from "lucide-react";

import sectorsData from "../verticals/finance/data/sectors.json";
import { sectorSteps, type SectorBrief } from "../verticals/finance/lib/sectorPrompts";
import "./discovery.css";

interface AiResult {
  response?: string;
  message?: string;
  error?: string;
}

async function postAi(prompt: string): Promise<string> {
  const response = await fetch("/api/mobile/ai/chat", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ message: prompt }),
  });
  const body = await response.json().catch(() => null) as AiResult | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body?.response?.trim() || "分析已写入 AI 对话历史。";
}

export function SectorPanel() {
  const sectors = sectorsData.sectors as SectorBrief[];
  const [activeKey, setActiveKey] = useState(sectors[0]?.key ?? "");
  const [copied, setCopied] = useState<number | null>(null);
  const [running, setRunning] = useState<number | null>(null);
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState("");
  const sector = sectors.find((item) => item.key === activeKey) ?? sectors[0];
  const steps = useMemo(() => sector ? sectorSteps(sector) : [], [sector]);

  async function copyStep(index: number, prompt: string) {
    setError("");
    try {
      await navigator.clipboard.writeText(prompt);
      setCopied(index);
      window.setTimeout(() => setCopied((current) => current === index ? null : current), 1500);
    } catch {
      setError("系统剪贴板不可用，可展开提示词后手动复制。 ");
    }
  }

  async function sendStep(index: number, prompt: string) {
    if (running !== null) return;
    setRunning(index);
    setAnswer("");
    setError("");
    try {
      setAnswer(await postAi(prompt));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(null);
    }
  }

  if (!sector) return <div className="mobile-empty">没有产业研究配置。</div>;

  return (
    <section className="mobile-section discovery-section">
      <div className="mobile-section-title">
        <div>
          <h1>产业研究</h1>
          <p>与 PC 共用同一份板块定义和分步研究方法；需要结论时由你配置的模型即时生成。</p>
        </div>
        <span>{sectors.length} 个</span>
      </div>

      <div className="mobile-sector-grid">
        {sectors.map((item) => (
          <button
            key={item.key}
            type="button"
            className={item.key === sector.key ? "active" : ""}
            onClick={() => {
              setActiveKey(item.key);
              setAnswer("");
              setError("");
            }}
          >
            {item.label}
          </button>
        ))}
      </div>

      <article className="mobile-quote-card discovery-sector-card">
        <div className="mobile-symbol-line"><strong>{sector.label}</strong></div>
        <p className="mobile-sector-tagline">{sector.tagline}</p>
        {sector.nodes.length > 0 && (
          <div className="mobile-chips discovery-node-chips">
            {sector.nodes.map((node) => <span key={node}>{node}</span>)}
          </div>
        )}
      </article>

      <div className="mobile-sector-steps">
        {steps.map((step, index) => (
          <article className="mobile-quote-card discovery-step-card" key={`${sector.key}-${step.n}`}>
            <div className="mobile-sector-step-head">
              <div><span>第 {step.n} 步</span><strong>{step.title}</strong></div>
              <div className="mobile-sector-step-actions">
                <button type="button" onClick={() => void copyStep(index, step.prompt)}>
                  {copied === index ? <Check size={14} /> : <Copy size={14} />}
                  {copied === index ? "已复制" : "复制"}
                </button>
                <button type="button" onClick={() => void sendStep(index, step.prompt)} disabled={running !== null}>
                  {running === index ? <RefreshCw className="spin" size={14} /> : <Send size={14} />}
                  {running === index ? "分析中" : "发给 AI"}
                </button>
              </div>
            </div>
            <details className="mobile-sector-prompt">
              <summary>查看提示词全文</summary>
              <pre>{step.prompt}</pre>
            </details>
          </article>
        ))}
      </div>

      {error && <div className="mobile-error discovery-message">{error}</div>}
      {answer && (
        <article className="mobile-quote-card discovery-answer-card">
          <div className="mobile-card-label">本次 AI 返回</div>
          <pre className="mobile-sector-answer">{answer}</pre>
        </article>
      )}
    </section>
  );
}
