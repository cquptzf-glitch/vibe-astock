import {
  CalendarCheck2,
  Flame,
  RefreshCw,
  Repeat2,
  Save,
  ShieldAlert,
  TrendingDown,
  TrendingUp,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import "./daily-review.css";

type MarketOverview = {
  trading_date: string;
  stale: boolean;
  breadth: {
    exchange: { up: number | null; down: number | null; flat: number | null };
  };
  sentiment: {
    limit_up: number | null;
    broken_board: number | null;
    limit_down: number | null;
    seal_rate: number | null;
  };
};

type LimitUpItem = {
  code: string;
  name: string;
  limit_days: number | null;
  industry: string;
};

type LimitUpResult = {
  date: string;
  stale: boolean;
  count: number;
  items: LimitUpItem[];
};

type RotationBrief = {
  code: string;
  name: string;
  rank: number;
  change_pct: number | null;
  main_net_yuan: number | null;
};

type RotationData = {
  summary: {
    persistent_count: number;
    new_hot_count: number;
    cooling_count: number;
    persistent: RotationBrief[];
    new_hot: RotationBrief[];
    cooling: RotationBrief[];
  };
};

type DailyRecord = {
  date: string;
  saved: boolean;
  observation: string;
  tomorrow_check: string;
  created_at: string | null;
  updated_at: string | null;
  storage: string;
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

function chinaToday(): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
}

function value(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : value.toLocaleString("zh-CN", { maximumFractionDigits: 1 });
}

function pct(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : `${value.toFixed(1)}%`;
}

export function DailyReviewPanel({ onOpenSymbol }: { onOpenSymbol: (symbol: string) => void }) {
  const [market, setMarket] = useState<MarketOverview | null>(null);
  const [limitUp, setLimitUp] = useState<LimitUpResult | null>(null);
  const [rotation, setRotation] = useState<RotationData | null>(null);
  const [record, setRecord] = useState<DailyRecord | null>(null);
  const [observation, setObservation] = useState("");
  const [tomorrowCheck, setTomorrowCheck] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  async function refresh() {
    setLoading(true);
    setError("");
    setMessage("");

    const results = await Promise.allSettled([
      api<MarketOverview>("/mobile/market-overview"),
      api<LimitUpResult>("/mobile/shortline/limit-up"),
      api<RotationData>("/mobile/rotation?type=concept&top=20"),
    ]);

    const nextMarket = results[0].status === "fulfilled" ? results[0].value : null;
    const nextLimitUp = results[1].status === "fulfilled" ? results[1].value : null;
    const nextRotation = results[2].status === "fulfilled" ? results[2].value : null;
    setMarket(nextMarket);
    setLimitUp(nextLimitUp);
    setRotation(nextRotation);

    const reviewDate = nextMarket?.trading_date || nextLimitUp?.date || chinaToday();
    try {
      const nextRecord = await api<DailyRecord>(`/mobile/daily-review?date=${encodeURIComponent(reviewDate)}`);
      setRecord(nextRecord);
      setObservation(nextRecord.observation);
      setTomorrowCheck(nextRecord.tomorrow_check);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }

    const failed = results.filter((item) => item.status === "rejected").length;
    if (failed === results.length) {
      setError((current) => current || "市场概览、涨停池和板块轮动当前均不可用；仍可编辑本地复盘记录。");
    } else if (failed > 0) {
      setMessage(`有 ${failed} 项市场数据暂时不可用，本页已保留成功返回的部分。`);
    }
    setLoading(false);
  }

  useEffect(() => { void refresh(); }, []);

  const reviewDate = record?.date || market?.trading_date || limitUp?.date || chinaToday();
  const breadth = market?.breadth.exchange;
  const advanceDecline = breadth
    ? (breadth.up ?? 0) - (breadth.down ?? 0)
    : null;
  const maxBoard = useMemo(() => {
    return (limitUp?.items ?? []).reduce((max, item) => Math.max(max, item.limit_days ?? 1), 0);
  }, [limitUp]);
  const leaders = useMemo(() => {
    if (!limitUp) return [];
    return [...limitUp.items]
      .sort((a, b) => (b.limit_days ?? 1) - (a.limit_days ?? 1))
      .slice(0, 5);
  }, [limitUp]);

  const dirty = Boolean(record)
    && (observation !== record?.observation || tomorrowCheck !== record?.tomorrow_check);

  async function save() {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      const next = await api<DailyRecord>("/mobile/daily-review", {
        method: "POST",
        body: JSON.stringify({
          date: reviewDate,
          observation,
          tomorrow_check: tomorrowCheck,
        }),
      });
      setRecord(next);
      setObservation(next.observation);
      setTomorrowCheck(next.tomorrow_check);
      setMessage("今日复盘已保存到手机本地 SQLite。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="daily-review-panel">
      <div className="daily-review-title">
        <div>
          <span><CalendarCheck2 size={15} /> 市场日复盘</span>
          <h1>{reviewDate} · 今日与明日验证</h1>
          <p>自动汇总市场、涨停高度和题材轮动；主观观察只保存在手机本地。</p>
        </div>
        <button onClick={() => void refresh()} disabled={loading || saving} aria-label="刷新日复盘">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && <div className="daily-review-error">{error}</div>}
      {message && <div className="daily-review-message">{message}</div>}

      <div className="daily-review-cards">
        <div>
          <TrendingUp size={16} />
          <span>上涨 / 下跌</span>
          <strong>{value(breadth?.up ?? null)} / {value(breadth?.down ?? null)}</strong>
          <small className={(advanceDecline ?? 0) >= 0 ? "up" : "down"}>差额 {advanceDecline === null ? "—" : advanceDecline}</small>
        </div>
        <div>
          <Flame size={16} />
          <span>涨停 / 跌停</span>
          <strong>{value(market?.sentiment.limit_up ?? limitUp?.count ?? null)} / {value(market?.sentiment.limit_down ?? null)}</strong>
          <small>最高 {maxBoard || "—"} 板</small>
        </div>
        <div>
          <ShieldAlert size={16} />
          <span>炸板 / 封板率</span>
          <strong>{value(market?.sentiment.broken_board ?? null)} / {pct(market?.sentiment.seal_rate ?? null)}</strong>
          <small>{market?.stale || limitUp?.stale ? "最近交易日数据" : "当前交易日"}</small>
        </div>
        <div>
          <Repeat2 size={16} />
          <span>持续强 / 新走强</span>
          <strong>{rotation?.summary.persistent_count ?? "—"} / {rotation?.summary.new_hot_count ?? "—"}</strong>
          <small>退潮 {rotation?.summary.cooling_count ?? "—"}</small>
        </div>
      </div>

      <section className="daily-review-block">
        <div className="daily-review-heading"><Flame size={16} /><strong>连板与高标</strong></div>
        {leaders.length === 0 ? (
          <div className="daily-review-empty">暂无可展示的涨停池数据。</div>
        ) : (
          <div className="daily-review-leaders">
            {leaders.map((item) => (
              <button key={item.code} onClick={() => onOpenSymbol(item.code)}>
                <div><strong>{item.name}</strong><span>{item.code}{item.industry ? ` · ${item.industry}` : ""}</span></div>
                <b>{Math.max(1, item.limit_days ?? 1)} 板</b>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="daily-review-block">
        <div className="daily-review-heading"><Repeat2 size={16} /><strong>题材轮动</strong></div>
        <div className="daily-review-rotation">
          <div>
            <span>持续强</span>
            <p>{rotation?.summary.persistent.slice(0, 6).map((item) => item.name).join(" / ") || "—"}</p>
          </div>
          <div>
            <span>新走强</span>
            <p>{rotation?.summary.new_hot.slice(0, 6).map((item) => item.name).join(" / ") || "—"}</p>
          </div>
          <div>
            <span>退潮观察</span>
            <p>{rotation?.summary.cooling.slice(0, 6).map((item) => item.name).join(" / ") || "—"}</p>
          </div>
        </div>
      </section>

      <section className="daily-review-block editor">
        <div className="daily-review-heading"><TrendingDown size={16} /><strong>你的复盘记录</strong><span>{record?.saved ? "已保存" : "未保存"}</span></div>
        <label>
          <span>今日观察</span>
          <textarea
            maxLength={6000}
            value={observation}
            onChange={(event) => setObservation(event.target.value)}
            placeholder="例如：指数缩量但高标承接仍强；地产链冲高回落，AI 应用午后回流……"
          />
        </label>
        <label>
          <span>明日验证</span>
          <textarea
            maxLength={6000}
            value={tomorrowCheck}
            onChange={(event) => setTomorrowCheck(event.target.value)}
            placeholder="例如：观察最高板是否继续晋级；验证持续强板块是否仍有资金回流……"
          />
        </label>
        <button className="daily-review-save" onClick={() => void save()} disabled={saving || (!dirty && Boolean(record?.saved))}>
          {saving ? <RefreshCw size={15} className="spin" /> : <Save size={15} />} 保存日复盘
        </button>
      </section>

      <p className="daily-review-note">自动数据用于记录市场状态，主观文本用于后续验证；页面不生成个股买卖建议。</p>
    </section>
  );
}
