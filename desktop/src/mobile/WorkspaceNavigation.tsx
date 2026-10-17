import { useEffect, useRef, useState } from "react";
import { Activity, ChevronRight, Eye, Home, LayoutGrid, Search, Settings2, UserRound } from "lucide-react";
import type { MobilePage } from "./mobileRoutes";
import { WORKSPACE_GROUPS, groupEntries, workspaceGroup, workspaceMatches, type WorkspaceGroup } from "./workspace";
import "./workspace.css";

export function FunctionDirectory({ onNavigate, group }: { onNavigate: (page: MobilePage) => void; group?: WorkspaceGroup }) {
  const [query, setQuery] = useState("");
  const matches = workspaceMatches(query, group).filter((entry) => entry.page !== "functions");
  return <section className="function-directory" aria-label={group === "my" ? "我的工作台" : "全部功能目录"}>
    <div className="workspace-title"><div><h1>{group === "my" ? "我的" : "全部功能"}</h1><p>{group === "my" ? "事件日历、提醒、研究资料与应用设置" : "按盯盘、复盘、研究和个人资料组织日常工作"}</p></div><span>{matches.length} 项</span></div>
    <label className="workspace-search"><Search size={17} /><input aria-label="搜索功能" placeholder="搜索功能，如 首板、晋级、公告、备份" value={query} onChange={(e) => setQuery(e.target.value)} /></label>
    {WORKSPACE_GROUPS.filter((section) => !group || group === section.id).map((section) => {
      const entries = matches.filter((entry) => entry.group === section.id);
      return entries.length > 0 && <section className="directory-group" key={section.id}>
        <div className="directory-heading"><h2>{section.title}</h2><span>{section.detail}</span></div>
        <div className="directory-grid">{entries.map((entry) => <button type="button" key={entry.page} onClick={() => onNavigate(entry.page)}>
          <span><strong>{entry.title}</strong><small>{entry.detail}</small></span><ChevronRight size={16} />
        </button>)}</div>
      </section>;
    })}
    {matches.length === 0 && <div className="mobile-empty">未找到匹配功能，请尝试“财务”“封板”“提醒”等词。</div>}
  </section>;
}

/** 栏目内的横向子页签:一屏可见同组全部功能,当前页自动滚到可见区域。 */
export function GroupTabs({ page, onNavigate }: { page: MobilePage; onNavigate: (page: MobilePage) => void }) {
  const group = workspaceGroup(page);
  const entries = groupEntries(group);
  const active = useRef<HTMLButtonElement>(null);
  useEffect(() => { active.current?.scrollIntoView?.({ block: "nearest", inline: "center" }); }, [page]);
  if (group === "overview" || page === "my" || page === "settings" || page === "functions" || !entries.some((entry) => entry.page === page)) return null;
  const section = WORKSPACE_GROUPS.find((item) => item.id === group)!;
  return <nav className="group-tabs" aria-label={section.title + "栏目"}>
    <div role="tablist" aria-label={section.title}>{entries.map((entry) =>
      <button type="button" role="tab" key={entry.page} ref={entry.page === page ? active : undefined} aria-selected={entry.page === page}
        className={entry.page === page ? "active" : ""} onClick={() => onNavigate(entry.page)}>{entry.title}</button>)}
    </div>
    <button type="button" className="group-tabs-all" aria-label="全部功能" onClick={() => onNavigate("functions")}><LayoutGrid size={16} /></button>
  </nav>;
}

const GROUP_ICONS = { overview: Home, watch: Eye, review: Activity, research: Search, my: UserRound };
export function WorkspaceNav({ page, onNavigate }: { page: MobilePage; onNavigate: (page: MobilePage) => void }) {
  const inSettings = page === "settings";
  const active = page === "functions" || inSettings ? null : workspaceGroup(page);
  return <nav className="mobile-nav" aria-label="主导航">{WORKSPACE_GROUPS.map((section) => {
    const Icon = GROUP_ICONS[section.id];
    return <button key={section.id} type="button" className={active === section.id ? "active" : ""} aria-current={active === section.id ? "page" : undefined} onClick={() => onNavigate(section.home)}><Icon size={19} /><span>{section.title}</span></button>;
  })}
    <button type="button" className={inSettings ? "active" : ""} aria-current={inSettings ? "page" : undefined} onClick={() => onNavigate("settings")}><Settings2 size={19} /><span>设置</span></button>
  </nav>;
}
