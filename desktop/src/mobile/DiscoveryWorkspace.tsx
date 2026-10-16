import { CalendarDays, Layers3, Radar, ScanSearch } from "lucide-react";
import { useState } from "react";

import { CalendarPanel } from "./CalendarPanel";
import { CandidatePanel } from "./CandidatePanel";
import { RotationPanel } from "./RotationPanel";
import { SectorPanel } from "./SectorPanel";
import "./discovery.css";

type DiscoveryView = "rotation" | "sector" | "candidate" | "calendar";

export function DiscoveryWorkspace({ onOpenSymbol }: { onOpenSymbol?: (symbol: string) => void }) {
  const [view, setView] = useState<DiscoveryView>("rotation");

  return (
    <section className="discovery-workspace">
      <div className="discovery-tabs discovery-main-tabs" role="tablist" aria-label="发现">
        <button
          type="button"
          role="tab"
          aria-selected={view === "rotation"}
          className={view === "rotation" ? "active" : ""}
          onClick={() => setView("rotation")}
        >
          <Layers3 size={14} /> 轮动
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "sector"}
          className={view === "sector" ? "active" : ""}
          onClick={() => setView("sector")}
        >
          <Radar size={14} /> 产业
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "candidate"}
          className={view === "candidate" ? "active" : ""}
          onClick={() => setView("candidate")}
        >
          <ScanSearch size={14} /> 候选
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "calendar"}
          className={view === "calendar" ? "active" : ""}
          onClick={() => setView("calendar")}
        >
          <CalendarDays size={14} /> 日历
        </button>
      </div>

      {view === "rotation" && <RotationPanel />}
      {view === "sector" && <SectorPanel />}
      {view === "candidate" && <CandidatePanel onOpenSymbol={onOpenSymbol ?? (() => undefined)} />}
      {view === "calendar" && <CalendarPanel />}
    </section>
  );
}
