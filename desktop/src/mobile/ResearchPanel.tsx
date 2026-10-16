import { useEffect, useState } from "react";
import { Building2, FileText, LineChart, RefreshCw } from "lucide-react";

type ResearchTab = "profile" | "financials" | "announcements" | "estimates";

interface ProfileResult {
  symbol: string;
  market: string;
  name: string;
  industry: string | null;
  ipo_date: string | null;
  total_shares: number | null;
  float_shares: number | null;
  total_market_cap: number | null;
  float_market_cap: number | null;
  source: string;
  fetched_at: string;
}

interface AnnouncementItem {
  title: string;
  date: string;
  art_code: string;
  pdf_url: string;
}

interface AnnouncementsResult {
  items: AnnouncementItem[];
  count: number;
  source: string;
}

interface FinancialRow {
  period: string;
  revenue_cum: number | null;
  net_profit_parent_cum: number | null;
  net_profit_deducted_cum: number | null;
}

interface FinancialsResult {
  rows: FinancialRow[];
  count: number;
  status: string;
  note: string;
  source: string;
}

interface EstimateReport {
  info_code: string | null;
  published_at: string;
  organization: string | null;
  title: string | null;
  rating: string | null;
  eps: Record<string, number>;
}

interface EstimatesResult {
  reports: EstimateReport[];
  count: number;
  status: string;
  note: string;
  source: string;
}

async function api<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, { headers: { Accept: "application/json" } });
  const body = await response.json().catch(() => null) as ({ message?: string; error?: string } & T) | null;
  if (!response.ok) {
    throw new Error(body?.message ?? body?.error ?? `HTTP ${response.status}`);
  }
  return body as T;
}

