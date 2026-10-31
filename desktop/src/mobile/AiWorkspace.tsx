import { usePageActive } from "./PageActivity.ts";
import { hasQueuedAiPrompt } from "./aiPageContext";
import { BrainCircuit, ChartNoAxesCombined } from "lucide-react";
import { useEffect, useState } from "react";

import { AiChatPanel } from "./AiChatPanel";
import { AiReviewPanel } from "./AiReviewPanel";
import { ResearchWorkflowPanel } from "./ResearchWorkflowPanel";
import "./ai-workspace.css";

type AiView = "chat" | "review" | "research";

export function AiWorkspace() {
  const active = usePageActive();
  const [view, setView] = useState<AiView>("chat");

  useEffect(() => { if (active && hasQueuedAiPrompt()) setView("chat"); }, [active]);

  return (
    <section className="ai-workspace">
      <div className="ai-workspace-tabs" role="tablist" aria-label="AI 功能">
        <button role="tab" aria-selected={view === "research"} className={view === "research" ? "active" : ""} onClick={() => setView("research")}><BrainCircuit size={14} /> 研究 / 辩论 / 复盘</button>
        <button
          role="tab"
          aria-selected={view === "chat"}
          className={view === "chat" ? "active" : ""}
          onClick={() => setView("chat")}
        >
          <BrainCircuit size={14} /> AI 对话
        </button>
        <button
          role="tab"
          aria-selected={view === "review"}
          className={view === "review" ? "active" : ""}
          onClick={() => setView("review")}
        >
          <ChartNoAxesCombined size={14} /> 交易复盘
        </button>
      </div>

      {view === "chat" ? <AiChatPanel /> : view === "research" ? <ResearchWorkflowPanel /> : <AiReviewPanel />}
    </section>
  );
}
