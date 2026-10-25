import { PageSections } from "./PageSections";
import {
  BellRing,
  Database,
  Download,
  HardDrive,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  Smartphone,
  Trash2,
  Upload,
} from "lucide-react";
import { useEffect, useState } from "react";
import { checkPassphrase, decryptBackup, encryptBackup, isEncryptedBackup } from "./backupCrypto";
import { askConfirm } from "./controls";

import "./settings.css";
import { AppearanceSettings } from "./AppearanceSettings";
import { AiReviewPanel } from "./AiReviewPanel";
import { OfflineSettings } from "./OfflineSettings";

type DataSummary = {
  source: string;
  generated_at: string;
  storage: {
    scope: string;
    database_bytes: number;
    wal_bytes: number;
    shm_bytes: number;
    total_bytes: number;
  };
  counts: {
    watchlist: number;
    journal_entries: number;
    journal_fills: number;
    daily_reviews: number;
    ai_messages: number;
    open_positions: number;
  };
  ai: {
    configured: boolean;
    key_present: boolean;
    storage: string;
    key_echoed: boolean;
  };
};

type Health = { ok: boolean; version: string };
type NotificationPermission = "granted" | "denied" | "default" | "unknown";
type NotificationApi = {
  isPermissionGranted(): Promise<boolean>;
  requestPermission(): Promise<"granted" | "denied" | "default">;
  sendNotification(options: { title: string; body?: string }): void;
};
type DialogFilter = { name: string; extensions: string[] };
type DialogApi = {
  open(options: { multiple: boolean; directory: boolean; filters?: DialogFilter[] }): Promise<string | string[] | null>;
  save(options: { defaultPath?: string; filters?: DialogFilter[] }): Promise<string | null>;
};
type FsApi = {
  readTextFile(path: string): Promise<string>;
  writeTextFile(path: string, data: string): Promise<void>;
};
type BackupEnvelope = Record<string, unknown>;
type BackupPreview = {
  valid: boolean;
  schema_version: number;
  source_version: string;
  created_at: string;
  counts: {
    watchlist: number;
    journal_entries: number;
    journal_fills: number;
    daily_reviews: number;
    ai_messages: number;
  };
  security: {
    credentials_included: boolean;
    ai_model_config_included: boolean;
  };
};
type BackupRestoreResult = {
  status: string;
  mode: string;
  credentials_imported: boolean;
  restart_required: boolean;
};

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
    },
  });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as T;
}

function bytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(2)} MiB`;
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${Math.round(value)} B`;
}

function notificationApi(): NotificationApi | null {
  if (typeof window === "undefined") return null;
  const tauri = window.__TAURI__ as unknown as { notification?: NotificationApi } | undefined;
  return tauri?.notification ?? null;
}

function backupFileApi(): { dialog: DialogApi; fs: FsApi } | null {
  if (typeof window === "undefined") return null;
  const tauri = window.__TAURI__ as unknown as { dialog?: DialogApi; fs?: FsApi } | undefined;
  if (!tauri?.dialog || !tauri.fs) return null;
  return { dialog: tauri.dialog, fs: tauri.fs };
}

function permissionLabel(permission: NotificationPermission, supported: boolean): string {
  if (!supported) return "不可用";
  if (permission === "granted") return "已授权";
  if (permission === "denied") return "已拒绝";
  if (permission === "default") return "未申请";
  return "检测中";
}

