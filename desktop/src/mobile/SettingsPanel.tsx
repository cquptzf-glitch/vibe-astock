import {
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

export function SettingsPanel() {
  const [data, setData] = useState<DataSummary | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

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
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  return (
    <section className="settings-panel">
      <div className="settings-title-row">
        <div>
          <span><Smartphone size={15} /> Android 设置</span>
          <h1>应用 · 本地数据 · 隐私</h1>
          <p>查看当前移动端核心、SQLite 数据和模型凭据状态。</p>
        </div>
        <button onClick={() => void refresh()} disabled={loading} aria-label="刷新设置状态">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && <div className="settings-error">{error}</div>}
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
              <KeyRound size={17} />
              <span>AI 配置</span>
              <strong>{data.ai.configured ? "已配置" : "未配置"}</strong>
              <small>{data.ai.key_present ? "已保存凭据" : "无本地凭据"}</small>
            </div>
          </div>

          <section className="settings-block">
            <div className="settings-heading"><Database size={16} /><strong>本地数据概览</strong></div>
            <div className="settings-list">
              <div><span>自选股</span><b>{data.counts.watchlist} 只</b></div>
              <div><span>交易日志</span><b>{data.counts.journal_entries} 条</b></div>
              <div><span>成交明细</span><b>{data.counts.journal_fills} 笔</b></div>
              <div><span>未平仓持仓</span><b>{data.counts.open_positions} 笔</b></div>
              <div><span>SQLite 主文件</span><b>{bytes(data.storage.database_bytes)}</b></div>
              <div><span>WAL / SHM</span><b>{bytes(data.storage.wal_bytes + data.storage.shm_bytes)}</b></div>
            </div>
          </section>

          <section className="settings-block">
            <div className="settings-heading"><ShieldCheck size={16} /><strong>数据与隐私</strong></div>
            <div className="settings-notes">
              <p>• 自选、交易日志、持仓汇总和模型配置均存放在 Android 应用私有数据目录。</p>
              <p>• 持仓由成交日志实时推导，不保存第二套持仓数据库。</p>
              <p>• AI 复盘默认只发送聚合统计；原始成交明细、交易备注和 API Key 不发送。</p>
              <p>• API Key 当前仍保存在应用私有 SQLite 且接口不回显；生产版仍需迁移 Android Keystore。</p>
            </div>
          </section>

          <section className="settings-block compact">
            <div className="settings-heading"><KeyRound size={16} /><strong>模型设置</strong></div>
            <p className="settings-tip">模型 Base URL、模型名称和 API Key 在上方“AI 复盘”页配置。更换 Base URL 时，旧 Key 会自动清除，除非同时输入新的 Key。</p>
          </section>
        </>
      )}
    </section>
  );
}