function money(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  if (Math.abs(value) >= 100_000_000) {
    return `${(value / 100_000_000).toLocaleString("zh-CN", { maximumFractionDigits: 2 })} 亿`;
  }
  if (Math.abs(value) >= 10_000) {
    return `${(value / 10_000).toLocaleString("zh-CN", { maximumFractionDigits: 2 })} 万`;
  }
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function shares(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "—";
  return `${(value / 100_000_000).toLocaleString("zh-CN", { maximumFractionDigits: 2 })} 亿股`;
}

export function ResearchPanel({ symbol }: { symbol: string }) {
  const [tab, setTab] = useState<ResearchTab>("profile");
  const [profile, setProfile] = useState<ProfileResult | null>(null);
  const [announcements, setAnnouncements] = useState<AnnouncementsResult | null>(null);
  const [financials, setFinancials] = useState<FinancialsResult | null>(null);
  const [estimates, setEstimates] = useState<EstimatesResult | null>(null);
  const [errors, setErrors] = useState<Partial<Record<ResearchTab, string>>>({});
  const [loading, setLoading] = useState(true);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    if (!symbol) return;
    let cancelled = false;
    setLoading(true);
    setProfile(null);
    setAnnouncements(null);
    setFinancials(null);
    setEstimates(null);
    setErrors({});

    const encoded = encodeURIComponent(symbol);
    const tasks: Array<[ResearchTab, Promise<unknown>]> = [
      ["profile", api<ProfileResult>(`/mobile/research/profile?symbol=${encoded}`)],
      ["financials", api<FinancialsResult>(`/mobile/research/financials?symbol=${encoded}&periods=8`)],
      ["announcements", api<AnnouncementsResult>(`/mobile/research/announcements?symbol=${encoded}&limit=20`)],
      ["estimates", api<EstimatesResult>(`/mobile/research/estimates?symbol=${encoded}&limit=20`)],
    ];

    void Promise.allSettled(tasks.map(([, promise]) => promise)).then((results) => {
      if (cancelled) return;
      const nextErrors: Partial<Record<ResearchTab, string>> = {};
      results.forEach((result, index) => {
        const task = tasks[index];
        if (!task) return;
        const key = task[0];
        if (result.status === "rejected") {
          nextErrors[key] = result.reason instanceof Error ? result.reason.message : String(result.reason);
          return;
        }
        if (key === "profile") setProfile(result.value as ProfileResult);
        if (key === "financials") setFinancials(result.value as FinancialsResult);
        if (key === "announcements") setAnnouncements(result.value as AnnouncementsResult);
        if (key === "estimates") setEstimates(result.value as EstimatesResult);
      });
      setErrors(nextErrors);
      setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [symbol, reloadKey]);

  const activeError = errors[tab];

  return (
    <article className="mobile-quote-card" style={{ marginTop: 14 }}>
      <div className="mobile-section-title" style={{ marginBottom: 10 }}>
        <div>
          <h1 style={{ fontSize: 18 }}>个股研究</h1>
          <p>公司资料、财务、公告和机构预测由手机端 Rust Core 直接取数。</p>
        </div>
        <button
          type="button"
          className="mobile-delete"
          onClick={() => setReloadKey((value) => value + 1)}
          aria-label="刷新个股研究数据"
        >
          <RefreshCw size={17} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="mobile-stock-tabs" role="tablist" aria-label="个股研究">
        <button role="tab" aria-selected={tab === "profile"} className={tab === "profile" ? "active" : ""} onClick={() => setTab("profile")}>资料</button>
        <button role="tab" aria-selected={tab === "financials"} className={tab === "financials" ? "active" : ""} onClick={() => setTab("financials")}>财务</button>
        <button role="tab" aria-selected={tab === "announcements"} className={tab === "announcements" ? "active" : ""} onClick={() => setTab("announcements")}>公告</button>
        <button role="tab" aria-selected={tab === "estimates"} className={tab === "estimates" ? "active" : ""} onClick={() => setTab("estimates")}>预测</button>
      </div>

      {loading && !activeError && (
        <div className="mobile-empty"><RefreshCw className="spin" size={18} /> 正在加载研究数据…</div>
      )}
      {activeError && <div className="mobile-error">{activeError}</div>}

      {tab === "profile" && profile && (
        <>
          <div className="mobile-metrics" style={{ marginTop: 12 }}>
            <div><span>行业</span><strong>{profile.industry || "—"}</strong></div>
            <div><span>上市日期</span><strong>{profile.ipo_date || "—"}</strong></div>
            <div><span>总股本</span><strong>{shares(profile.total_shares)}</strong></div>
            <div><span>流通股本</span><strong>{shares(profile.float_shares)}</strong></div>
            <div><span>总市值</span><strong>{money(profile.total_market_cap)}</strong></div>
            <div><span>流通市值</span><strong>{money(profile.float_market_cap)}</strong></div>
          </div>
          <div className="mobile-source-line"><Building2 size={14} /> 数据源：{profile.source}</div>
        </>
      )}

      {tab === "financials" && financials && (
        <>
          <div className="mobile-watch-list" style={{ marginTop: 12 }}>
            {financials.rows.map((row) => (
              <div className="mobile-watch-row" key={row.period}>
                <div className="mobile-watch-open" style={{ cursor: "default" }}>
                  <div style={{ width: "100%" }}>
                    <strong>{row.period}</strong>
                    <span>营收 {money(row.revenue_cum)} · 归母净利 {money(row.net_profit_parent_cum)}</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
          <div className="mobile-source-line"><LineChart size={14} /> {financials.note}</div>
        </>
      )}

      {tab === "announcements" && announcements && (
        <div className="mobile-watch-list" style={{ marginTop: 12 }}>
          {announcements.items.length === 0 ? (
            <div className="mobile-empty">暂无公告</div>
          ) : announcements.items.slice(0, 20).map((item) => (
            <div className="mobile-watch-row" key={item.art_code}>
              <div className="mobile-watch-open" style={{ cursor: "default" }}>
                <div style={{ width: "100%" }}>
                  <strong>{item.title}</strong>
                  <span>{item.date} · {item.art_code}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {tab === "estimates" && estimates && (
        <>
          <div className="mobile-watch-list" style={{ marginTop: 12 }}>
            {estimates.reports.slice(0, 12).map((report, index) => (
              <div className="mobile-watch-row" key={report.info_code || `${report.published_at}-${index}`}>
                <div className="mobile-watch-open" style={{ cursor: "default" }}>
                  <div style={{ width: "100%" }}>
                    <strong>{report.organization || "机构研报"}{report.rating ? ` · ${report.rating}` : ""}</strong>
                    <span>
                      {report.published_at || "—"} · {Object.entries(report.eps).map(([year, eps]) => `${year} EPS ${eps}`).join(" / ")}
                    </span>
                  </div>
                </div>
              </div>
            ))}
          </div>
          <div className="mobile-source-line"><FileText size={14} /> {estimates.note}</div>
        </>
      )}
    </article>
  );
}
