import { BarChart3, NotebookPen } from "lucide-react";
import { useState } from "react";

import { JournalPanelV2 } from "./JournalPanelV2";
import { JournalReviewPanel } from "./JournalReviewPanel";
import "./journal-workspace.css";

type ViewMode = "records" | "review";

export function JournalWorkspace({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [view, setView] = useState<ViewMode>("records");

  return (
    <section className="journal-workspace">
      <div className="journal-workspace-tabs" role="tablist" aria-label="交易日志视图">
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
      </div>

      {view === "records"
        ? <JournalPanelV2 onOpenSymbol={onOpenSymbol} />
        : <JournalReviewPanel onOpenSymbol={onOpenSymbol} />}
    </section>
  );
}
