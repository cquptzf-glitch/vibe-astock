import { BarChart3, CalendarDays, Layers3, Radar, ScanSearch } from "lucide-react";

import { BacktestPanel } from "./BacktestPanel";
import { CalendarPanel } from "./CalendarPanel";
import { CandidatePanel } from "./CandidatePanel";
import { RotationPanel } from "./RotationPanel";
import { SectorPanel } from "./SectorPanel";
import "./discovery.css";

export type DiscoveryView = "rotation" | "sector" | "candidate" | "calendar" | "backtest";

export function DiscoveryWorkspace({ onOpenSymbol, view, onViewChange: setView }: {
  onOpenSymbol?: (symbol: string) => void;
  view: DiscoveryView;
  onViewChange: (view: DiscoveryView) => void;
}) {

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
        <button
          type="button"
          role="tab"
          aria-selected={view === "backtest"}
          className={view === "backtest" ? "active" : ""}
          onClick={() => setView("backtest")}
        >
          <BarChart3 size={14} /> 回测
        </button>
      </div>

      {view === "rotation" && <RotationPanel />}
      {view === "sector" && <SectorPanel />}
      {view === "candidate" && <CandidatePanel onOpenSymbol={onOpenSymbol ?? (() => undefined)} />}
      {view === "calendar" && <CalendarPanel />}
      {view === "backtest" && <BacktestPanel />}
    </section>
  );
}
