import React, { useEffect, useState, useRef, useMemo } from "react";
import { formatTokens } from "../utils/formatters";

interface ChartDataItem {
  date: string;
  value: number;
  claude: number;
  codex: number;
  antigravity: number;
}

interface SplineChartProps {
  dailyTokenUsage: any[];
  hourlyTokenUsage: any[];
}

/**
 * 인터랙티브 SVG 스플라인 차트 컴포넌트
 * 
 * 마우스오버 시 정확한 포인트에 정렬되어 뜨는 툴팁과 점선 가이드라인을 제공합니다.
 */
export function SplineChart({
  dailyTokenUsage,
  hourlyTokenUsage,
}: SplineChartProps) {
  const [chartViewMode, setChartViewMode] = useState<"daily" | "hourly">("daily");
  const [chartDays, setChartDays] = useState<number>(14);
  const [chartWidth, setChartWidth] = useState(700);

  const chartRef = useRef<SVGSVGElement>(null);

  // SVG Chart Dimensions
  const chartHeight = 240;
  const paddingLeft = 60;
  const paddingRight = 30;
  const paddingTop = 20;
  const paddingBottom = 30;

  // ResizeObserver를 통해 SVG 크기를 감지하여 유연한 반응형 스케일 지원
  useEffect(() => {
    if (!chartRef.current) return;
    const parent = chartRef.current.parentElement;
    if (!parent) return;

    const resizeObserver = new ResizeObserver((entries) => {
      for (let entry of entries) {
        const width = Math.max(entry.contentRect.width, 300);
        setChartWidth(width);
      }
    });
    resizeObserver.observe(parent);

    const initialRect = parent.getBoundingClientRect();
    if (initialRect.width > 0) {
      setChartWidth(Math.max(initialRect.width, 300));
    }

    return () => resizeObserver.disconnect();
  }, []);

  const chartData = useMemo<ChartDataItem[]>(() => {
    return chartViewMode === "daily"
      ? dailyTokenUsage.slice(-chartDays).map(d => ({
          date: d.date,
          value: d.total_tokens,
          claude: d.claude_tokens || 0,
          codex: d.codex_tokens || 0,
          antigravity: d.antigravity_tokens || 0
        }))
      : hourlyTokenUsage.map(d => ({
          date: `${d.hour}:00`,
          value: d.total_tokens,
          claude: d.claude_tokens || 0,
          codex: d.codex_tokens || 0,
          antigravity: d.antigravity_tokens || 0
        }));
  }, [chartViewMode, chartDays, dailyTokenUsage, hourlyTokenUsage]);

  const contentWidth = chartWidth - paddingLeft - paddingRight;
  const contentHeight = chartHeight - paddingTop - paddingBottom;

  const maxValue = useMemo(() => {
    return Math.max(...chartData.map((d) => d.value), 1000) * 1.15;
  }, [chartData]);

  const getLinePoints = (field: "value" | "claude" | "codex" | "antigravity") => {
    return chartData.map((d, index) => {
      const x = paddingLeft + (index / Math.max(chartData.length - 1, 1)) * contentWidth;
      const val = d[field];
      const y = paddingTop + contentHeight - (val / maxValue) * contentHeight;
      return { x, y, date: d.date, value: val };
    });
  };

  const totalPoints = useMemo(() => getLinePoints("value"), [chartData, maxValue, contentWidth]);
  const claudePoints = useMemo(() => getLinePoints("claude"), [chartData, maxValue, contentWidth]);
  const codexPoints = useMemo(() => getLinePoints("codex"), [chartData, maxValue, contentWidth]);
  const antigravityPoints = useMemo(() => getLinePoints("antigravity"), [chartData, maxValue, contentWidth]);

  const generatePaths = (pts: typeof totalPoints) => {
    let pathD = "";
    let areaD = "";
    if (pts.length > 0) {
      pathD = `M ${pts[0].x} ${pts[0].y}`;
      for (let i = 0; i < pts.length - 1; i++) {
        const p1 = pts[i];
        const p2 = pts[i + 1];
        const cp1x = p1.x + (p2.x - p1.x) / 2;
        const cp1y = p1.y;
        const cp2x = p2.x - (p2.x - p1.x) / 2;
        const cp2y = p2.y;
        pathD += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
      }
      areaD = `${pathD} L ${pts[pts.length - 1].x} ${chartHeight - paddingBottom} L ${pts[0].x} ${chartHeight - paddingBottom} Z`;
    }
    return { pathD, areaD };
  };

  const totalPath = useMemo(() => generatePaths(totalPoints), [totalPoints]);
  const claudePath = useMemo(() => generatePaths(claudePoints), [claudePoints]);
  const codexPath = useMemo(() => generatePaths(codexPoints), [codexPoints]);
  const antigravityPath = useMemo(() => generatePaths(antigravityPoints), [antigravityPoints]);

  // Tooltip local state
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    visible: boolean;
    date: string;
    value: number;
    claude: number;
    codex: number;
    antigravity: number;
  }>({ x: 0, y: 0, visible: false, date: "", value: 0, claude: 0, codex: 0, antigravity: 0 });

  const activePoint = useMemo(() => {
    return totalPoints.find(p => p.date === tooltip.date);
  }, [totalPoints, tooltip.date]);

  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!chartRef.current || chartData.length === 0) return;
    const rect = chartRef.current.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;

    const svgX = (mouseX / rect.width) * chartWidth;

    let closestIndex = 0;
    let minDist = Math.abs(totalPoints[0].x - svgX);
    for (let i = 1; i < totalPoints.length; i++) {
      const dist = Math.abs(totalPoints[i].x - svgX);
      if (dist < minDist) {
        minDist = dist;
        closestIndex = i;
      }
    }

    const closestPoint = totalPoints[closestIndex];
    const dataItem = chartData[closestIndex];

    // 스케일링 오차를 완벽 방지하기 위해 픽셀 대신 백분율(%) 단위로 좌표 지정
    const pctX = (closestPoint.x / chartWidth) * 100;
    const pctY = (closestPoint.y / chartHeight) * 100;

    setTooltip({
      x: pctX,
      y: pctY,
      visible: true,
      date: closestPoint.date,
      value: closestPoint.value,
      claude: dataItem.claude,
      codex: dataItem.codex,
      antigravity: dataItem.antigravity,
    });
  };

  const handleMouseLeave = () => {
    setTooltip((prev) => ({ ...prev, visible: false }));
  };

  return (
    <section className="chart-container glass" style={{ marginTop: "1.5rem", marginBottom: "1.5rem" }}>
      <div className="chart-title-wrapper" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1rem" }}>
        <div>
          <h3 className="chart-title" style={{ fontSize: "1.1rem", fontWeight: "700" }}>
            {chartViewMode === "daily" ? "일자별 토큰 사용량 추이" : "시간대별 토큰 사용량 추이"}
          </h3>
          <span style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 55%)" }}>최근 에이전트 토큰 누적 흐름</span>
        </div>

        <div style={{ display: "flex", gap: "0.75rem", alignItems: "center" }}>
          {chartViewMode === "daily" && (
            <div style={{ display: "flex", alignItems: "center", gap: "0.3rem", background: "rgba(255,255,255,0.02)", padding: "2px 6px", borderRadius: "8px", border: "1px solid rgba(255,255,255,0.06)" }}>
              <span style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)", fontWeight: 600, paddingLeft: "4px", paddingRight: "2px" }}>기간:</span>
              {([{ label: "3d", value: 3 }, { label: "7d", value: 7 }, { label: "14d", value: 14 }, { label: "1m", value: 30 }] as { label: string; value: number }[]).map(opt => (
                <button
                  key={opt.value}
                  onClick={() => setChartDays(opt.value)}
                  style={{
                    padding: "0.2rem 0.55rem",
                    fontSize: "0.7rem",
                    fontWeight: 700,
                    borderRadius: "6px",
                    border: "none",
                    background: chartDays === opt.value ? "rgba(0, 242, 254, 0.12)" : "transparent",
                    color: chartDays === opt.value ? "var(--neon-blue)" : "hsl(215, 20%, 55%)",
                    cursor: "pointer",
                    transition: "all 0.15s ease",
                  }}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}

          <div style={{ display: "flex", gap: "2px", background: "rgba(255,255,255,0.03)", padding: "2px", borderRadius: "8px", border: "1px solid rgba(255,255,255,0.08)" }}>
            <button
              onClick={() => setChartViewMode("daily")}
              style={{
                padding: "0.35rem 0.75rem",
                fontSize: "0.8rem",
                fontWeight: 600,
                borderRadius: "6px",
                border: "none",
                cursor: "pointer",
                background: chartViewMode === "daily" ? "var(--neon-blue)" : "transparent",
                color: chartViewMode === "daily" ? "#0a0c10" : "hsl(215, 20%, 65%)",
                transition: "all 0.2s ease"
              }}
            >
              일자별
            </button>
            <button
              onClick={() => setChartViewMode("hourly")}
              style={{
                padding: "0.35rem 0.75rem",
                fontSize: "0.8rem",
                fontWeight: 600,
                borderRadius: "6px",
                border: "none",
                cursor: "pointer",
                background: chartViewMode === "hourly" ? "var(--neon-blue)" : "transparent",
                color: chartViewMode === "hourly" ? "#0a0c10" : "hsl(215, 20%, 65%)",
                transition: "all 0.2s ease"
              }}
            >
              시간대별
            </button>
          </div>
        </div>
      </div>

      {chartData.length > 0 ? (
        <div style={{ position: "relative" }}>
          <svg
            ref={chartRef}
            viewBox={`0 0 ${chartWidth} ${chartHeight}`}
            className="svg-chart"
            onMouseMove={handleMouseMove}
            onMouseLeave={handleMouseLeave}
          >
            <defs>
              <linearGradient id="claude-gradient" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0%" stopColor="hsl(25, 95%, 55%)" />
                <stop offset="100%" stopColor="hsl(35, 90%, 65%)" />
              </linearGradient>
              <linearGradient id="claude-area-gradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="hsl(25, 95%, 55%)" stopOpacity="0.25" />
                <stop offset="100%" stopColor="hsl(25, 95%, 55%)" stopOpacity="0" />
              </linearGradient>

              <linearGradient id="codex-gradient" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0%" stopColor="hsl(180, 100%, 45%)" />
                <stop offset="100%" stopColor="hsl(160, 100%, 50%)" />
              </linearGradient>
              <linearGradient id="codex-area-gradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="hsl(180, 100%, 45%)" stopOpacity="0.25" />
                <stop offset="100%" stopColor="hsl(180, 100%, 45%)" stopOpacity="0" />
              </linearGradient>

              <linearGradient id="antigravity-gradient" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0%" stopColor="hsl(280, 95%, 65%)" />
                <stop offset="100%" stopColor="hsl(320, 90%, 70%)" />
              </linearGradient>
              <linearGradient id="antigravity-area-gradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="hsl(280, 95%, 65%)" stopOpacity="0.25" />
                <stop offset="100%" stopColor="hsl(280, 95%, 65%)" stopOpacity="0" />
              </linearGradient>
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
              const val = maxValue * (1 - ratio);
              return (
                <text
                  key={i}
                  x={paddingLeft - 8}
                  y={y + 3}
                  textAnchor="end"
                  className="chart-axis-text"
                >
                  {formatTokens(val)}
                </text>
              );
            })}

            {/* X Axis Line */}
            <line
              x1={paddingLeft}
              y1={chartHeight - paddingBottom}
              x2={chartWidth - paddingRight}
              y2={chartHeight - paddingBottom}
              className="chart-axis-line"
            />

            {/* X Axis Labels */}
            {(() => {
              const N = chartData.length;
              let interval = 2;
              if (N > 15) interval = 5;
              if (N > 31) interval = 10;
              if (N <= 7) interval = 1;

              return chartData.map((d, i) => {
                const isLast = i === N - 1;
                const isTargetInterval = i % interval === 0;
                const hasEnoughSpace = (N - 1 - i) >= interval * 0.8;

                if (!isLast && (!isTargetInterval || !hasEnoughSpace)) {
                  return null;
                }

                const x = paddingLeft + (i / Math.max(N - 1, 1)) * contentWidth;
                const label = chartViewMode === "daily"
                  ? (d.date.length > 5 ? d.date.substring(5) : d.date)
                  : d.date;

                return (
                  <text
                    key={i}
                    x={x}
                    y={chartHeight - paddingBottom + 16}
                    textAnchor="middle"
                    className="chart-axis-text"
                  >
                    {label}
                  </text>
                );
              });
            })()}

            {/* Active Vertical Crosshair Line (가로 점선 가이드라인) */}
            {tooltip.visible && activePoint && (
              <line
                x1={activePoint.x}
                y1={paddingTop}
                x2={activePoint.x}
                y2={chartHeight - paddingBottom}
                stroke="rgba(255, 255, 255, 0.15)"
                strokeWidth="1.5"
                strokeDasharray="4 4"
                pointerEvents="none"
              />
            )}

            {/* Total Tokens (Dashed Background Line) */}
            {totalPath.pathD && (
              <path
                d={totalPath.pathD}
                fill="none"
                stroke="rgba(255, 255, 255, 0.15)"
                strokeWidth="1.5"
                strokeDasharray="4 4"
              />
            )}

            {/* Claude (Anthropic) Line & Area */}
            {claudePath.areaD && <path d={claudePath.areaD} fill="url(#claude-area-gradient)" style={{ opacity: 0.12 }} />}
            {claudePath.pathD && <path d={claudePath.pathD} fill="none" stroke="url(#claude-gradient)" strokeWidth="2.5" />}

            {/* Codex (OpenAI) Line & Area */}
            {codexPath.areaD && <path d={codexPath.areaD} fill="url(#codex-area-gradient)" style={{ opacity: 0.12 }} />}
            {codexPath.pathD && <path d={codexPath.pathD} fill="none" stroke="url(#codex-gradient)" strokeWidth="2.5" />}

            {/* Antigravity (Local) Line & Area */}
            {antigravityPath.areaD && <path d={antigravityPath.areaD} fill="url(#antigravity-area-gradient)" style={{ opacity: 0.12 }} />}
            {antigravityPath.pathD && <path d={antigravityPath.pathD} fill="none" stroke="url(#antigravity-gradient)" strokeWidth="2.5" />}

            {/* Interactive Points circles */}
            {claudePoints.map((p, i) => p.value > 0 && (
              <circle
                key={`claude-${i}`}
                cx={p.x}
                cy={p.y}
                r={3}
                fill="hsl(25, 95%, 55%)"
                stroke="#0a0c10"
                strokeWidth="1.5"
                className="chart-point"
              />
            ))}
            {codexPoints.map((p, i) => p.value > 0 && (
              <circle
                key={`codex-${i}`}
                cx={p.x}
                cy={p.y}
                r={3}
                fill="hsl(180, 100%, 45%)"
                stroke="#0a0c10"
                strokeWidth="1.5"
                className="chart-point"
              />
            ))}
            {antigravityPoints.map((p, i) => p.value > 0 && (
              <circle
                key={`antigravity-${i}`}
                cx={p.x}
                cy={p.y}
                r={3}
                fill="hsl(280, 95%, 65%)"
                stroke="#0a0c10"
                strokeWidth="1.5"
                className="chart-point"
              />
            ))}
          </svg>

          {/* Chart Legends */}
          <div style={{
            display: "flex",
            justifyContent: "center",
            gap: "1.25rem",
            marginTop: "0.75rem",
            fontSize: "0.7rem",
            color: "hsl(215, 20%, 65%)"
          }}>
            <div style={{ display: "flex", alignItems: "center", gap: "0.25rem" }}>
              <span style={{ display: "inline-block", width: "8px", height: "8px", borderRadius: "50%", background: "hsl(25, 95%, 55%)" }} />
              <span>Anthropic (Claude)</span>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "0.25rem" }}>
              <span style={{ display: "inline-block", width: "8px", height: "8px", borderRadius: "50%", background: "hsl(180, 100%, 45%)" }} />
              <span>OpenAI (Codex)</span>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "0.25rem" }}>
              <span style={{ display: "inline-block", width: "8px", height: "8px", borderRadius: "50%", background: "hsl(280, 95%, 65%)" }} />
              <span>Antigravity (Local)</span>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "0.25rem" }}>
              <span style={{ display: "inline-block", width: "12px", height: "2px", borderTop: "2px dashed rgba(255, 255, 255, 0.3)" }} />
              <span>전체 합계 (Total)</span>
            </div>
          </div>

          {/* DOM Interactive Tooltip (퍼센트 기반 스타일 렌더링) */}
          <div
            className="chart-tooltip"
            style={{
              position: "absolute",
              pointerEvents: "none",
              background: "rgba(12, 15, 20, 0.96)",
              border: "1px solid rgba(255, 255, 255, 0.08)",
              borderRadius: "8px",
              padding: "0.6rem 0.8rem",
              boxShadow: "0 4px 20px rgba(0, 0, 0, 0.4)",
              opacity: tooltip.visible ? 1 : 0,
              left: `${tooltip.x}%`,
              top: `${tooltip.y}%`,
              transform: "translate(-50%, -105%)",
              transition: "opacity 0.2s ease, left 0.08s ease, top 0.08s ease",
              zIndex: 100,
              backdropFilter: "blur(6px)",
              minWidth: "160px"
            }}
          >
            <div style={{ fontWeight: 700, color: "#fff", fontSize: "0.75rem", borderBottom: "1px solid rgba(255,255,255,0.08)", paddingBottom: "0.3rem", marginBottom: "0.4rem" }}>
              📅 {tooltip.date}
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "0.3rem", fontSize: "0.7rem" }}>
              <div style={{ display: "flex", justifyContent: "space-between", gap: "1rem" }}>
                <span style={{ color: "hsl(25, 90%, 70%)", display: "flex", alignItems: "center", gap: "0.2rem" }}>
                  <span style={{ width: "6px", height: "6px", borderRadius: "50%", background: "hsl(25, 95%, 55%)" }} />
                  Claude:
                </span>
                <span style={{ fontWeight: 600, color: "#fff" }}>{formatTokens(tooltip.claude || 0)}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", gap: "1rem" }}>
                <span style={{ color: "hsl(180, 90%, 70%)", display: "flex", alignItems: "center", gap: "0.2rem" }}>
                  <span style={{ width: "6px", height: "6px", borderRadius: "50%", background: "hsl(180, 100%, 45%)" }} />
                  Codex:
                </span>
                <span style={{ fontWeight: 600, color: "#fff" }}>{formatTokens(tooltip.codex || 0)}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", gap: "1rem" }}>
                <span style={{ color: "hsl(280, 90%, 75%)", display: "flex", alignItems: "center", gap: "0.2rem" }}>
                  <span style={{ width: "6px", height: "6px", borderRadius: "50%", background: "hsl(280, 95%, 65%)" }} />
                  Local:
                </span>
                <span style={{ fontWeight: 600, color: "#fff" }}>{formatTokens(tooltip.antigravity || 0)}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", gap: "1rem", borderTop: "1px dashed rgba(255,255,255,0.08)", paddingTop: "0.3rem", marginTop: "0.1rem" }}>
                <span style={{ color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>Total:</span>
                <span style={{ fontWeight: 700, color: "var(--neon-blue)" }}>{formatTokens(tooltip.value || 0)}</span>
              </div>
            </div>
          </div>
        </div>
      ) : (
        <div style={{ height: "240px", display: "flex", alignItems: "center", justifyContent: "center", color: "hsl(215, 20%, 40%)", fontSize: "0.85rem" }}>
          기간 내 수집된 에이전트 토큰 데이터가 존재하지 않습니다.
        </div>
      )}
    </section>
  );
}
export default SplineChart;
