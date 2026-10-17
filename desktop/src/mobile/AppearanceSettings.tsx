import { Eye, EyeOff, Monitor, Moon, Sun } from "lucide-react";
import { usePrivacyMode } from "./privacy";
import { WebSelect } from "./controls";
import { usePresentation, type Density, type Presentation, type RefreshSeconds, type ThemeMode } from "./presentation";

/** 隐私模式开关:金额、股数、自己写的价位遮成 ••••,输入框用密码样式,权益曲线不画;市场行情和百分比不遮 */
export function PrivacyToggle() {
  const privacy = usePrivacyMode();
  return <fieldset><legend>隐私模式</legend>
    <div className="appearance-options">
      <button type="button" aria-pressed={privacy.on} onClick={privacy.toggle}>{privacy.on ? <EyeOff size={18} /> : <Eye size={18} />}<span>{privacy.on ? "已开启:金额和股数已遮住" : "未开启"}</span></button>
    </div>
    <p>遮住持仓、成交、成本、盈亏、分红和账户规模的金额与股数(显示 ••••),价位输入框改成密码样式,权益曲线不画。市场行情、涨跌幅和情绪读数不遮。只管屏幕上看得见什么——个人交易数据本来就不会发给 AI。</p>
  </fieldset>;
}

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
      <PrivacyToggle />
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
