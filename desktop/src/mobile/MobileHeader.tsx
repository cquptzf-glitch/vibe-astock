import { Activity, ArrowUpRight, BookOpenCheck, Flame, LayoutGrid, Layers3, ListOrdered, NotebookPen, ScanSearch, Wifi } from "lucide-react";
import type { MobilePage } from "./mobileRoutes";

const SHORTCUTS = [
  { page: "monitor", title: "实时动态", detail: "自选持仓", icon: Wifi },
  { page: "research", title: "个股深挖", detail: "财务·公告", icon: ScanSearch },
  { page: "records", title: "交易日志", detail: "记一笔", icon: NotebookPen },
  { page: "daily", title: "每日复盘", detail: "明日验证", icon: BookOpenCheck },
  { page: "emotion", title: "今日情绪", detail: "晋级·赚钱效应", icon: Activity },
  { page: "ladder", title: "连板天梯", detail: "高标·断层", icon: ListOrdered },
  { page: "limitUp", title: "涨停全表", detail: "涨停·炸板·跌停", icon: Flame },
  { page: "rotation", title: "板块轮动", detail: "资金·成分", icon: Layers3 },
] as const;
export function HomeShortcuts({ onNavigate }: { onNavigate: (page: MobilePage) => void }) {
  return <section className="home-watch-panel" aria-label="工作台快捷功能">
    <div className="home-section-heading"><div><h2>常用功能</h2><span>盯盘 → 复盘 → 研究 → 记录</span></div>
      <button type="button" className="home-section-link" onClick={() => onNavigate("functions")}><LayoutGrid size={15} />全部功能</button></div>
    <div className="home-shortcuts">
      {SHORTCUTS.map(({ page, title, detail, icon: Icon }) => <button type="button" key={page} onClick={() => onNavigate(page)}><Icon size={19} /><strong>{title}</strong><small>{detail}</small></button>)}
    </div>
  </section>;
}
export function SectionLink({ label, onClick }: { label: string; onClick: () => void }) {
  return <button type="button" className="home-section-link" onClick={onClick}>{label}<ArrowUpRight size={15} /></button>;
}
