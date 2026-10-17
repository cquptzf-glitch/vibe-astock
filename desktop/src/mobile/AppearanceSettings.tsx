import { Monitor, Moon, Sun } from "lucide-react";
import { WebSelect } from "./controls";
import { usePresentation, type Density, type Presentation, type RefreshSeconds, type ThemeMode } from "./presentation";

export function AppearanceControls({ value, onChange }: { value: Presentation; onChange: (patch: Partial<Pick<Presentation, "theme" | "density" | "refreshSeconds">>) => void }) {
  return <section className="settings-block appearance-block" aria-labelledby="appearance-title">
    <div className="settings-heading"><Sun size={16} /><strong id="appearance-title">外观与显示</strong><span>自动保存</span></div>
    <div className="appearance-body">
      <fieldset><legend>主题模式</legend><div className="appearance-options">
        {([["light", "白色模式", Sun], ["dark", "深色模式", Moon], ["system", "跟随系统", Monitor]] as const).map(([mode, label, Icon]) =>
          <button key={mode} type="button" aria-pressed={value.theme === mode} onClick={() => onChange({ theme: mode as ThemeMode })}><Icon size={18} /><span>{label}</span></button>)}
      </div></fieldset>
      <fieldset><legend>信息密度</legend><div className="appearance-options density-options">
        {([["compact", "紧凑", "更多行情同屏"], ["comfortable", "舒适", "更宽松的间距"]] as const).map(([density, label, detail]) =>
          <button key={density} type="button" aria-pressed={value.density === density} onClick={() => onChange({ density: density as Density })}><strong>{label}</strong><small>{detail}</small></button>)}
      </div></fieldset>
      <label className="appearance-refresh">前台行情刷新<WebSelect aria-label="前台行情刷新" value={value.refreshSeconds} onChange={(event) => onChange({ refreshSeconds: Number(event.target.value) as RefreshSeconds })}>
        <option value={30}>每 30 秒</option><option value={60}>每 60 秒</option><option value={120}>每 120 秒</option><option value={0}>仅手动刷新</option>
      </WebSelect></label>
      <p>页面切到后台时暂停刷新。主题、密度和刷新偏好保存在本机。</p>
    </div>
  </section>;
}
export function AppearanceSettings() {
  const value = usePresentation();
  return <AppearanceControls value={value} onChange={value.update} />;
}
