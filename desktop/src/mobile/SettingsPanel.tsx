import {
  BellRing,
  Database,
  Download,
  HardDrive,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  Smartphone,
  Upload,
} from "lucide-react";
import { useEffect, useState } from "react";

import "./settings.css";

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
  const tauri = window.__TAURI__ as unknown as { notification?: NotificationApi } | undefined;
  return tauri?.notification ?? null;
}

function backupFileApi(): { dialog: DialogApi; fs: FsApi } | null {
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

export function SettingsPanel() {
  const [data, setData] = useState<DataSummary | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [notificationSupported, setNotificationSupported] = useState(false);
  const [notificationPermission, setNotificationPermission] = useState<NotificationPermission>("unknown");
  const [backupBusy, setBackupBusy] = useState<"" | "export" | "import">("");

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
      const backup = await api<BackupEnvelope>("/mobile/backup/export");
      const date = new Date().toISOString().slice(0, 10);
      const path = await plugin.dialog.save({
        defaultPath: `vibe-astock-backup-${date}.json`,
        filters: [{ name: "Vibe AStock JSON Backup", extensions: ["json"] }],
      });
      if (!path) return;
      await plugin.fs.writeTextFile(path, JSON.stringify(backup, null, 2));
      setMessage("本地业务数据备份已导出。API Key、Keystore 密钥和模型配置未写入备份文件。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBackupBusy("");
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
      const text = await plugin.fs.readTextFile(path);
      if (text.length > 32 * 1024 * 1024) throw new Error("备份文件过大，最大允许 32 MiB。");
      const backup = JSON.parse(text) as BackupEnvelope;
      const preview = await api<BackupPreview>("/mobile/backup/validate", {
        method: "POST",
        body: JSON.stringify({ backup }),
      });
      const confirmed = window.confirm(
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

          <section className="settings-block">
            <div className="settings-heading"><BellRing size={16} /><strong>通知</strong><span>{permissionLabel(notificationPermission, notificationSupported)}</span></div>
            <div className="settings-notification-body">
              <p>通知能力用于后续价格、持仓风险、盘中监控和复盘完成提醒。权限由 Android 系统管理。</p>
              <div className="settings-actions">
                <button onClick={() => void requestNotification()} disabled={!notificationSupported || notificationPermission === "granted"}>申请通知权限</button>
                <button className="primary" onClick={() => void sendTestNotification()} disabled={!notificationSupported}>发送测试通知</button>
              </div>
            </div>
          </section>

          <section className="settings-block">
            <div className="settings-heading"><HardDrive size={16} /><strong>备份与恢复</strong><span>{fileSupported ? "系统文件选择器" : "不可用"}</span></div>
            <div className="settings-notification-body">
              <p>导出版本化 JSON 备份：自选、交易日志、成交明细、市场日复盘和 AI 聊天历史。API Key、Android Keystore 密钥及模型配置始终排除；恢复也不会修改当前设备的模型凭据。</p>
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
            <p className="settings-tip">模型 Base URL、模型名称和 API Key 在上方“AI”页配置。更换 Base URL 时，旧 Key 会自动清除，除非同时输入新的 Key。</p>
          </section>
        </>
      )}
    </section>
  );
}
