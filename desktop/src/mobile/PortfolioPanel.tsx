import {
  BriefcaseBusiness,
  ChevronRight,
  RefreshCw,
  Share2,
  ShieldAlert,
  TrendingDown,
  TrendingUp,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { shareText } from "./nativeShare";
import { PortfolioAnalysis } from "./PortfolioAnalysis";
import { PortfolioIncome } from "./PortfolioIncome";
import "./portfolio.css";

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
  generated_at: string;
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
};

type QuoteEnvelope = {
  evidence: { field: string; value: number | string | null; unit: string }[];
};

type FetchResult = { envelope: QuoteEnvelope };

type PositionView = RiskPosition & {
  price: number | null;
  change_pct: number | null;
  market_value: number | null;
  unrealized_pnl: number | null;
  unrealized_pct: number | null;
};

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  return body as T;
}

function evidenceNumber(envelope: QuoteEnvelope, field: string): number | null {
  const raw = envelope.evidence.find((item) => item.field === field)?.value;
  if (raw === null || raw === undefined || raw === "") return null;
  const value = typeof raw === "number" ? raw : Number(raw);
  return Number.isFinite(value) ? value : null;
}

function money(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function pct(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return `${value > 0 ? "+" : ""}${value.toFixed(2)}%`;
}

export function PortfolioPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [review, setReview] = useState<ReviewView | null>(null);
  const [positions, setPositions] = useState<PositionView[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    setLoading(true);
    setError("");
    try {
      const next = await api<ReviewView>("/mobile/journal/review");
      const enriched = await Promise.all(next.positions.map(async (position): Promise<PositionView> => {
        try {
          const result = await api<FetchResult>("/fetch", {
            method: "POST",
            body: JSON.stringify({ endpoint: "tx_quote", symbol: position.symbol, refresh: true }),
          });
          const price = evidenceNumber(result.envelope, "price");
          const changePct = evidenceNumber(result.envelope, "change_pct");
          const avgCost = position.avg_cost;
          const marketValue = price === null ? null : price * position.open_shares;
          const unrealized = price === null || avgCost === null
            ? null
            : (price - avgCost) * position.open_shares;
          const unrealizedPct = price === null || avgCost === null || avgCost <= 0
            ? null
            : (price / avgCost - 1) * 100;
          return {
            ...position,
            price,
            change_pct: changePct,
            market_value: marketValue,
            unrealized_pnl: unrealized,
            unrealized_pct: unrealizedPct,
          };
        } catch {
          return {
            ...position,
            price: null,
            change_pct: null,
            market_value: null,
            unrealized_pnl: null,
            unrealized_pct: null,
          };
        }
      }));
      setReview(next);
      setPositions(enriched);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  const summary = useMemo(() => {
    const quoted = positions.filter((position) => position.market_value !== null);
    const pnlKnown = positions.filter((position) => position.unrealized_pnl !== null);
    return {
      quoted: quoted.length,
      marketValue: quoted.length > 0
        ? quoted.reduce((sum, position) => sum + (position.market_value ?? 0), 0)
        : null,
      unrealized: pnlKnown.length > 0
        ? pnlKnown.reduce((sum, position) => sum + (position.unrealized_pnl ?? 0), 0)
        : null,
    };
  }, [positions]);

  async function sharePortfolio() {
    if (!review || positions.length === 0) return;
    const rows = positions.map((position) => {
      const label = position.name || position.symbol;
      return `${label} ${position.symbol} · ${position.open_shares.toLocaleString("zh-CN")}股 · 成本 ${money(position.avg_cost)} · 最新 ${money(position.price)} · 浮盈亏 ${position.unrealized_pnl !== null && position.unrealized_pnl > 0 ? "+" : ""}${money(position.unrealized_pnl)}元 (${pct(position.unrealized_pct)})`;
    });
    const text = [
      "【Vibe AStock · 本地持仓快照】",
      `持仓 ${positions.length} 只 · 成本 ${money(review.risk.open_cost_total)} 元 · 最新市值 ${money(summary.marketValue)} 元`,
      `浮动盈亏 ${summary.unrealized !== null && summary.unrealized > 0 ? "+" : ""}${money(summary.unrealized)} 元`,
      `止损覆盖 ${review.risk.stop_covered}/${review.risk.open_positions} · 计划风险 ${money(review.risk.planned_risk_total)} 元`,
      "",
      ...rows,
      "",
      "数据来自本机交易日志与当前行情，仅作个人记录与复盘，不构成投资建议。",
    ].join("\n");
    try {
      await shareText({ title: "Vibe AStock · 持仓快照", text, chooserTitle: "分享持仓快照" });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <section className="portfolio-panel">
      <div className="portfolio-title-row">
        <div>
          <span><BriefcaseBusiness size={15} /> 本地持仓</span>
          <h2>持仓 · 浮盈亏 · 计划风险</h2>
          <p>由交易日志成交自动汇总，不维护第二套持仓数据。</p>
        </div>
        <div className="portfolio-title-actions">
          <button onClick={() => void sharePortfolio()} disabled={loading || positions.length === 0} aria-label="分享持仓快照">
            <Share2 size={17} />
          </button>
          <button onClick={() => void refresh()} disabled={loading} aria-label="刷新持仓">
            <RefreshCw size={17} className={loading ? "spin" : ""} />
          </button>
        </div>
      </div>

      {error && <div className="portfolio-error">持仓刷新失败：{error}</div>}
      {!review && loading && <div className="portfolio-empty"><RefreshCw size={18} className="spin" /> 正在汇总持仓…</div>}

      {review && positions.length === 0 && (
        <div className="portfolio-empty">交易日志中暂时没有未平仓成交。新增买入记录后，持仓会自动出现在这里。</div>
      )}

      {review && positions.length > 0 && (
        <>
          <div className="portfolio-summary">
            <div><span>持仓</span><strong>{positions.length}</strong><small>只</small></div>
            <div><span>持仓成本</span><strong>{money(review.risk.open_cost_total)}</strong><small>元</small></div>
            <div><span>最新市值</span><strong>{money(summary.marketValue)}</strong><small>{summary.quoted}/{positions.length} 已报价</small></div>
            <div>
              <span>浮动盈亏</span>
              <strong className={(summary.unrealized ?? 0) >= 0 ? "up" : "down"}>
                {summary.unrealized !== null && summary.unrealized > 0 ? "+" : ""}{money(summary.unrealized)}
              </strong>
              <small>元</small>
            </div>
          </div>

          <div className="portfolio-risk">
            <ShieldAlert size={16} />
            <span>止损覆盖 {review.risk.stop_covered}/{review.risk.open_positions}</span>
            <span>计划风险 {money(review.risk.planned_risk_total)} 元</span>
            {review.risk.without_stop > 0 && <b>{review.risk.without_stop} 笔未写止损</b>}
          </div>

          <div className="portfolio-list">
            {positions.map((position) => {
              const pnlUp = (position.unrealized_pnl ?? 0) >= 0;
              return (
                <button key={position.id} className="portfolio-row" onClick={() => onOpenSymbol(position.symbol)}>
                  <div className="portfolio-main">
                    <div className="portfolio-name">
                      <strong>{position.name || position.symbol}</strong>
                      <span>{position.symbol} · {position.open_shares.toLocaleString("zh-CN")} 股</span>
                    </div>
                    <div className="portfolio-price">
                      <strong>{money(position.price)}</strong>
                      <span className={(position.change_pct ?? 0) >= 0 ? "up" : "down"}>{pct(position.change_pct)}</span>
                    </div>
                  </div>

                  <div className="portfolio-metrics">
                    <span>成本 <b>{money(position.avg_cost)}</b></span>
                    <span>市值 <b>{money(position.market_value)}</b></span>
                    <span>止损 <b>{position.planned_stop ?? "未写"}</b></span>
                    <span>目标 <b>{position.planned_target ?? "未写"}</b></span>
                  </div>

                  <div className="portfolio-pnl">
                    <span className={pnlUp ? "up" : "down"}>
                      {pnlUp ? <TrendingUp size={14} /> : <TrendingDown size={14} />}
                      {position.unrealized_pnl === null ? "行情不可用" : `${position.unrealized_pnl > 0 ? "+" : ""}${money(position.unrealized_pnl)} 元`}
                    </span>
                    <b className={pnlUp ? "up" : "down"}>{pct(position.unrealized_pct)}</b>
                    <ChevronRight size={16} />
                  </div>
                </button>
              );
            })}
          </div>

          <p className="portfolio-note">最新价来自当前行情源；持仓数量与成本来自本地交易日志。浮盈亏仅作记录与复盘，不构成投资建议。</p>
        </>
      )}

      <PortfolioIncome />
      <PortfolioAnalysis />
    </section>
  );
}
