import { Layers3, Radar } from "lucide-react";
import { useState } from "react";

import { RotationPanel } from "./RotationPanel";
import { SectorPanel } from "./SectorPanel";
import "./discovery.css";

type DiscoveryView = "rotation" | "sector";

export function DiscoveryWorkspace() {
  const [view, setView] = useState<DiscoveryView>("rotation");

  return (
    <section className="discovery-workspace">
      <div className="discovery-tabs" role="tablist" aria-label="发现">
        <button
          type="button"
          role="tab"
          aria-selected={view === "rotation"}
          className={view === "rotation" ? "active" : ""}
          onClick={() => setView("rotation")}
        >
          <Layers3 size={14} /> 题材轮动
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "sector"}
          className={view === "sector" ? "active" : ""}
          onClick={() => setView("sector")}
        >
          <Radar size={14} /> 产业研究
        </button>
      </div>

      {view === "rotation" && <RotationPanel />}
      {view === "sector" && <SectorPanel />}
    </section>
  );
}
