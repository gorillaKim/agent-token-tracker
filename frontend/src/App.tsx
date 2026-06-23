import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Session {
  session_id: string;
  agent_type: string;
  agent_version?: string;
  started_at: string;
  ended_at?: string;
  cwd: string;
  model_id?: string;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface AgentSummary {
  agent_type: string;
  session_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

interface LoopDetectionResult {
  session_id: string;
  is_anomaly: boolean;
  signals: any[];
}

interface DailyCost {
  date: string;
  total_cost: number;
}

function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [summaries, setSummaries] = useState<AgentSummary[]>([]);
  const [anomalies, setAnomalies] = useState<LoopDetectionResult[]>([]);
  const [dailyCosts, setDailyCosts] = useState<DailyCost[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Tooltip state
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    visible: boolean;
    date: string;
    cost: number;
  }>({ x: 0, y: 0, visible: false, date: "", cost: 0 });

  const chartRef = useRef<SVGSVGElement>(null);

  async function loadData() {
    try {
      const sessList = await invoke<Session[]>("get_active_sessions");
      setSessions(sessList);

      const sumList = await invoke<AgentSummary[]>("get_agent_summaries");
      setSummaries(sumList);

      const anomalyList = await invoke<LoopDetectionResult[]>("get_loop_signals");
      setAnomalies(anomalyList);

      const costList = await invoke<DailyCost[]>("get_daily_costs");
      setDailyCosts(costList);
    } catch (err: any) {
      setError(err.toString());
    }
  }

  useEffect(() => {
    loadData();

    // db-updated 이벤트 리스닝 연동
    const unlistenPromise = listen("db-updated", () => {
      console.log("[Watch] DB 수정 감지! 데이터를 새로고침합니다.");
      loadData();
    });

    return () => {
      unlistenPromise.then((f) => f());
    };
  }, []);

  // SVG Chart Dimensions
  const chartWidth = 700;
  const chartHeight = 200;
  const paddingLeft = 60;
  const paddingRight = 30;
  const paddingTop = 20;
  const paddingBottom = 30;

  const contentWidth = chartWidth - paddingLeft - paddingRight;
  const contentHeight = chartHeight - paddingTop - paddingBottom;

  // Max cost for Y axis scaling (at least $0.001 to avoid divide by zero)
  const maxCost = Math.max(...dailyCosts.map((d) => d.total_cost), 0.001) * 1.15;

  // Map dailyCosts to points
  const points = dailyCosts.map((d, index) => {
    const x = paddingLeft + (index / Math.max(dailyCosts.length - 1, 1)) * contentWidth;
    const y = paddingTop + contentHeight - (d.total_cost / maxCost) * contentHeight;
    return { x, y, date: d.date, cost: d.total_cost };
  });

  // Calculate Cubic Bezier path
  let pathD = "";
  let areaD = "";
  if (points.length > 0) {
    pathD = `M ${points[0].x} ${points[0].y}`;
    for (let i = 0; i < points.length - 1; i++) {
      const p1 = points[i];
      const p2 = points[i + 1];
      const cp1x = p1.x + (p2.x - p1.x) / 2;
      const cp1y = p1.y;
      const cp2x = p2.x - (p2.x - p1.x) / 2;
      const cp2y = p2.y;
      pathD += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
    }
    areaD = `${pathD} L ${points[points.length - 1].x} ${chartHeight - paddingBottom} L ${points[0].x} ${chartHeight - paddingBottom} Z`;
  }

  // Handle Chart Hover / Tooltip
  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!chartRef.current || points.length === 0) return;
    const rect = chartRef.current.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;

    // Convert mouseX to SVG coordinates
    const svgX = (mouseX / rect.width) * chartWidth;
    
    // Find closest point
    let closestPoint = points[0];
    let minDist = Math.abs(points[0].x - svgX);
    for (let i = 1; i < points.length; i++) {
      const dist = Math.abs(points[i].x - svgX);
      if (dist < minDist) {
        minDist = dist;
        closestPoint = points[i];
      }
    }

    // Set tooltip position in DOM coordinates relative to chart container
    const domX = (closestPoint.x / chartWidth) * rect.width;
    const domY = (closestPoint.y / chartHeight) * rect.height;

    setTooltip({
      x: domX,
      y: domY - 10, // slightly above point
      visible: true,
      date: closestPoint.date,
      cost: closestPoint.cost,
    });
  };

  const handleMouseLeave = () => {
    setTooltip((prev) => ({ ...prev, visible: false }));
  };

  // Aggregated totals for status bar
  const totalCostOverall = summaries.reduce((acc, curr) => acc + curr.total_cost_usd, 0);
  const totalSessionsOverall = sessions.length;

  return (
    <div className="dashboard-container">
      {/* Sidebar Navigation */}
      <aside className="sidebar">
        <div className="sidebar-title">
          <span style={{ fontSize: "1.6rem" }}>⚡</span> ATK Monitor
        </div>
        <ul className="sidebar-menu">
          <li className="menu-item active">
            <span>📊</span> 대시보드
          </li>
          <li className="menu-item">
            <span>🔍</span> 실시간 스캔
          </li>
          <li className="menu-item">
            <span>⚙️</span> 환경 설정
          </li>
        </ul>
        <div style={{ marginTop: "auto", fontSize: "0.75rem", color: "hsl(215, 20%, 40%)", textAlign: "center" }}>
          v0.1.0-alpha
        </div>
      </aside>

      {/* Main Panel Content */}
      <main className="main-content">
        {/* Top Status Bar */}
        <header className="statusbar">
          <div className="statusbar-metrics">
            <div className="metric-item">
              <span className="metric-label">총 누적 세션</span>
              <span className="metric-value">{totalSessionsOverall} Sessions</span>
            </div>
            <div className="metric-item">
              <span className="metric-label">총 누적 토큰 비용</span>
              <span className="metric-value" style={{ color: "var(--neon-purple)" }}>
                ${totalCostOverall.toFixed(4)} USD
              </span>
            </div>
          </div>
          <div className="pulse-badge">
            <span className="pulse-dot"></span>
            <span>로컬 감시 모드 작동 중</span>
          </div>
        </header>

        {error && <div style={{ color: "hsl(0, 100%, 65%)", marginBottom: "1rem", fontWeight: "600" }}>⚠️ 오류: {error}</div>}

        {/* 3대 에이전트 요약 카드 그리드 */}
        <section className="cards-grid">
          {summaries.map((s) => {
            let iconClass = "icon-claude";
            let displayName = "Claude Code";
            let short = "CC";
            if (s.agent_type === "codex") {
              iconClass = "icon-codex";
              displayName = "Codex";
              short = "CD";
            } else if (s.agent_type === "antigravity") {
              iconClass = "icon-antigravity";
              displayName = "Antigravity";
              short = "AG";
            }

            return (
              <div key={s.agent_type} className="agent-card glass">
                <div className="agent-card-header">
                  <h3 className="agent-name">{displayName}</h3>
                  <div className={`agent-icon-wrapper ${iconClass}`}>{short}</div>
                </div>
                <div className="agent-stats">
                  <div>
                    <div className="stat-label">활성 세션</div>
                    <div className="stat-val">{s.session_count} 건</div>
                  </div>
                  <div>
                    <div className="stat-label">총 입/출력 토큰</div>
                    <div className="stat-val">
                      {s.total_input_tokens + s.total_output_tokens > 0 
                        ? (s.total_input_tokens + s.total_output_tokens).toLocaleString()
                        : "-"}
                    </div>
                  </div>
                  <div className="agent-cost">
                    <span className="stat-label">누적 비용</span>
                    <span className="agent-cost-val">${s.total_cost_usd.toFixed(5)}</span>
                  </div>
                </div>
              </div>
            );
          })}
        </section>

        {/* Spline Chart */}
        <section className="chart-container glass">
          <div className="chart-title-wrapper">
            <h3 className="chart-title">최근 14일간 토큰 비용 추이 (USD)</h3>
            <span style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 55%)" }}>스플라인 차트</span>
          </div>

          {dailyCosts.length > 0 ? (
            <div style={{ position: "relative" }}>
              <svg
                ref={chartRef}
                viewBox={`0 0 ${chartWidth} ${chartHeight}`}
                className="svg-chart"
                onMouseMove={handleMouseMove}
                onMouseLeave={handleMouseLeave}
              >
                <defs>
                  {/* Neon Line Gradient */}
                  <linearGradient id="chart-gradient" x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0%" stopColor="var(--neon-blue)" />
                    <stop offset="100%" stopColor="var(--neon-purple)" />
                  </linearGradient>
                  {/* Under Area Gradient */}
                  <linearGradient id="area-gradient" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="var(--neon-purple)" stopOpacity="0.4" />
                    <stop offset="100%" stopColor="var(--neon-purple)" stopOpacity="0" />
                  </linearGradient>
                  {/* Glow Filter */}
                  <filter id="neon-glow" x="-20%" y="-20%" width="140%" height="140%">
                    <feGaussianBlur stdDeviation="6" result="blur" />
                    <feComposite in="SourceGraphic" in2="blur" operator="over" />
                  </filter>
                </defs>

                {/* Grid Lines */}
                {[0, 0.25, 0.5, 0.75, 1].map((ratio, i) => {
                  const y = paddingTop + ratio * contentHeight;
                  return (
                    <line
                      key={i}
                      x1={paddingLeft}
                      y1={y}
                      x2={chartWidth - paddingRight}
                      y2={y}
                      className="chart-grid-line"
                    />
                  );
                })}

                {/* Y Axis Labels */}
                {[0, 0.5, 1].map((ratio, i) => {
                  const y = paddingTop + ratio * contentHeight;
                  const val = (maxCost * (1 - ratio)).toFixed(5);
                  return (
                    <text
                      key={i}
                      x={paddingLeft - 8}
                      y={y + 3}
                      textAnchor="end"
                      className="chart-axis-text"
                    >
                      ${val}
                    </text>
                  );
                })}

                {/* X Axis Axis line */}
                <line
                  x1={paddingLeft}
                  y1={chartHeight - paddingBottom}
                  x2={chartWidth - paddingRight}
                  y2={chartHeight - paddingBottom}
                  className="chart-axis-line"
                />

                {/* X Axis Labels (every 2 days to fit) */}
                {dailyCosts.map((d, i) => {
                  if (i % 2 !== 0 && i !== dailyCosts.length - 1) return null;
                  const x = paddingLeft + (i / (dailyCosts.length - 1)) * contentWidth;
                  const dateStr = d.date.substring(5); // MM-DD
                  return (
                    <text
                      key={i}
                      x={x}
                      y={chartHeight - paddingBottom + 16}
                      textAnchor="middle"
                      className="chart-axis-text"
                    >
                      {dateStr}
                    </text>
                  );
                })}

                {/* Area under curve */}
                {areaD && <path d={areaD} className="chart-area" />}

                {/* Spline Curve Line */}
                {pathD && <path d={pathD} className="chart-line" />}

                {/* Point Circles */}
                {points.map((p, i) => (
                  <circle
                    key={i}
                    cx={p.x}
                    cy={p.y}
                    r={3.5}
                    className="chart-point"
                  />
                ))}
              </svg>

              {/* DOM Interactive Tooltip */}
              <div
                className="chart-tooltip"
                style={{
                  opacity: tooltip.visible ? 1 : 0,
                  left: `${tooltip.x}px`,
                  top: `${tooltip.y}px`,
                }}
              >
                <div style={{ fontWeight: 600, color: "var(--neon-blue)" }}>{tooltip.date}</div>
                <div style={{ marginTop: "2px" }}>비용: <span style={{ color: "#fff", fontWeight: 700 }}>${tooltip.cost.toFixed(5)}</span></div>
              </div>
            </div>
          ) : (
            <div style={{ padding: "3rem", textAlign: "center", color: "hsl(215, 20%, 50%)" }}>
              차트 데이터를 불러오는 중이거나 최근 14일간의 토큰 비용 기록이 없습니다.
            </div>
          )}
        </section>

        {/* Bottom details column grid */}
        <div className="bottom-sections">
          {/* Active Sessions list */}
          <section className="section-card glass">
            <h4 className="section-header">활성 세션 현황</h4>
            <div className="session-list">
              {sessions.slice(0, 5).map((s) => (
                <div key={s.session_id} className="session-item">
                  <div className="session-meta">
                    <span className="session-id">{s.session_id.substring(0, 16)}...</span>
                    <span className="session-agent">{s.agent_type} • {s.cwd}</span>
                  </div>
                  <span className="session-tokens">
                    {(s.total_input_tokens + s.total_output_tokens).toLocaleString()} Tokens
                  </span>
                </div>
              ))}
              {sessions.length === 0 && (
                <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "1rem" }}>
                  현재 활성화된 세션이 없습니다.
                </div>
              )}
            </div>
          </section>

          {/* Anomalies list */}
          <section className="section-card glass">
            <h4 className="section-header">실시간 이상 징후 (Anomalies)</h4>
            <div className="anomaly-list">
              {anomalies.slice(0, 3).map((a) => (
                <div key={a.session_id} className="anomaly-item">
                  <span className="anomaly-id">Session: {a.session_id.substring(0, 8)}...</span>
                  <span className="anomaly-desc">오작동 시그널 {a.signals.length}개 검출됨</span>
                </div>
              ))}
              {anomalies.length === 0 && (
                <div style={{ color: "hsl(150, 100%, 35%)", textAlign: "center", padding: "1rem" }}>
                  지속 루프 및 토큰 폭팽 등의 오작동 세션이 없습니다.
                </div>
              )}
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}

export default App;
