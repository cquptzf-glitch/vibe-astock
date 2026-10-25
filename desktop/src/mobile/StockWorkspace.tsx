import { useId, useState, type ReactNode } from "react";

const TABS = [
  { id: "quote", title: "行情图表" },
  { id: "company", title: "公司资料" },
  { id: "insights", title: "量价资金" },
] as const;
type StockTab = typeof TABS[number]["id"];

/** Load a section on first use, then keep its filters and chart position when switching. */
export function StockWorkspace({ quote, company, insights }: Record<StockTab, ReactNode>) {
  const [selected, setSelected] = useState<StockTab>("quote");
  const [visited, setVisited] = useState<StockTab[]>(["quote"]);
  const id = useId();
  const panels = { quote, company, insights };
  const select = (tab: StockTab) => { setSelected(tab); setVisited((current) => current.includes(tab) ? current : [...current, tab]); };
  return <div className="stock-workspace">
    <div className="stock-workspace-tabs" role="tablist" aria-label="个股内容">
      {TABS.map((tab) => <button type="button" role="tab" id={`${id}-${tab.id}-tab`} aria-controls={`${id}-${tab.id}`} aria-selected={selected === tab.id} tabIndex={selected === tab.id ? 0 : -1} key={tab.id}
        onClick={() => select(tab.id)} onKeyDown={(event) => {
          const index = TABS.findIndex((item) => item.id === tab.id);
          const next = event.key === "ArrowRight" ? (index + 1) % TABS.length : event.key === "ArrowLeft" ? (index + TABS.length - 1) % TABS.length : event.key === "Home" ? 0 : event.key === "End" ? TABS.length - 1 : -1;
          const target = TABS[next];
          if (!target) return;
          event.preventDefault(); select(target.id);
          document.getElementById(`${id}-${target.id}-tab`)?.focus();
        }}>{tab.title}</button>)}
    </div>
    {TABS.map((tab) => <div role="tabpanel" id={`${id}-${tab.id}`} aria-labelledby={`${id}-${tab.id}-tab`} hidden={selected !== tab.id} key={tab.id}>
      {visited.includes(tab.id) && panels[tab.id]}
    </div>)}
  </div>;
}
