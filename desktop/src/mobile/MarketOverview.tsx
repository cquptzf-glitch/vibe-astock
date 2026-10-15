import { useCallback, useEffect, useMemo, useState } from "react";
import { Activity, RefreshCw, TrendingDown, TrendingUp } from "lucide-react";
import "./overview.css";

interface MarketIndex {
  code: string;
  name: string;
  price: number | null;
  change_pct: number | null;
  change: number | null;
  amount_yuan: number | null;
}

interface DistributionItem {
  key: string;
  label: string;
  count: number | null;
}

interface MarketOverviewData {
  status: "ok" | "partial";
  source: string;
  fetched_at: string;
  trading_date: string;
  stale: boolean;
  indexes: MarketIndex[];
  breadth: {
    exchange: {
      up: number | null;
      down: number | null;
      flat: number | null;
      amount_yuan: number | null;
    };
    distribution: DistributionItem[];
  };
  sentiment: {
    limit_up: number | null;
    broken_board: number | null;
    limit_down: number | null;
    seal_rate: number | null;
  };
  errors: { part?: string; message?: string }[];
}

async function loadOverview(): Promise<MarketOverviewData> {
  const response = await fetch("/api/mobile/market-overview", {
    headers: { Accept: "application/json" },
  });
  const body = await response.json() as MarketOverviewData & { message?: string; error?: string };
  if (!response.ok) throw new Error(body.message ?? body.error ?? `HTTP ${response.status}`);
  return body;
}

function num(value: number | null, digits = 2): string {
  return value === null ? "—" : value.toLocaleString("zh-CN", { maximumFractionDigits: digits });
}

function IndexCard({ item }: { item: MarketIndex }) {
  const direction = item.change_pct === null ? "flat" : item.change_pct >= 0 ? "up" : "down";
  return (
    <div className="mo-index-card">
      <div className="mo-index-name">{item.name || item.code}</div>
      <div className="mo-index-price">{num(item.price)}</div>
      <div className={`mo-index-change ${direction}`}>
        {item.change_pct === null ? "—" : `${item.change_pct >= 0 ? "+" : ""}${num(item.change_pct)}%`}
      </div>
    </div>
  );
}

export function MarketOverview() {
  const [data, setData] = useState<MarketOverviewData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      setData(await loadOverview());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const breadthTotal = useMemo(() => {
    if (!data) return 0;
    return (data.breadth.exchange.up ?? 0) + (data.breadth.exchange.down ?? 0) + (data.breadth.exchange.flat ?? 0);
  }, [data]);

  const upRatio = breadthTotal > 0 ? ((data?.breadth.exchange.up ?? 0) / breadthTotal) * 100 : 50;

  return (
    <section className="mo-wrap">
      <div className="mo-heading">
        <div>
          <div className="mo-eyebrow">市场概览</div>
          <h2>{data?.trading_date ?? "A 股"}</h2>
          <p>{data?.stale ? "当前为最近交易日数据" : "实时市场快照"}</p>
        </div>
        <button className="mo-refresh" onClick={() => void refresh()} disabled={loading} aria-label="刷新市场概览">
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      {error && <div className="mo-error">市场概览暂时不可用：{error}</div>}

      <div className="mo-index-grid">
        {(data?.indexes ?? []).map((item) => <IndexCard key={item.code} item={item} />)}
        {!data && !error && Array.from({ length: 4 }).map((_, index) => <div className="mo-index-card skeleton" key={index} />)}
      </div>

      {data && (
        <>
          <div className="mo-breadth-card">
            <div className="mo-breadth-head">
              <div className="mo-breadth-side up"><TrendingUp size={15} />上涨 <strong>{num(data.breadth.exchange.up, 0)}</strong></div>
              <div className="mo-breadth-side down"><TrendingDown size={15} />下跌 <strong>{num(data.breadth.exchange.down, 0)}</strong></div>
            </div>
            <div className="mo-breadth-bar" aria-label="上涨下跌比例">
              <span style={{ width: `${Math.max(2, Math.min(98, upRatio))}%` }} />
            </div>
            <div className="mo-breadth-foot">
              <span>平盘 {num(data.breadth.exchange.flat, 0)}</span>
              <span>样本 {breadthTotal.toLocaleString("zh-CN")} 家</span>
            </div>
          </div>

          <div className="mo-sentiment-grid">
            <div><span>涨停</span><strong className="up">{num(data.sentiment.limit_up, 0)}</strong></div>
            <div><span>炸板</span><strong>{num(data.sentiment.broken_board, 0)}</strong></div>
            <div><span>跌停</span><strong className="down">{num(data.sentiment.limit_down, 0)}</strong></div>
            <div><span>封板率</span><strong>{data.sentiment.seal_rate === null ? "—" : `${num(data.sentiment.seal_rate, 1)}%`}</strong></div>
          </div>

          <div className="mo-distribution-card">
            <div className="mo-card-title"><Activity size={15} />涨跌分布</div>
            <div className="mo-distribution-list">
              {data.breadth.distribution.map((item) => (
                <div className="mo-distribution-item" key={item.key}>
                  <span>{item.label}</span>
                  <strong>{num(item.count, 0)}</strong>
                </div>
              ))}
            </div>
          </div>

          {data.status === "partial" && (
            <div className="mo-partial">部分数据源未返回，本页只展示已成功取得的数据。</div>
          )}
        </>
      )}
    </section>
  );
}
