import { useEffect, useMemo, useState } from "react";
import { ArrowDownRight, ArrowUpRight, Layers3, RefreshCw, Repeat2 } from "lucide-react";

import "./rotation.css";

type BoardType = "concept" | "industry";
type Period = "today" | "five_day";
type Signal = "persistent" | "new_hot" | "cooling" | null;

interface RotationRow {
  rank: number;
  code: string;
  name: string;
  change_pct: number | null;
  main_net_yuan: number | null;
  main_pct: number | null;
  leader: string;
  up_count: number | null;
  down_count: number | null;
  rotation_signal: Signal;
}

interface RotationBrief {
  code: string;
  name: string;
  rank: number;
  change_pct: number | null;
  main_net_yuan: number | null;
}

interface RotationData {
  status: string;
  source: string;
  board_type: BoardType;
  fetched_at: string;
  summary: {
    persistent_count: number;
    new_hot_count: number;
    cooling_count: number;
    persistent: RotationBrief[];
    new_hot: RotationBrief[];
    cooling: RotationBrief[];
  };
  today: { period: "today"; total: number; rows: RotationRow[] };
  five_day: { period: "5d"; total: number; rows: RotationRow[] };
}

async function mobileApi<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, { headers: { Accept: "application/json" } });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) {
    throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  }
  return body as T;
}

function formatPct(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return `${value >= 0 ? "+" : ""}${value.toFixed(2)}%`;
}

function formatMoney(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  const sign = value > 0 ? "+" : "";
  if (Math.abs(value) >= 100_000_000) {
    return `${sign}${(value / 100_000_000).toLocaleString("zh-CN", { maximumFractionDigits: 2 })}亿`;
  }
  if (Math.abs(value) >= 10_000) {
    return `${sign}${(value / 10_000).toLocaleString("zh-CN", { maximumFractionDigits: 1 })}万`;
  }
  return `${sign}${value.toLocaleString("zh-CN", { maximumFractionDigits: 0 })}`;
}

function signalLabel(signal: Signal): string {
  if (signal === "persistent") return "持续强";
  if (signal === "new_hot") return "新走强";
  if (signal === "cooling") return "退潮观察";
  return "";
}

export function RotationPanel() {
  const [boardType, setBoardType] = useState<BoardType>("concept");
  const [period, setPeriod] = useState<Period>("today");
  const [cache, setCache] = useState<Partial<Record<BoardType, RotationData>>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const data = cache[boardType] ?? null;
  const rows = useMemo(() => {
    if (!data) return [];
    return period === "today" ? data.today.rows : data.five_day.rows;
  }, [data, period]);

  async function load(type: BoardType, force = false) {
    if (!force && cache[type]) return;
    setLoading(true);
    setError("");
    try {
      const result = await mobileApi<RotationData>(`/mobile/rotation?type=${type}&top=20`);
      setCache((current) => ({ ...current, [type]: result }));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (!cache[boardType]) {
      void load(boardType);
    }
  }, [boardType]);

  return (
    <section className="rotation-panel">
      <div className="rotation-title-row">
        <div>
          <div className="rotation-kicker"><Repeat2 size={15} /> 资金轮动</div>
          <h1>题材 · 板块轮动</h1>
          <p>对比今日与 5 日主力资金排名，识别持续强、新走强与退潮板块。</p>
        </div>
        <button className="rotation-refresh" onClick={() => void load(boardType, true)} disabled={loading} aria-label="刷新轮动数据">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="rotation-type-tabs" role="tablist" aria-label="板块类型">
        <button className={boardType === "concept" ? "active" : ""} onClick={() => setBoardType("concept")}>概念题材</button>
        <button className={boardType === "industry" ? "active" : ""} onClick={() => setBoardType("industry")}>行业板块</button>
      </div>

      {error && (
        <div className="rotation-error">
          <strong>轮动数据暂不可用</strong>
          <span>{error}</span>
          <button onClick={() => void load(boardType, true)}>重试</button>
        </div>
      )}

      {!error && !data && loading && (
        <div className="rotation-loading"><RefreshCw className="spin" size={20} /> 正在计算板块轮动…</div>
      )}

      {data && (
        <>
          <div className="rotation-summary">
            <div className="persistent"><span>持续强</span><strong>{data.summary.persistent_count}</strong><small>今日 + 5日均居前</small></div>
            <div className="new-hot"><span>新走强</span><strong>{data.summary.new_hot_count}</strong><small>今日新进入前列</small></div>
            <div className="cooling"><span>退潮</span><strong>{data.summary.cooling_count}</strong><small>5日强、今日掉队</small></div>
          </div>

          {data.summary.persistent.length > 0 && (
            <div className="rotation-focus">
              <span>持续强焦点</span>
              <div>
                {data.summary.persistent.slice(0, 5).map((item) => <b key={item.code}>{item.name}</b>)}
              </div>
            </div>
          )}

          <div className="rotation-period-tabs" role="tablist" aria-label="轮动周期">
            <button className={period === "today" ? "active" : ""} onClick={() => setPeriod("today")}>今日资金</button>
            <button className={period === "five_day" ? "active" : ""} onClick={() => setPeriod("five_day")}>5日资金</button>
          </div>

          <div className="rotation-list">
            {rows.map((row) => {
              const up = row.change_pct !== null && row.change_pct >= 0;
              const netUp = row.main_net_yuan !== null && row.main_net_yuan >= 0;
              return (
                <article className="rotation-row" key={`${period}-${row.code}`}>
                  <div className="rotation-rank">{row.rank}</div>
                  <div className="rotation-row-main">
                    <div className="rotation-name-line">
                      <strong>{row.name}</strong>
                      {row.rotation_signal && <span className={`signal ${row.rotation_signal}`}>{signalLabel(row.rotation_signal)}</span>}
                    </div>
                    <div className="rotation-meta">
                      <span>{row.code}</span>
                      {row.leader && <span>领涨 {row.leader}</span>}
                      {row.up_count !== null && row.down_count !== null && <span>{row.up_count}↑ / {row.down_count}↓</span>}
                    </div>
                  </div>
                  <div className="rotation-values">
                    <strong className={up ? "up" : "down"}>{formatPct(row.change_pct)}</strong>
                    <span className={netUp ? "net-up" : "net-down"}>
                      {netUp ? <ArrowUpRight size={12} /> : <ArrowDownRight size={12} />}
                      {formatMoney(row.main_net_yuan)}
                    </span>
                    <small>主力占比 {formatPct(row.main_pct)}</small>
                  </div>
                </article>
              );
            })}
          </div>

          <div className="rotation-source">
            <Layers3 size={13} /> 东财板块资金流 · {boardType === "concept" ? "概念" : "行业"} · Top {rows.length}
          </div>
        </>
      )}
    </section>
  );
}
