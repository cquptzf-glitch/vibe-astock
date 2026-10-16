import {
  BellRing,
  Database,
  HardDrive,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  Smartphone,
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

async function api<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, { headers: { Accept: "application/json" } });
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

  return (
    <section className="settings-panel">
      <div className="settings-title-row">
        <div>
          <span><Smartphone size={15} /> Android 设置</span>
          <h1>应用 · 本地数据 · 隐私</h1>
          <p>查看移动端核心、SQLite、模型凭据和系统通知状态。</p>
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
            <div className="settings-heading"><Database size={16} /><strong>本地数据概览</strong></div>
            <div className="settings-list">
              <div><span>自选股</span><b>{data.counts.watchlist} 只</b></div>
              <div><span>交易日志</span><b>{data.counts.journal_entries} 条</b></div>
              <div><span>成交明细</span><b>{data.counts.journal_fills} 笔</b></div>
              <div><span>市场日复盘</span><b>{data.counts.daily_reviews} 天</b></div>
              <div><span>未平仓持仓</span><b>{data.counts.open_positions} 笔</b></div>
              <div><span>SQLite 主文件</span><b>{bytes(data.storage.database_bytes)}</b></div>
              <div><span>WAL / SHM</span><b>{bytes(data.storage.wal_bytes + data.storage.shm_bytes)}</b></div>
            </div>
          </section>

          <section className="settings-block">
            <div className="settings-heading"><ShieldCheck size={16} /><strong>数据与隐私</strong></div>
            <div className="settings-notes">
              <p>• 自选、交易日志、日复盘、持仓汇总和模型配置均存放在 Android 应用私有数据目录。</p>
              <p>• 持仓由成交日志实时推导，不保存第二套持仓数据库。</p>
              <p>• AI 复盘默认只发送聚合统计；原始成交明细、交易备注和 API Key 不发送。</p>
              <p>• API Key 当前仍保存在应用私有 SQLite 且接口不回显；生产版仍需迁移 Android Keystore。</p>
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
