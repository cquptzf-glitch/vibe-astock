import { useState } from "react";
import { Activity, ChevronRight, Eye, Home, LayoutGrid, Search, UserRound, X } from "lucide-react";
import type { MobilePage } from "./mobileRoutes";
import { WORKSPACE_GROUPS, WORKSPACE_SECTIONS, sectionEntries, workspaceGroup, workspaceMatches, type WorkspaceGroup } from "./workspace";
import "./workspace.css";

export function FunctionDirectory({ onNavigate, group, initialGroup, onSettings }: { onNavigate: (page: MobilePage) => void; group?: WorkspaceGroup; initialGroup?: WorkspaceGroup; onSettings?: (id:string) => void }) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WorkspaceGroup | undefined>(initialGroup);
  const matches = workspaceMatches(query, group ?? filter).filter((entry) => entry.page !== "functions");
  return <section className="function-directory" aria-label={group === "my" ? "我的工作台" : "全部功能目录"}>
    <div className="workspace-title"><div><h1>{group === "my" ? "我的" : "全部功能"}</h1><p>{group === "my" ? "应用设置、日程与通知" : "按工作分类，快速找到需要的工具"}</p></div><span aria-live="polite">{group === "my" ? "本机偏好" : `${matches.length} 项`}</span></div>
    <div className="directory-search-row"><label className="workspace-search"><Search size={17} /><input type="search" aria-label="搜索功能" placeholder="搜索功能、关键词" value={query} onChange={(e) => setQuery(e.target.value)} /></label>{query && <button type="button" className="directory-clear" aria-label="清除功能搜索" onClick={() => setQuery("")}><X size={17} /></button>}</div>
    {!group && <div className="directory-filters" role="group" aria-label="功能分类">
      <button type="button" aria-pressed={!filter} onClick={() => setFilter(undefined)}>全部</button>
      {WORKSPACE_GROUPS.filter((item) => item.id !== "overview").map((item) => <button type="button" key={item.id} aria-pressed={filter === item.id} onClick={() => setFilter(item.id)}>{item.title}</button>)}
    </div>}
    {group === "my" && onSettings && !query && <section className="directory-group"><div className="directory-heading"><h2>应用与数据</h2></div><div className="directory-grid">
      {([["settings-appearance","外观与显示","主题、文字大小和信息密度"],["mobile-model-settings","模型接入","模型连接与 API 设置"],["settings-notifications","通知与提醒","系统权限与通知测试"],["settings-backup","备份与恢复","导出、校验和恢复本地业务数据"]] as const).map(([id,title,detail]) => <button type="button" key={id} onClick={() => onSettings(id)}><span><strong>{title}</strong><small>{detail}</small></span><ChevronRight size={16}/></button>)}
    </div></section>}
    {WORKSPACE_SECTIONS.map((section) => {
      if (group === "my" && onSettings && !query && section.id === "app") return null;
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

/** A single contextual row replaces stacked section and page tab bars. */
export function GroupTabs({ page, onNavigate, onBack, children }: { page: MobilePage; onNavigate: (page: MobilePage) => void; onBack?: () => void; children?: import("react").ReactNode }) {
  const group = workspaceGroup(page);
  const section = WORKSPACE_GROUPS.find(item => item.id === group)!;
  const entry = workspaceMatches("").find(item => item.page === page);
  const root = ["market", "reviewHome", "research", "my"].includes(page);
  return <nav className="group-tabs" aria-label="页面导航">
    <div className="mobile-page-heading">
      {!root && <button type="button" className="page-back" onClick={onBack ?? (() => onNavigate(section.home))} aria-label="返回">‹ 返回</button>}
      <strong>{entry?.title ?? (root ? section.title : page === "stock" ? "个股详情" : page === "board" ? "板块详情" : "研究资料")}</strong>
      <div className="page-heading-tools">{group === "research" && !["research", "stock", "functions"].includes(page) && <button type="button" aria-label="搜索股票" onClick={() => onNavigate("research")}><Search size={18}/></button>}{children}{page !== "functions" && <button type="button" aria-label="全部功能" onClick={() => onNavigate("functions")}><LayoutGrid size={18} /></button>}</div>
    </div>
    {["watch", "hold", "monitor"].includes(page) && <div className="watch-view-tabs" role="tablist" aria-label="盯盘视图">
      {([['watch','自选'],['hold','持仓'],['monitor','异动']] as const).map(([target,label]) => <button key={target} type="button" role="tab" aria-selected={page === target} onClick={() => onNavigate(target)}>{label}</button>)}
    </div>}
  </nav>;
}

export function WorkspaceHub({ group, onNavigate }: { group: WorkspaceGroup; onNavigate: (page: MobilePage) => void }) {
  const [query, setQuery] = useState("");
  const sections = WORKSPACE_SECTIONS.filter(item => item.group === group && item.id !== "review-home");
  return <section className="workspace-hub" aria-label={group === "review" ? "复盘功能" : "研究工具与资料"}>
    <label className="workspace-search"><Search size={18}/><input type="search" aria-label={group === "review" ? "搜索复盘功能" : "搜索研究功能"} placeholder={group === "review" ? "搜索复盘功能" : "搜索研究功能"} value={query} onChange={event => setQuery(event.target.value)}/></label>
    {sections.map(section => {
      const entries = sectionEntries(section).filter(entry => workspaceMatches(query, group).some(match => match.page === entry.page) && entry.page !== "research");
      return entries.length > 0 && <details className="hub-section" key={section.id + Boolean(query)} open={query ? true : undefined}>
        <summary><span><strong>{section.title}</strong><small>{section.detail}</small></span><ChevronRight size={18}/></summary>
        <div className="hub-links">{entries.map(entry => <button type="button" key={entry.page} onClick={() => onNavigate(entry.page)}><span>{entry.title}</span><ChevronRight size={17}/></button>)}</div>
      </details>;
    })}
    {workspaceMatches(query,group).length === 0 && <p role="status">未找到功能，请更换关键词。</p>}
  </section>;
}

const GROUP_ICONS = { overview: Home, watch: Eye, review: Activity, research: Search, my: UserRound };
export function WorkspaceNav({ page, onNavigate, unreadMessages = 0, activeGroup, onSelectGroup }: { page: MobilePage; onNavigate: (page: MobilePage) => void; unreadMessages?: number; activeGroup?: WorkspaceGroup; onSelectGroup?: (group: WorkspaceGroup) => void }) {
  const active = activeGroup ?? workspaceGroup(page);
  return <nav className="mobile-nav" aria-label="主导航">{WORKSPACE_GROUPS.map((section) => {
    const Icon = GROUP_ICONS[section.id];
    return <button key={section.id} type="button" className={active === section.id ? "active" : ""} aria-current={active === section.id ? "page" : undefined} onClick={() => onSelectGroup ? onSelectGroup(section.id) : onNavigate(section.home)}><Icon size={19} /><span>{section.title}</span>{section.id === "my" && unreadMessages > 0 && <i className="nav-badge" aria-label={`${unreadMessages} 条未读消息`}>{unreadMessages > 9 ? "9+" : unreadMessages}</i>}</button>;
  })}
  </nav>;
}
