import { useEffect, useRef, useState } from "react";
import { Activity, ChevronRight, Eye, Home, LayoutGrid, Search, UserRound, X } from "lucide-react";
import type { MobilePage } from "./mobileRoutes";
import { WORKSPACE_GROUPS, WORKSPACE_SECTIONS, sectionEntries, workspaceGroup, workspaceMatches, workspaceSection, type WorkspaceGroup } from "./workspace";
import "./workspace.css";

export function FunctionDirectory({ onNavigate, group, initialGroup }: { onNavigate: (page: MobilePage) => void; group?: WorkspaceGroup; initialGroup?: WorkspaceGroup }) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WorkspaceGroup | undefined>(initialGroup);
  const matches = workspaceMatches(query, group ?? filter).filter((entry) => entry.page !== "functions");
  return <section className="function-directory" aria-label={group === "my" ? "我的工作台" : "全部功能目录"}>
    <div className="workspace-title"><div><h1>{group === "my" ? "我的" : "全部功能"}</h1><p>{group === "my" ? "应用设置、日程提醒与研究资料" : "按工作分类，快速找到需要的工具"}</p></div><span aria-live="polite">{matches.length} 项</span></div>
    <div className="directory-search-row"><label className="workspace-search"><Search size={17} /><input type="search" aria-label="搜索功能" placeholder="搜索功能、关键词" value={query} onChange={(e) => setQuery(e.target.value)} /></label>{query && <button type="button" className="directory-clear" aria-label="清除功能搜索" onClick={() => setQuery("")}><X size={17} /></button>}</div>
    {!group && <div className="directory-filters" role="group" aria-label="功能分类">
      <button type="button" aria-pressed={!filter} onClick={() => setFilter(undefined)}>全部</button>
      {WORKSPACE_GROUPS.filter((item) => item.id !== "overview").map((item) => <button type="button" key={item.id} aria-pressed={filter === item.id} onClick={() => setFilter(item.id)}>{item.title}</button>)}
    </div>}
    {WORKSPACE_SECTIONS.map((section) => {
      const entries = sectionEntries(section).filter((entry) => matches.some((match) => match.page === entry.page));
      return entries.length > 0 && <section className="directory-group" key={section.id}>
        <div className="directory-heading"><div><h2>{section.title}</h2><p>{section.detail}</p></div><span>{entries.length} 项</span></div>
        <div className="directory-grid">{entries.map((entry) => <button type="button" key={entry.page} onClick={() => onNavigate(entry.page)}>
          <span><strong>{entry.title}</strong><small>{entry.detail}</small></span><ChevronRight size={16} />
        </button>)}</div>
      </section>;
    })}
    {matches.length === 0 && <div className="mobile-empty">未找到匹配功能，请尝试“财务”“封板”“提醒”等词。</div>}
  </section>;
}

/** 先选工作分组，再显示本组页签；目录保留所有功能和搜索。 */
export function GroupTabs({ page, onNavigate }: { page: MobilePage; onNavigate: (page: MobilePage) => void }) {
  const group = workspaceGroup(page);
  const sections = WORKSPACE_SECTIONS.filter((section) => section.group === group);
  const selected = workspaceSection(page);
  const entries = selected ? sectionEntries(selected) : [];
  const active = useRef<HTMLButtonElement>(null);
  useEffect(() => { active.current?.scrollIntoView?.({ block: "nearest", inline: "center" }); }, [page]);
  if (group === "overview" || page === "my" || page === "settings" || page === "functions" || !entries.some((entry) => entry.page === page)) return null;
  const section = WORKSPACE_GROUPS.find((item) => item.id === group)!;
  return <nav className="group-tabs" aria-label={section.title + "栏目"}>
    {sections.length > 1 && <div className="workspace-sections" role="group" aria-label={section.title + "功能分组"}>
      {sections.map((item) => <button type="button" key={item.id} aria-pressed={item.id === selected?.id} onClick={() => onNavigate(item.pages[0]!)}>{item.title}</button>)}
    </div>}
    <div className="group-tabs-row">
    <div role="tablist" aria-label={section.title}>{entries.map((entry) =>
      <button type="button" role="tab" key={entry.page} ref={entry.page === page ? active : undefined} aria-selected={entry.page === page}
        aria-label={entry.title} className={entry.page === page ? "active" : ""} onClick={() => onNavigate(entry.page)}>{entry.title.length > 5 ? entry.short : entry.title}</button>)}
    </div>
    <button type="button" className="group-tabs-all" aria-label="全部功能" onClick={() => onNavigate("functions")}><LayoutGrid size={16} /><span>目录</span></button>
    </div>
  </nav>;
}

const GROUP_ICONS = { overview: Home, watch: Eye, review: Activity, research: Search, my: UserRound };
export function WorkspaceNav({ page, onNavigate, unreadMessages = 0 }: { page: MobilePage; onNavigate: (page: MobilePage) => void; unreadMessages?: number }) {
  const active = page === "functions" ? null : workspaceGroup(page);
  return <nav className="mobile-nav" aria-label="主导航">{WORKSPACE_GROUPS.map((section) => {
    const Icon = GROUP_ICONS[section.id];
    return <button key={section.id} type="button" className={active === section.id ? "active" : ""} aria-current={active === section.id ? "page" : undefined} onClick={() => onNavigate(section.home)}><Icon size={19} /><span>{section.title}</span>{section.id === "my" && unreadMessages > 0 && <i className="nav-badge" aria-label={`${unreadMessages} 条未读消息`}>{unreadMessages > 9 ? "9+" : unreadMessages}</i>}</button>;
  })}
  </nav>;
}
