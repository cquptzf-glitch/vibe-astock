import { BarChart3, RefreshCw, ShieldAlert, Target, TrendingDown, TrendingUp } from "lucide-react";
import { useEffect, useState } from "react";

import "./journal-review.css";

type Stats = {
  label: string | null;
  trades: number;
  scored: number;
  wins: number;
  losses: number;
  flats: number;
  win_rate: number | null;
  avg_pct: number | null;
  avg_win_pct: number | null;
  avg_loss_pct: number | null;
  payoff_ratio: number | null;
  best_pct: number | null;
  worst_pct: number | null;
  money_scored: number;
  net_pnl: number | null;
  profit_factor: number | null;
};

type RiskPosition = {
  id: number;
  symbol: string;
  name: string;
  date: string;
  open_shares: number;
  avg_cost: number | null;
  open_cost: number | null;
  planned_stop: number | null;
  planned_target: number | null;
  risk_amount: number | null;
  risk_pct: number | null;
};

type ReviewView = {
  available: boolean;
  source: string;
  generated_at: string;
  performance: Stats;
  recent_10: Stats;
  discipline: {
    planned: Stats;
    unplanned: Stats;
    unknown: number;
  };
  playbooks: Stats[];
  risk: {
    open_positions: number;
    open_cost_total: number | null;
    stop_covered: number;
    stop_coverage: number | null;
    planned_risk_total: number | null;
    without_stop: number;
    note: string;
  };
  positions: RiskPosition[];
  notes: string[];
};

async function loadReview(): Promise<ReviewView> {
  const response = await fetch("/api/mobile/journal/review", { headers: { Accept: "application/json" } });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & ReviewView) | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as ReviewView;
}

function pct(value: number | null, ratio = false): string {
  if (value === null || !Number.isFinite(value)) return "—";
  const actual = ratio ? value * 100 : value;
  return `${actual > 0 ? "+" : ""}${actual.toFixed(1)}%`;
}

function amount(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function ratio(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : value.toFixed(2);
}

function PerformanceLine({ stats }: { stats: Stats }) {
  return (
    <div className="journal-review-line">
      <div><strong>{stats.label ?? "全部记录"}</strong><span>{stats.scored} 个有效样本</span></div>
      <div><span>胜率</span><b>{pct(stats.win_rate, true)}</b></div>
      <div><span>平均</span><b>{pct(stats.avg_pct)}</b></div>
      <div><span>盈亏比</span><b>{ratio(stats.payoff_ratio)}</b></div>
    </div>
  );
}

export function JournalReviewPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [data, setData] = useState<ReviewView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    setLoading(true);
    setError("");
    try {
      setData(await loadReview());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  return (
    <section className="journal-review">
      <div className="journal-review-title">
        <div>
          <span><BarChart3 size={15} /> 本地复盘统计</span>
          <h1>表现 · 纪律 · 风险</h1>
          <p>只读取手机 SQLite，不联网；统计用于复盘历史记录。</p>
        </div>
        <button onClick={() => void refresh()} disabled={loading} aria-label="刷新复盘统计">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && <div className="journal-review-error">{error}</div>}
      {!data && loading && <div className="journal-review-empty"><RefreshCw className="spin" size={18} /> 正在统计…</div>}

      {data && (
        <>
          <div className="journal-review-cards">
            <div><span>有效样本</span><strong>{data.performance.scored}</strong><small>笔</small></div>
            <div><span>胜率</span><strong>{pct(data.performance.win_rate, true)}</strong><small>持平剔除</small></div>
            <div><span>盈亏比</span><strong>{ratio(data.performance.payoff_ratio)}</strong><small>均盈 / 均亏</small></div>
            <div><span>盈利因子</span><strong>{ratio(data.performance.profit_factor)}</strong><small>金额口径</small></div>
          </div>

          <div className="journal-review-balance">
            <div className="win"><TrendingUp size={17} /><span>平均盈利</span><strong>{pct(data.performance.avg_win_pct)}</strong></div>
            <div className="loss"><TrendingDown size={17} /><span>平均亏损</span><strong>{pct(data.performance.avg_loss_pct)}</strong></div>
            <div><BarChart3 size={17} /><span>最近 10 笔</span><strong>{pct(data.recent_10.avg_pct)}</strong></div>
            <div><Target size={17} /><span>金额净盈亏</span><strong>{amount(data.performance.net_pnl)} 元</strong></div>
          </div>

          <section className="journal-review-block">
            <div className="journal-review-heading">
              <div><ShieldAlert size={17} /><strong>未平仓计划风险</strong></div>
              <span>{data.risk.open_positions} 笔</span>
            </div>
            <div className="journal-review-risk-grid">
              <div><span>估算持仓成本</span><strong>{amount(data.risk.open_cost_total)} 元</strong></div>
              <div><span>计划止损风险</span><strong>{amount(data.risk.planned_risk_total)} 元</strong></div>
              <div><span>止损覆盖</span><strong>{pct(data.risk.stop_coverage, true)}</strong></div>
              <div><span>未写止损</span><strong className={data.risk.without_stop > 0 ? "warn" : ""}>{data.risk.without_stop} 笔</strong></div>
            </div>
            <p className="journal-review-note">{data.risk.note}</p>

            {data.positions.length > 0 && (
              <div className="journal-review-positions">
                {data.positions.map((position) => (
                  <button key={position.id} onClick={() => onOpenSymbol(position.symbol)}>
                    <div><strong>{position.name || position.symbol}</strong><span>{position.symbol} · {position.open_shares.toLocaleString("zh-CN")} 股</span></div>
                    <div><span>均价 {amount(position.avg_cost)}</span><span>止损 {position.planned_stop ?? "未写"}</span></div>
                    <div className={position.risk_amount === null ? "unknown" : ""}>
                      <b>{position.risk_amount === null ? "风险未界定" : `${amount(position.risk_amount)} 元`}</b>
                      <small>{position.risk_pct === null ? "" : pct(position.risk_pct)}</small>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="journal-review-block">
            <div className="journal-review-heading"><div><Target size={17} /><strong>执行纪律</strong></div><span>未标注 {data.discipline.unknown} 笔</span></div>
            <PerformanceLine stats={data.discipline.planned} />
            <PerformanceLine stats={data.discipline.unplanned} />
          </section>

          <section className="journal-review-block">
            <div className="journal-review-heading"><div><BarChart3 size={17} /><strong>打法表现</strong></div><span>{data.playbooks.length} 类</span></div>
            {data.playbooks.length === 0
              ? <div className="journal-review-empty">已有记录还没有填写打法。</div>
              : data.playbooks.map((stats) => <PerformanceLine key={stats.label ?? "unknown"} stats={stats} />)}
          </section>

          <section className="journal-review-block compact">
            <div className="journal-review-heading"><strong>样本说明</strong></div>
            <div className="journal-review-notes">{data.notes.map((note) => <p key={note}>• {note}</p>)}</div>
            <p className="journal-review-disclaimer">这些统计只描述你已经记录的历史交易，不构成投资建议。</p>
          </section>
        </>
      )}
    </section>
  );
}
