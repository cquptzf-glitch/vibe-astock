import { BarChart3, BrainCircuit, NotebookPen, Settings2 } from "lucide-react";
import { useState } from "react";

import { AiReviewPanel } from "./AiReviewPanel";
import { JournalPanelV2 } from "./JournalPanelV2";
import { JournalReviewPanel } from "./JournalReviewPanel";
import { SettingsPanel } from "./SettingsPanel";
import "./journal-workspace.css";

type ViewMode = "records" | "review" | "ai" | "settings";

export function JournalWorkspace({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [view, setView] = useState<ViewMode>("records");

  return (
    <section className="journal-workspace">
      <div className="journal-workspace-tabs" role="tablist" aria-label="个人工作台">
        <button
          role="tab"
          aria-selected={view === "records"}
          className={view === "records" ? "active" : ""}
          onClick={() => setView("records")}
        >
          <NotebookPen size={15} /> 记录
        </button>
        <button
          role="tab"
          aria-selected={view === "review"}
          className={view === "review" ? "active" : ""}
          onClick={() => setView("review")}
        >
          <BarChart3 size={15} /> 复盘
        </button>
        <button
          role="tab"
          aria-selected={view === "ai"}
          className={view === "ai" ? "active" : ""}
          onClick={() => setView("ai")}
        >
          <BrainCircuit size={15} /> AI
        </button>
        <button
          role="tab"
          aria-selected={view === "settings"}
          className={view === "settings" ? "active" : ""}
          onClick={() => setView("settings")}
        >
          <Settings2 size={15} /> 设置
        </button>
      </div>

      {view === "records" && <JournalPanelV2 onOpenSymbol={onOpenSymbol} />}
      {view === "review" && <JournalReviewPanel onOpenSymbol={onOpenSymbol} />}
      {view === "ai" && <AiReviewPanel />}
      {view === "settings" && <SettingsPanel />}
    </section>
  );
}
