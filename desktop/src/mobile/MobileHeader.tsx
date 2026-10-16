import { ArrowUpRight, BrainCircuit, CalendarDays, ChartNoAxesCombined, Layers3, Newspaper, NotebookPen, Settings2, Sun, Moon, Wifi, LibraryBig } from "lucide-react";
import type { MobilePage } from "./mobileRoutes";

export function MobileHeader({ version, ready, theme, onToggleTheme, onOpenSettings }: {
  version: string; ready: boolean; theme: "light" | "dark"; onToggleTheme: () => void; onOpenSettings: () => void;
}) {
  return <header className="mobile-header">
    <div className="mobile-header-brand"><div className="mobile-brand">Vibe AStock</div>
      <div className="mobile-subtitle"><span className={ready ? "connection-dot ready" : "connection-dot"} /><span>{ready ? "本地工作台" : "连接中"} · v{version}</span></div>
    </div>
    <div className="mobile-header-actions">
      <button type="button" className="mobile-header-action" aria-label={theme === "dark" ? "切换白色模式" : "切换深色模式"} title={theme === "dark" ? "切换白色模式" : "切换深色模式"} onClick={onToggleTheme}>{theme === "dark" ? <Sun size={19} /> : <Moon size={19} />}</button>
      <button type="button" className="mobile-header-action settings-entry" aria-label="打开设置" onClick={onOpenSettings}><Settings2 size={18} /><span>设置</span></button>
    </div>
  </header>;
}
const SHORTCUTS = [
  { page: "monitor", title: "盯盘", detail: "自选动态", icon: Wifi },
  { page: "rotation", title: "板块", detail: "轮动成分", icon: Layers3 },
  { page: "news", title: "资讯", detail: "产业追踪", icon: Newspaper },
  { page: "calendar", title: "日历", detail: "事件验证", icon: CalendarDays },
  { page: "daily", title: "复盘", detail: "观察记录", icon: NotebookPen },
  { page: "ai", title: "AI 助手", detail: "研究对话", icon: BrainCircuit },
  { page: "backtest", title: "回测", detail: "历史核对", icon: ChartNoAxesCombined },
  { page: "library", title: "资料", detail: "笔记报告", icon: LibraryBig },
] as const;
export function HomeShortcuts({ onNavigate }: { onNavigate: (page: MobilePage) => void }) {
  return <section className="home-shortcuts" aria-label="工作台快捷功能">
    {SHORTCUTS.map(({ page, title, detail, icon: Icon }) => <button type="button" key={page} onClick={() => onNavigate(page)}><Icon size={19} /><strong>{title}</strong><small>{detail}</small></button>)}
  </section>;
}
export function SectionLink({ label, onClick }: { label: string; onClick: () => void }) {
  return <button type="button" className="home-section-link" onClick={onClick}>{label}<ArrowUpRight size={15} /></button>;
}
