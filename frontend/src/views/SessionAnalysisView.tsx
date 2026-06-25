import { useState, useMemo, useRef } from "react";
import { Session, LoopDetectionResult, SessionAnalysis, TurnTokenUsage } from "../types";
import { formatTokens, formatUsd } from "../utils/formatters";
import { LoopDirectionViewer } from "../components/LoopDirectionViewer";

interface SessionAnalysisViewProps {
  sessions: Session[];
  anomalies: LoopDetectionResult[];
  analysisSessionId: string | null;
  analysisData: SessionAnalysis | null;
  analysisLoading: boolean;
  onSelectSession: (id: string) => void;
  onInterrupt: (agentType: string, cwd: string) => Promise<void>;
  interruptLoading: boolean;
  interruptMessage: string | null;
}

/**
 * 대시보드 내부의 세션 분석(Analysis) 탭 전용 뷰 컴포넌트
 * 
 * 좌측 세션 히스토리 탐색기 및 우측의 심층 분석 데이터(턴별 스택 바, 캐시 도넛, 도구 랭킹, 루프 다이어그램 등)를 렌더링합니다.
 */
export function SessionAnalysisView({
  sessions,
  anomalies,
  analysisSessionId,
  analysisData,
  analysisLoading,
  onSelectSession,
  onInterrupt,
  interruptLoading,
  interruptMessage,
}: SessionAnalysisViewProps) {
  const chartContainerRef = useRef<HTMLDivElement>(null);
  const [hoveredTurn, setHoveredTurn] = useState<TurnTokenUsage | null>(null);
  const [tooltipPos, setTooltipPos] = useState({ x: 0, y: 0 });

  const [filterAgent, setFilterAgent] = useState<string>("all");
  const [filterAnomaly, setFilterAnomaly] = useState<boolean>(false);
  const [sortBy, setSortBy] = useState<"date_desc" | "date_asc" | "tokens_desc" | "tokens_asc">("date_desc");

  const anomalyMap = useMemo(() => new Map(anomalies.map(a => [a.session_id, a])), [anomalies]);

  const filteredSessions = useMemo(() => {
    return sessions
      .filter((s) => {
        if (filterAgent !== "all" && s.agent_type !== filterAgent) return false;
        if (filterAnomaly && !anomalyMap.has(s.session_id)) return false;
        return true;
      })
      .sort((a, b) => {
        if (sortBy === "date_desc") {
          return new Date(b.started_at).getTime() - new Date(a.started_at).getTime();
        } else if (sortBy === "date_asc") {
          return new Date(a.started_at).getTime() - new Date(b.started_at).getTime();
        } else if (sortBy === "tokens_desc") {
          const totalA = a.total_input_tokens + a.total_output_tokens;
          const totalB = b.total_input_tokens + b.total_output_tokens;
          return totalB - totalA;
        } else if (sortBy === "tokens_asc") {
          const totalA = a.total_input_tokens + a.total_output_tokens;
          const totalB = b.total_input_tokens + b.total_output_tokens;
          return totalA - totalB;
        }
        return 0;
      });
  }, [sessions, filterAgent, filterAnomaly, sortBy, anomalyMap]);

  return (
    <div style={{ display: "flex", flex: 1, gap: "1.5rem", height: "calc(100vh - 8rem)", overflow: "hidden" }}>
      {/* 1. 좌측 세션 목록 */}
      <div className="glass" style={{ width: "320px", display: "flex", flexDirection: "column", padding: "1.25rem", overflowY: "auto" }}>
        <h3 style={{ fontSize: "1.1rem", fontWeight: 700, marginBottom: "0.75rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
          <span>📂</span> 세션 히스토리 ({filteredSessions.length}/{sessions.length})
        </h3>

        {/* 필터 및 정렬 컨트롤러 */}
        <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", marginBottom: "1.25rem" }}>
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <select
              value={filterAgent}
              onChange={(e) => setFilterAgent(e.target.value)}
              style={{
                flex: 1,
                padding: "0.4rem 0.5rem",
                borderRadius: "6px",
                background: "rgba(255,255,255,0.03)",
                border: "1px solid rgba(255,255,255,0.1)",
                color: "var(--foreground)",
                fontSize: "0.75rem",
                cursor: "pointer",
                outline: "none"
              }}
            >
              <option value="all" style={{ background: "#0a0c10" }}>모든 제공사</option>
              {Array.from(new Set(sessions.map(s => s.agent_type))).map(type => {
                let displayName = type;
                if (type === "claude_code") displayName = "Claude Code";
                else if (type === "codex") displayName = "Codex (OpenAI)";
                else if (type === "antigravity") displayName = "Antigravity (Local)";
                return (
                  <option key={type} value={type} style={{ background: "#0a0c10" }}>{displayName}</option>
                );
              })}
            </select>

            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value as any)}
              style={{
                flex: 1,
                padding: "0.4rem 0.5rem",
                borderRadius: "6px",
                background: "rgba(255,255,255,0.03)",
                border: "1px solid rgba(255,255,255,0.1)",
                color: "var(--foreground)",
                fontSize: "0.75rem",
                cursor: "pointer",
                outline: "none"
              }}
            >
              <option value="date_desc" style={{ background: "#0a0c10" }}>최신 날짜순</option>
              <option value="date_asc" style={{ background: "#0a0c10" }}>오래된 날짜순</option>
              <option value="tokens_desc" style={{ background: "#0a0c10" }}>토큰 많은순</option>
              <option value="tokens_asc" style={{ background: "#0a0c10" }}>토큰 적은순</option>
            </select>
          </div>

          <label style={{ display: "flex", alignItems: "center", gap: "0.4rem", cursor: "pointer", fontSize: "0.75rem", color: filterAnomaly ? "var(--neon-red)" : "hsl(215, 20%, 65%)", transition: "color 0.2s" }}>
            <input
              type="checkbox"
              checked={filterAnomaly}
              onChange={(e) => setFilterAnomaly(e.target.checked)}
              style={{
                accentColor: "var(--neon-red)",
                cursor: "pointer"
              }}
            />
            <span style={{ fontWeight: 500 }}>🚨 이상 감지 세션만 보기</span>
          </label>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
          {filteredSessions.map((s) => {
            const isSelected = s.session_id === analysisSessionId;
            const hasAnomaly = anomalyMap.has(s.session_id);
            return (
              <div
                key={s.session_id}
                onClick={() => onSelectSession(s.session_id)}
                className={`session-item session-item-clickable ${isSelected ? "active" : ""}`}
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "stretch",
                  gap: "0.35rem",
                  background: isSelected 
                    ? "rgba(139, 92, 246, 0.15)" 
                    : hasAnomaly 
                      ? "rgba(239, 68, 68, 0.05)" 
                      : "rgba(255, 255, 255, 0.02)",
                  borderColor: isSelected 
                    ? "var(--neon-purple)" 
                    : hasAnomaly 
                      ? "rgba(239, 68, 68, 0.3)" 
                      : "rgba(255, 255, 255, 0.05)",
                  borderWidth: "1px",
                  borderStyle: "solid",
                  padding: "0.75rem 1rem",
                  borderRadius: "8px",
                  cursor: "pointer",
                  transition: "all 0.2s ease"
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.25rem" }}>
                  <span 
                    title={s.session_name || s.session_id} 
                    style={{ fontWeight: 700, fontSize: "0.85rem", color: hasAnomaly ? "var(--neon-red)" : "var(--foreground)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: "160px" }}
                  >
                    {s.session_name || s.session_id.substring(0, 12) + "..."}
                  </span>
                  {hasAnomaly && (
                    <span style={{ fontSize: "0.7rem", padding: "0.1rem 0.3rem", borderRadius: "4px", background: "rgba(239, 68, 68, 0.2)", color: "hsl(0, 100%, 75%)", fontWeight: "bold" }}>
                      LOOP
                    </span>
                  )}
                </div>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", display: "flex", alignItems: "center", gap: "0.3rem" }}>
                  <span>{s.agent_type}</span>
                  {s.parent_session_id && (
                    <span style={{ 
                      fontSize: "0.55rem", 
                      padding: "0.02rem 0.25rem", 
                      borderRadius: "3px", 
                      background: "rgba(139, 92, 246, 0.15)", 
                      color: "var(--neon-purple)", 
                      fontWeight: "bold",
                      border: "1px solid rgba(139, 92, 246, 0.2)",
                      whiteSpace: "nowrap"
                    }}>
                      서브에이전트 세션
                    </span>
                  )}
                  <span>• {s.started_at.substring(11, 19)}</span>
                </div>
                <div style={{ display: "flex", justifyContent: "space-between", marginTop: "0.5rem", fontSize: "0.75rem" }}>
                  <span style={{ color: "hsl(215, 20%, 70%)" }}>
                    {formatTokens(s.total_input_tokens + s.total_output_tokens)} Tokens
                  </span>
                </div>
              </div>
            );
          })}
          {filteredSessions.length === 0 && (
            <div style={{ textAlign: "center", padding: "2rem 1rem", color: "hsl(215, 20%, 40%)", fontSize: "0.8rem" }}>
              조건에 부합하는 세션이 없습니다.
            </div>
          )}
        </div>
      </div>

      {/* 2. 우측 분석 상세 패널 */}
      <div className="glass" style={{ flex: 1, display: "flex", flexDirection: "column", padding: "1.5rem", overflowY: "auto" }}>
        {analysisLoading ? (
          <div style={{ display: "flex", flex: 1, flexDirection: "column", justifyContent: "center", alignItems: "center", color: "hsl(215, 20%, 55%)", gap: "1rem" }}>
            <div style={{ width: "32px", height: "32px", border: "3px solid rgba(255,255,255,0.08)", borderTopColor: "var(--neon-blue)", borderRadius: "50%", animation: "rotate-sync 1s linear infinite" }} />
            <span>세션 심층 분석 데이터 수집 중...</span>
          </div>
        ) : analysisData ? (
          <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
            {/* 세션 개요 헤더 */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", borderBottom: "1px solid rgba(255,255,255,0.08)", paddingBottom: "1rem" }}>
              <div>
                <h2 style={{ fontSize: "1.3rem", fontWeight: 800, margin: 0, display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <span>🔍</span> 세션 분석 보고서
                  {sessions.find(s => s.session_id === analysisSessionId)?.parent_session_id && (
                    <span style={{ 
                      fontSize: "0.65rem", 
                      padding: "0.15rem 0.5rem", 
                      borderRadius: "6px", 
                      background: "rgba(139, 92, 246, 0.2)", 
                      color: "var(--neon-purple)", 
                      fontWeight: "bold",
                      border: "1px solid rgba(139, 92, 246, 0.3)",
                      display: "inline-flex",
                      alignItems: "center"
                    }}>
                      서브에이전트 세션
                    </span>
                  )}
                </h2>
                <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 50%)", fontFamily: "monospace", marginTop: "0.25rem" }}>
                  {analysisData.session_name && (
                    <span style={{ color: "var(--foreground)", fontWeight: 700, marginRight: "1rem" }}>
                      {analysisData.session_name}
                    </span>
                  )}
                  ID: {analysisData.session_id}
                </div>
              </div>

              <div style={{ display: "flex", gap: "0.75rem", alignItems: "center" }}>
                <span className="pulse-badge" style={{
                  color: analysisData.is_anomaly ? "var(--neon-red)" : "var(--neon-blue)",
                  background: analysisData.is_anomaly ? "rgba(239, 68, 68, 0.1)" : "rgba(6, 182, 212, 0.1)",
                  borderColor: analysisData.is_anomaly ? "rgba(239, 68, 68, 0.3)" : "rgba(6, 182, 212, 0.2)"
                }}>
                  {analysisData.is_anomaly ? "🚨 위험 상태 감지" : "✓ 안전 세션"}
                </span>
                <button
                  onClick={() => onInterrupt(analysisData.agent_type, "")}
                  disabled={interruptLoading}
                  className="btn btn-delete"
                  style={{ padding: "0.4rem 1rem", fontSize: "0.8rem" }}
                >
                  {interruptLoading ? "중단 중..." : "에이전트 강제종료"}
                </button>
              </div>
            </div>

            {/* 기본 수치 요약 */}
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "1rem" }}>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>총 소비 비용</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "var(--neon-purple)", marginTop: "0.25rem", textShadow: "0 0 10px rgba(139, 92, 246, 0.3)" }}>
                  ${formatUsd(analysisData.total_cost_usd)}
                </div>
              </div>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>총 사용 토큰</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "var(--neon-blue)", marginTop: "0.25rem" }}>
                  {formatTokens(analysisData.total_input_tokens + analysisData.total_output_tokens)}
                </div>
              </div>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>캐시 히트율</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "var(--neon-green)", marginTop: "0.25rem" }}>
                  {(analysisData.cache_hit_rate * 100).toFixed(1)}%
                </div>
              </div>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>캐시 절감 비용</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "hsl(150, 100%, 40%)", marginTop: "0.25rem" }}>
                  ${formatUsd(analysisData.cache_saved_cost)}
                </div>
              </div>
            </div>

            {/* 턴별 토큰 소비 스택 바 차트 */}
            <div ref={chartContainerRef} style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1.25rem", borderRadius: "12px", position: "relative" }}>
              <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 1rem 0" }}>⚡ 턴별 토큰 소비 분석 (턴 순서 흐름)</h4>
              
              {analysisData.turns.length > 0 ? (
                <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
                  {/* Legend */}
                  <div style={{ display: "flex", gap: "1rem", fontSize: "0.75rem", justifyContent: "flex-end" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                      <div style={{ width: "10px", height: "10px", background: "var(--neon-blue)", borderRadius: "2px" }} />
                      <span>Input Tokens</span>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                      <div style={{ width: "10px", height: "10px", background: "var(--neon-purple)", borderRadius: "2px" }} />
                      <span>Output Tokens</span>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                      <div style={{ width: "10px", height: "10px", background: "var(--neon-green)", borderRadius: "2px" }} />
                      <span>Cache Read Tokens</span>
                    </div>
                  </div>

                  {/* Scrollable Container */}
                  <div 
                    style={{ 
                      overflowX: "auto", 
                      marginTop: "0.5rem",
                      paddingBottom: "0.5rem"
                    }}
                  >
                    <div style={{ display: "flex", flexDirection: "column", minWidth: "max-content", gap: "0.5rem" }}>
                      
                      {/* Bars container */}
                      <div 
                        style={{ 
                          display: "flex", 
                          alignItems: "flex-end", 
                          height: "180px", 
                          gap: "0.4rem", 
                          borderBottom: "1px solid rgba(255,255,255,0.08)",
                          paddingBottom: "0.5rem"
                        }}
                      >
                        {analysisData.turns.map((turn, idx) => {
                          const turnTotal = turn.input_tokens + turn.output_tokens;
                          const maxTurnTotal = Math.max(...analysisData.turns.map(t => t.input_tokens + t.output_tokens), 1);
                          const barHeightPct = (turnTotal / maxTurnTotal) * 100;
                          
                          const inputPct = turnTotal > 0 ? (turn.input_tokens / turnTotal) * 100 : 0;
                          const outputPct = turnTotal > 0 ? (turn.output_tokens / turnTotal) * 100 : 0;
                          const cachePct = turn.input_tokens > 0 ? (turn.cache_read_tokens / turn.input_tokens) * 100 : 0;

                          return (
                            <div
                              key={idx}
                              style={{
                                flex: "0 0 28px",
                                height: `${barHeightPct}%`,
                                minHeight: "10px",
                                display: "flex",
                                flexDirection: "column-reverse",
                                borderRadius: "4px 4px 0 0",
                                overflow: "hidden",
                                cursor: "pointer",
                                transition: "opacity 0.2s ease",
                                background: "rgba(255,255,255,0.02)"
                              }}
                              onMouseEnter={(e) => {
                                setHoveredTurn(turn);
                                const rect = e.currentTarget.getBoundingClientRect();
                                const containerRect = chartContainerRef.current?.getBoundingClientRect();
                                if (containerRect) {
                                  setTooltipPos({
                                    x: rect.left - containerRect.left + 14,
                                    y: rect.top - containerRect.top - 10
                                  });
                                }
                              }}
                              onMouseLeave={() => setHoveredTurn(null)}
                            >
                              {/* Input Section */}
                              <div style={{ height: `${inputPct}%`, background: "var(--neon-blue)", position: "relative" }}>
                                {/* Cache section embedded inside input */}
                                {cachePct > 0 && (
                                  <div style={{ position: "absolute", bottom: 0, left: 0, right: 0, height: `${cachePct}%`, background: "var(--neon-green)" }} />
                                )}
                              </div>
                              {/* Output Section */}
                              <div style={{ height: `${outputPct}%`, background: "var(--neon-purple)" }} />
                            </div>
                          );
                        })}
                      </div>

                      {/* X Axis Labels */}
                      <div style={{ display: "flex", gap: "0.4rem", fontSize: "0.65rem", color: "hsl(215, 20%, 50%)" }}>
                        {analysisData.turns.map((turn, idx) => (
                          <div key={idx} style={{ flex: "0 0 28px", textAlign: "center" }}>
                            T{turn.turn_index}
                          </div>
                        ))}
                      </div>

                    </div>
                  </div>

                  {/* Interactive Tooltip inside container */}
                  {hoveredTurn && (
                    <div
                      style={{
                        position: "absolute",
                        left: `${tooltipPos.x}px`,
                        top: `${tooltipPos.y}px`,
                        transform: "translate(-50%, -100%)",
                        background: "rgba(10, 12, 16, 0.95)",
                        border: "1px solid var(--neon-blue)",
                        borderRadius: "6px",
                        padding: "0.5rem 0.75rem",
                        fontSize: "0.72rem",
                        boxShadow: "0 0 10px rgba(0, 242, 254, 0.2)",
                        pointerEvents: "none",
                        zIndex: 100,
                        transition: "all 0.1s ease"
                      }}
                    >
                      <div style={{ fontWeight: 700, color: "var(--neon-blue)", marginBottom: "4px" }}>
                        Turn {hoveredTurn.turn_index} ({hoveredTurn.role})
                      </div>
                      <div style={{ color: "#fff" }}>
                        입력: <span style={{ fontWeight: 600 }}>{hoveredTurn.input_tokens.toLocaleString()}</span>
                      </div>
                      <div style={{ color: "#fff" }}>
                        출력: <span style={{ fontWeight: 600 }}>{hoveredTurn.output_tokens.toLocaleString()}</span>
                      </div>
                      {hoveredTurn.cache_read_tokens > 0 && (
                        <div style={{ color: "var(--neon-green)" }}>
                          캐시 리드: <span style={{ fontWeight: 600 }}>{hoveredTurn.cache_read_tokens.toLocaleString()}</span>
                        </div>
                      )}
                      <div style={{ color: "var(--neon-purple)", fontWeight: 600, marginTop: "4px" }}>
                        비용: ${formatUsd(hoveredTurn.cost_usd)}
                      </div>
                    </div>
                  )}
                </div>
              ) : (
                <div style={{ padding: "2rem", textAlign: "center", color: "hsl(215, 20%, 40%)", fontSize: "0.85rem" }}>
                  턴별 토큰 사용 내역이 없습니다.
                </div>
              )}
            </div>

            {/* 하단 2단: 캐시 도넛 차트 & 도구 비용 랭킹 */}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1.5rem" }}>
              {/* 캐시 히트율 도넛 차트 카드 */}
              <div className="glass" style={{ padding: "1.25rem", background: "rgba(255,255,255,0.01)", position: "relative", zIndex: 10 }}>
                <div style={{ display: "flex", alignItems: "center", gap: "0.3rem", margin: "0 0 1rem 0" }}>
                  <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: 0 }}>🔌 캐시 효율성 & 히트율</h4>
                  <div className="tooltip-container">
                    <span style={{ cursor: "help", fontSize: "0.75rem", opacity: 0.5 }}>ℹ️</span>
                    <div className="tooltip-text" style={{ bottom: "125%", left: "0", transform: "none", width: "285px" }}>
                      캐시가 탑재된 Claude 모델 사용 시 입력 토큰을 최대 90% 저렴하게 처리하여 예산을 대폭 절감합니다.
                    </div>
                  </div>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: "2rem" }}>
                  {/* SVG 도넛 */}
                  <div style={{ position: "relative", width: "120px", height: "120px" }}>
                    <svg width="100%" height="100%" viewBox="0 0 42 42">
                      <circle cx="21" cy="21" r="15.915" fill="transparent" stroke="rgba(255,255,255,0.03)" strokeWidth="4" />
                      <circle
                        cx="21"
                        cy="21"
                        r="15.915"
                        fill="transparent"
                        stroke="var(--neon-green)"
                        strokeWidth="4"
                        strokeDasharray={`${analysisData.cache_hit_rate * 100} ${100 - (analysisData.cache_hit_rate * 100)}`}
                        strokeDashoffset="25"
                        style={{ filter: "drop-shadow(0 0 4px rgba(16, 185, 129, 0.4))", transition: "stroke-dasharray 0.5s ease" }}
                      />
                    </svg>
                    <div style={{ position: "absolute", top: "50%", left: "50%", transform: "translate(-50%, -50%)", textAlign: "center" }}>
                      <div style={{ fontSize: "1.1rem", fontWeight: 800, color: "var(--foreground)" }}>
                        {(analysisData.cache_hit_rate * 100).toFixed(0)}%
                      </div>
                      <div style={{ fontSize: "0.6rem", color: "hsl(215, 20%, 50%)" }}>HIT RATE</div>
                    </div>
                  </div>

                  <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", flex: 1 }}>
                    <div style={{ background: "linear-gradient(135deg, rgba(16, 185, 129, 0.1), rgba(6, 182, 212, 0.05))", border: "1px solid rgba(16, 185, 129, 0.15)", borderRadius: "8px", padding: "0.5rem 0.75rem" }}>
                      <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 65%)" }}>누적 캐시 사용량</div>
                      <div style={{ fontSize: "0.95rem", fontWeight: 700, color: "var(--neon-green)", marginTop: "0.15rem" }}>
                        {formatTokens(analysisData.total_cache_read_tokens)} Tokens
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              {/* 도구 비용 랭킹 카드 */}
              <div className="glass" style={{ padding: "1.25rem", background: "rgba(255,255,255,0.01)", position: "relative", zIndex: 10 }}>
                <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 1rem 0" }}>🛠️ 도구별 비용 랭킹</h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", maxHeight: "160px", overflowY: "auto", paddingRight: "0.25rem" }}>
                  {analysisData.tool_cost_rank.map((t) => {
                    const maxCost = Math.max(...analysisData.tool_cost_rank.map(tc => tc.total_cost_usd), 0.0001);
                    const barWidth = (t.total_cost_usd / maxCost) * 100;
                    return (
                      <div key={t.tool_name} style={{ display: "flex", flexDirection: "column", gap: "0.2rem" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "0.75rem", fontSize: "0.75rem" }}>
                          <div className="tooltip-container" style={{ flex: 1, minWidth: 0, display: "flex", justifyContent: "flex-start" }}>
                            <span 
                              style={{ 
                                fontWeight: 600, 
                                color: "hsl(215, 20%, 85%)", 
                                fontFamily: "monospace",
                                whiteSpace: "nowrap",
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                                flex: 1,
                                minWidth: 0,
                                maxWidth: "200px",
                                textAlign: "left",
                                cursor: "help"
                              }}
                            >
                              {t.tool_name}
                            </span>
                            <div className="tooltip-text" style={{ bottom: "125%", left: "0", transform: "none", width: "300px", wordBreak: "break-all" }}>
                              <b>도구명 전체 식별자</b>:<br/>
                              {t.tool_name}
                            </div>
                          </div>
                          <span style={{ fontWeight: 700, color: "var(--neon-blue)", flexShrink: 0, whiteSpace: "nowrap" }}>
                            ${formatUsd(t.total_cost_usd)} ({t.call_count}회)
                          </span>
                        </div>
                        <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden" }}>
                          <div style={{ height: "100%", width: `${barWidth}%`, background: "var(--neon-blue)", borderRadius: "3px" }} />
                        </div>
                      </div>
                    );
                  })}
                  {analysisData.tool_cost_rank.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", fontSize: "0.8rem", textAlign: "center", padding: "1.5rem" }}>
                      이 세션에 도구 호출 정보가 없습니다.
                    </div>
                  )}
                </div>
              </div>
            </div>

            {/* 이상 제어 / 시각화 디렉션 */}
            <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1.25rem", borderRadius: "12px" }}>
              <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 0.75rem 0", color: analysisData.is_anomaly ? "var(--neon-red)" : "var(--neon-green)" }}>
                {analysisData.is_anomaly ? "🚨 오작동 이상 탐지 분석" : "✓ 세션 이상 탐지 분석"}
              </h4>
              {analysisData.is_anomaly ? (
                <div>
                  <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 85%)", marginBottom: "1rem", lineHeight: 1.5 }}>
                    {analysisData.anomaly_signals.map((s, idx) => (
                      <div key={idx} style={{ padding: "0.5rem 0.75rem", background: "rgba(239, 68, 68, 0.06)", borderLeft: "3px solid var(--neon-red)", borderRadius: "0 6px 6px 0", marginBottom: "0.5rem" }}>
                        <strong>{s.signal_type === "repeated_call" ? "자가 루프 의심" : "핑퐁 순환 호출"}:</strong> {s.description}
                      </div>
                    ))}
                  </div>
                  <LoopDirectionViewer signals={analysisData.anomaly_signals} />
                </div>
              ) : (
                <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 70%)" }}>
                  현재 세션에서 동일한 도구 호출의 오작동 순환(Loop) 현상이나 급격한 토큰 폭팽 이상 징후가 검출되지 않았습니다. 안전하게 관리되고 있습니다.
                </div>
              )}
            </div>
            
            {interruptMessage && (
              <div style={{ padding: "0.75rem 1rem", background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.05)", borderRadius: "8px", fontSize: "0.75rem" }}>
                {interruptMessage}
              </div>
            )}
          </div>
        ) : (
          <div style={{ display: "flex", flex: 1, flexDirection: "column", justifyContent: "center", alignItems: "center", color: "hsl(215, 20%, 40%)" }}>
            <span>👈 왼쪽 히스토리 목록에서 분석할 세션을 선택해 주세요.</span>
          </div>
        )}
      </div>
    </div>
  );
}
export default SessionAnalysisView;