export function SettingsPanel({ onOpenAi, onOpenReminders }: { onOpenAi?: () => void; onOpenReminders?: () => void } = {}) {
  const [data, setData] = useState<DataSummary | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [notificationSupported, setNotificationSupported] = useState(false);
  const [notificationPermission, setNotificationPermission] = useState<NotificationPermission>("unknown");
  const [backupBusy, setBackupBusy] = useState<"" | "export" | "import">("");
  const [passphrase, setPassphrase] = useState("");
  const [clearing, setClearing] = useState(false);
  const [showModelSettings, setShowModelSettings] = useState(false);

  async function refreshNotification() {
    const plugin = notificationApi();
    setNotificationSupported(Boolean(plugin));
    if (!plugin) {
      setNotificationPermission("unknown");
      return;
    }
    try {
      setNotificationPermission(await plugin.isPermissionGranted() ? "granted" : "default");
    } catch {
      setNotificationPermission("unknown");
    }
  }

  async function refresh() {
    setLoading(true);
    setError("");
    try {
      const [summary, core] = await Promise.all([
        api<DataSummary>("/mobile/data/summary"),
        api<Health>("/health"),
      ]);
      setData(summary);
      setHealth(core);
      await refreshNotification();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  async function requestNotification() {
    const plugin = notificationApi();
    if (!plugin) {
      setError("当前运行环境没有加载 Android 通知插件。");
      return;
    }
    setError("");
    setMessage("");
    try {
      const permission = await plugin.requestPermission();
      setNotificationPermission(permission);
      setMessage(permission === "granted" ? "通知权限已开启。" : "通知权限未开启，可稍后在系统设置中修改。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function sendTestNotification() {
    const plugin = notificationApi();
    if (!plugin) {
      setError("当前运行环境没有加载 Android 通知插件。");
      return;
    }
    setError("");
    setMessage("");
    try {
      let granted = await plugin.isPermissionGranted();
      if (!granted) {
        const permission = await plugin.requestPermission();
        setNotificationPermission(permission);
        granted = permission === "granted";
      }
      if (!granted) {
        setMessage("未获得通知权限，没有发送测试通知。");
        return;
      }
      plugin.sendNotification({
        title: "Vibe AStock",
        body: "Android 原生通知已可用。后续价格、风险和复盘提醒会使用此通道。",
      });
      setNotificationPermission("granted");
      setMessage("测试通知已发送。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function exportBackup() {
    const plugin = backupFileApi();
    if (!plugin) {
      setError("当前运行环境没有加载系统文件选择与文件读写插件。");
      return;
    }
    setBackupBusy("export");
    setError("");
    setMessage("");
    try {
      const encrypt = passphrase.length > 0;
      if (encrypt) {
        const problem = checkPassphrase(passphrase);
        if (problem) throw new Error(problem);
      }
      const backup = await api<BackupEnvelope>("/mobile/backup/export");
      const date = new Date().toISOString().slice(0, 10);
      const path = await plugin.dialog.save({
        defaultPath: `vibe-astock-backup-${date}${encrypt ? ".enc" : ""}.json`,
        filters: [{ name: "Vibe AStock JSON Backup", extensions: ["json"] }],
      });
      if (!path) return;
      const plain = JSON.stringify(backup, null, 2);
      await plugin.fs.writeTextFile(path, encrypt ? await encryptBackup(plain, passphrase) : plain);
      setMessage(encrypt
        ? "已导出加密备份(口令加密,忘了口令就解不开)。API Key、Keystore 密钥和模型配置未写入备份文件。"
        : "本地业务数据备份已导出(未加密)。API Key、Keystore 密钥和模型配置未写入备份文件。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBackupBusy("");
    }
  }

  /** 统一清除全部本地数据:两次确认 → SQLite 全清(Core)+ Keystore 里的 API Key + 提醒调度(命令层)→ 清浏览器里的本机偏好 → 重新加载 */
  async function clearAllData() {
    if (!(await askConfirm("清除这台手机上 Vibe AStock 的全部本地数据?\n\n包括自选、交易日志与成交、分红、日复盘、AI 聊天与模型配置、资料库、日历、提醒、各类缓存,以及 Android Keystore 里的 API Key。\n\n建议先「导出备份」。"))) return;
    if (!(await askConfirm("最后确认:清除后无法恢复,除非你有备份文件。真的要清除吗?"))) return;
    setClearing(true);
    setError("");
    setMessage("");
    try {
      const out = await api<{ rows_removed: number; keystore_cleared?: boolean; keystore_error?: string; reminders_cancelled?: boolean }>("/mobile/data/clear", {
        method: "POST",
        body: JSON.stringify({ confirm: true, phrase: "清除全部本地数据" }),
      });
      try { window.localStorage.clear(); window.sessionStorage.clear(); } catch { /* 存储不可用就跳过 */ }
      if (out.keystore_cleared === false) {
        setError(`本地数据已清空(${out.rows_removed} 行),但 Keystore 里的 API Key 没清掉:${out.keystore_error ?? "未知原因"}。可以在「AI」页清除模型配置。`);
        setClearing(false);
        return;
      }
      window.location.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setClearing(false);
    }
  }

  async function importBackup() {
    const plugin = backupFileApi();
    if (!plugin) {
      setError("当前运行环境没有加载系统文件选择与文件读写插件。");
      return;
    }
    setBackupBusy("import");
    setError("");
    setMessage("");
    try {
      const selected = await plugin.dialog.open({
        multiple: false,
        directory: false,
        filters: [{ name: "Vibe AStock JSON Backup", extensions: ["json"] }],
      });
      if (!selected) return;
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      let text = await plugin.fs.readTextFile(path);
      if (text.length > 48 * 1024 * 1024) throw new Error("备份文件过大，最大允许 32 MiB。");
      if (isEncryptedBackup(text)) {
        if (!passphrase) throw new Error("这是加密备份:先在「备份口令」里填导出时用的口令,再点「从备份恢复」。");
        text = await decryptBackup(text, passphrase);
      }
      if (text.length > 32 * 1024 * 1024) throw new Error("备份文件过大，最大允许 32 MiB。");
      const backup = JSON.parse(text) as BackupEnvelope;
      const preview = await api<BackupPreview>("/mobile/backup/validate", {
        method: "POST",
        body: JSON.stringify({ backup }),
      });
      const confirmed = await askConfirm(
        `确认恢复这份备份？\n\n` +
        `自选 ${preview.counts.watchlist} 只\n` +
        `交易日志 ${preview.counts.journal_entries} 条 / 成交 ${preview.counts.journal_fills} 笔\n` +
        `日复盘 ${preview.counts.daily_reviews} 天\n` +
        `AI 聊天 ${preview.counts.ai_messages} 条\n\n` +
        `当前本地业务数据将被替换。设备上的模型配置和 Android Keystore API Key 保持不变。`,
      );
      if (!confirmed) return;
      await api<BackupRestoreResult>("/mobile/backup/import", {
        method: "POST",
        body: JSON.stringify({ backup, mode: "replace", confirm: true }),
      });
      await refresh();
      setMessage("备份恢复完成。当前设备的模型配置与 Keystore API Key 未被修改。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBackupBusy("");
    }
  }

  const fileSupported = Boolean(backupFileApi());

  return (
    <section className="settings-panel">
      <div className="settings-title-row">
        <div>
          <span><Smartphone size={15} /> Android 设置</span>
          <h1>应用 · 本地数据 · 隐私</h1>
          <p>查看移动端核心、SQLite、模型凭据、备份与系统通知状态。</p>
        </div>
        <button onClick={() => void refresh()} disabled={loading} aria-label="刷新设置状态">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <PageSections label="设置分区" items={[{ id: "settings-appearance", label: "外观" }, { id: "mobile-model-settings", label: "模型", open: () => setShowModelSettings(true) }, { id: "settings-notifications", label: "通知" }, { id: "settings-backup", label: "备份" }]} />
      <div id="settings-appearance"><AppearanceSettings /></div>
      <div className="settings-quick-links">
        <button type="button" onClick={() => setShowModelSettings((value) => !value)} aria-expanded={showModelSettings} aria-controls="mobile-model-settings"><KeyRound size={16} /> 模型 API 设置</button>
        {onOpenReminders && <button type="button" onClick={onOpenReminders}><BellRing size={16} /> 提醒规则设置</button>}
      </div>
      <div id="mobile-model-settings" hidden={!showModelSettings}>{showModelSettings && <><AiReviewPanel settingsOnly />{onOpenAi && <button className="discovery-action" type="button" onClick={onOpenAi}>进入 AI 助手</button>}</>}</div>
      {error && <div className="settings-error">{error}</div>}
      {message && <div className="settings-message">{message}</div>}
      {!data && loading && <div className="settings-loading"><RefreshCw size={18} className="spin" /> 正在读取本地状态…</div>}

      {data && (
        <>
          <div className="settings-cards">
            <div>
              <Smartphone size={17} />
              <span>Rust Core</span>
              <strong>{health?.version ?? "—"}</strong>
              <small>{health?.ok ? "运行正常" : "状态未知"}</small>
            </div>
            <div>
              <HardDrive size={17} />
              <span>本地数据</span>
              <strong>{bytes(data.storage.total_bytes)}</strong>
              <small>SQLite + WAL</small>
            </div>
            <div>
              <Database size={17} />
              <span>交易记录</span>
              <strong>{data.counts.journal_entries}</strong>
              <small>{data.counts.journal_fills} 笔成交明细</small>
            </div>
            <div>
              <BellRing size={17} />
              <span>系统通知</span>
              <strong>{permissionLabel(notificationPermission, notificationSupported)}</strong>
              <small>Android 原生通知</small>
            </div>
          </div>

          <section className="settings-block" id="settings-notifications">
            <div className="settings-heading"><BellRing size={16} /><strong>通知</strong><span>{permissionLabel(notificationPermission, notificationSupported)}</span></div>
            <div className="settings-notification-body">
              <p>通知能力用于后续价格、持仓风险、盘中监控和复盘完成提醒。权限由 Android 系统管理。</p>
              <div className="settings-actions">
                <button onClick={() => void requestNotification()} disabled={!notificationSupported || notificationPermission === "granted"}>申请通知权限</button>
                <button className="primary" onClick={() => void sendTestNotification()} disabled={!notificationSupported}>发送测试通知</button>
              </div>
            </div>
          </section>

          <section className="settings-block" id="settings-backup">
            <div className="settings-heading"><HardDrive size={16} /><strong>备份与恢复</strong><span>{fileSupported ? "系统文件选择器" : "不可用"}</span></div>
            <div className="settings-notification-body">
              <p>导出版本化 JSON 备份：自选、交易日志、成交明细、市场日复盘和 AI 聊天历史。API Key、Android Keystore 密钥及模型配置始终排除；恢复也不会修改当前设备的模型凭据。</p>
              <label className="settings-passphrase">
                <span>备份口令(选填:填了就加密导出;恢复加密备份时必填)</span>
                <input type="password" autoComplete="off" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} aria-label="备份口令" placeholder="至少 8 个字符;不保存、不上传,忘了就解不开" />
              </label>
              <div className="settings-actions">
                <button onClick={() => void exportBackup()} disabled={!fileSupported || Boolean(backupBusy)}>
                  <Download size={14} /> {backupBusy === "export" ? "正在导出…" : "导出备份"}
                </button>
                <button className="primary" onClick={() => void importBackup()} disabled={!fileSupported || Boolean(backupBusy)}>
                  <Upload size={14} /> {backupBusy === "import" ? "正在恢复…" : "从备份恢复"}
                </button>
              </div>
            </div>
          </section>

          <section className="settings-block">
            <OfflineSettings />
            <div className="settings-heading"><Database size={16} /><strong>本地数据概览</strong></div>
            <div className="settings-list">
              <div><span>自选股</span><b>{data.counts.watchlist} 只</b></div>
              <div><span>交易日志</span><b>{data.counts.journal_entries} 条</b></div>
              <div><span>成交明细</span><b>{data.counts.journal_fills} 笔</b></div>
              <div><span>市场日复盘</span><b>{data.counts.daily_reviews} 天</b></div>
              <div><span>AI 聊天</span><b>{data.counts.ai_messages} 条</b></div>
              <div><span>未平仓持仓</span><b>{data.counts.open_positions} 笔</b></div>
              <div><span>SQLite 主文件</span><b>{bytes(data.storage.database_bytes)}</b></div>
              <div><span>WAL / SHM</span><b>{bytes(data.storage.wal_bytes + data.storage.shm_bytes)}</b></div>
            </div>
          </section>

          <section className="settings-block">
            <div className="settings-heading"><ShieldCheck size={16} /><strong>数据与隐私</strong></div>
            <div className="settings-notes">
              <p>• 自选、交易日志、日复盘、持仓汇总和模型元数据均存放在 Android 应用私有数据目录。</p>
              <p>• 持仓由成交日志实时推导，不保存第二套持仓数据库。</p>
              <p>• AI 复盘默认只发送聚合统计；原始成交明细、交易备注和 API Key 不发送。</p>
              <p>• API Key 使用 Android Keystore 设备密钥进行 AES-256-GCM 加密；SQLite 只保存模型 Base URL 与模型名称，接口不回显 Key。</p>
              <p>• 业务数据备份不包含模型配置与任何凭据；恢复时当前设备 Keystore 内容保持不变。</p>
            </div>
          </section>

          <section className="settings-block compact">
            <div className="settings-heading"><KeyRound size={16} /><strong>模型设置</strong></div>
            <p className="settings-tip">点击本页“模型 API 设置”直接展开配置表单，填写 Base URL、模型名称和 API Key。更换 Base URL 时，旧 Key 会自动清除，除非同时输入新的 Key。</p>
          </section>
          <section className="settings-block settings-danger">
            <div className="settings-heading"><Trash2 size={16} /><strong>清除全部本地数据</strong><span>不可恢复</span></div>
            <div className="settings-notification-body">
              <p>一次清空这台手机上的全部本地数据:自选、交易日志与成交、分红、日复盘、AI 聊天与模型配置、资料库、日历、提醒、行情缓存,以及 Android Keystore 里的 API Key,并取消已排的提醒。要先留一份,请先导出备份。</p>
              <div className="settings-actions">
                <button className="danger" onClick={() => void clearAllData()} disabled={clearing}><Trash2 size={14} /> {clearing ? "正在清除…" : "清除全部本地数据"}</button>
              </div>
            </div>
          </section>

        </>
      )}
    </section>
  );
}
