import { BrainCircuit, ChartNoAxesCombined } from "lucide-react";
import { useState } from "react";

import { AiChatPanel } from "./AiChatPanel";
import { AiReviewPanel } from "./AiReviewPanel";
import "./ai-workspace.css";

type AiView = "chat" | "review";

export function AiWorkspace() {
  const [view, setView] = useState<AiView>("chat");

  return (
    <section className="ai-workspace">
      <div className="ai-workspace-tabs" role="tablist" aria-label="AI 功能">
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

      {view === "chat" ? <AiChatPanel /> : <AiReviewPanel />}
    </section>
  );
}
