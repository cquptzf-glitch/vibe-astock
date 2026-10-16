import { Archive, BarChart3, BrainCircuit, CalendarCheck2, NotebookPen, Settings2 } from "lucide-react";
import { useState } from "react";

import { AiWorkspace } from "./AiWorkspace";
import { DailyReviewPanel } from "./DailyReviewPanel";
import { JournalPanelV2 } from "./JournalPanelV2";
import { JournalReviewPanel } from "./JournalReviewPanel";
import { LibraryPanel } from "./LibraryPanel";
import { SettingsPanel } from "./SettingsPanel";
import "./journal-workspace.css";
import "./library.css";

type ViewMode = "records" | "daily" | "review" | "ai" | "library" | "settings";

export function JournalWorkspace({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [view, setView] = useState<ViewMode>("records");

  return (
    <section className="journal-workspace">
      <div className="journal-workspace-tabs" role="tablist" aria-label="复盘与个人工作台">
        <button
          role="tab"
          aria-selected={view === "records"}
          className={view === "records" ? "active" : ""}
          onClick={() => setView("records")}
        >
          <NotebookPen size={14} /> 交易
        </button>
        <button
          role="tab"
          aria-selected={view === "daily"}
          className={view === "daily" ? "active" : ""}
          onClick={() => setView("daily")}
        >
          <CalendarCheck2 size={14} /> 日复
        </button>
        <button
          role="tab"
          aria-selected={view === "review"}
          className={view === "review" ? "active" : ""}
          onClick={() => setView("review")}
        >
          <BarChart3 size={14} /> 统计
        </button>
        <button
          role="tab"
          aria-selected={view === "ai"}
          className={view === "ai" ? "active" : ""}
          onClick={() => setView("ai")}
        >
          <BrainCircuit size={14} /> AI
        </button>
        <button
          role="tab"
          aria-selected={view === "library"}
          className={view === "library" ? "active" : ""}
          onClick={() => setView("library")}
        >
          <Archive size={14} /> 资料
        </button>
        <button
          role="tab"
          aria-selected={view === "settings"}
          className={view === "settings" ? "active" : ""}
          onClick={() => setView("settings")}
        >
          <Settings2 size={14} /> 设置
        </button>
      </div>

      {view === "records" && <JournalPanelV2 onOpenSymbol={onOpenSymbol} />}
      {view === "daily" && <DailyReviewPanel onOpenSymbol={onOpenSymbol} />}
      {view === "review" && <JournalReviewPanel onOpenSymbol={onOpenSymbol} />}
      {view === "ai" && <AiWorkspace />}
      {view === "library" && <LibraryPanel />}
      {view === "settings" && <SettingsPanel />}
    </section>
  );
}
