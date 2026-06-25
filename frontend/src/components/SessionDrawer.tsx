import { Session, LoopDetectionResult, SessionDetails } from "../types";
import { formatCwd, formatTokens, formatUsd } from "../utils/formatters";
import { LoopDirectionViewer } from "./LoopDirectionViewer";

interface SessionDrawerProps {
  selectedSessionId: string | null;
  setSelectedSessionId: (id: string | null) => void;
  sessions: Session[];
  anomalies: LoopDetectionResult[];
  sessionDetails: SessionDetails | null;
  detailsLoading: boolean;
  interruptLoading: boolean;
  interruptMessage: string | null;
  onInterrupt: (agentType: string, cwd: string) => Promise<void>;
}

/**
 * 세션 상세 정보 디버깅을 위한 우측 사이드 드로어 패널 컴포넌트
 * 
 * 낭비된 리스크 비용 경고 배지 노출, 도구 호출 상세 타임라인 트래킹, 
 * 그리고 오작동 에이전트에 대한 수동 인터럽트(강제 종료)를 수행할 수 있습니다.
 */
export function SessionDrawer({
  selectedSessionId,
  setSelectedSessionId,
  sessions,
  anomalies,
  sessionDetails,
  detailsLoading,
  interruptLoading,
  interruptMessage,
  onInterrupt,
}: SessionDrawerProps) {
  const selectedSess = sessions.find((s) => s.session_id === selectedSessionId);
  const selectedAnomaly = anomalies.find((a) => a.session_id === selectedSessionId);

  // 낭비 비용(Cost Waste)을 간이 추산합니다. (루핑 또는 실패한 도구 호출 비용 합산)
  let costWasteVal = 0;
  if (sessionDetails) {
    const loopingTools = new Set<string>();
    if (selectedAnomaly) {
      for (const s of selectedAnomaly.signals) {
        if (s.signal_type === "ping_pong") {
          const parts = s.evidence.split(",").map((p) => p.trim());
          for (const p of parts) {
            if (p.startsWith("tool_A=")) loopingTools.add(p.substring(7));
            if (p.startsWith("tool_B=")) loopingTools.add(p.substring(7));
          }
        } else if (s.signal_type === "repeated_call") {
          const parts = s.evidence.split(",").map((p) => p.trim());
          for (const p of parts) {
            if (p.startsWith("tool_name=")) loopingTools.add(p.substring(10));
          }
        }
      }
    }

    for (const tc of sessionDetails.tool_calls) {
      if (!tc.success || loopingTools.has(tc.tool_name)) {
        costWasteVal += tc.cost_usd || 0;
      }
    }
  }

  return (
    <>
      <div
        className={`drawer-overlay ${selectedSessionId ? "open" : ""}`}
        onClick={() => setSelectedSessionId(null)}
      />
      <div className={`drawer ${selectedSessionId ? "open" : ""}`}>
        <button className="drawer-close-btn" onClick={() => setSelectedSessionId(null)}>✕</button>
        {selectedSess ? (
          <>
            <h3 className="drawer-title">세션 상세 디버거</h3>
            <div className="drawer-subtitle" style={{ fontSize: "0.95rem", color: "var(--foreground)", fontWeight: 700, marginBottom: "0.2rem" }}>
              {selectedSess.session_name || "이름 없음"}
            </div>
            <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 55%)", fontFamily: "monospace", marginBottom: "1.2rem" }}>
              ID: {selectedSess.session_id}
            </div>

            {/* 낭비 비용(Cost Waste) 경고 배지 노출 */}
            {selectedAnomaly && (
              <div
                style={{
                  padding: "1rem",
                  background: "rgba(239, 68, 68, 0.1)",
                  border: "1px solid rgba(239, 68, 68, 0.3)",
                  borderRadius: "8px",
                  color: "hsl(0, 100%, 75%)",
                  marginBottom: "1.5rem",
                  boxShadow: "0 0 15px rgba(239, 68, 68, 0.1)",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <span style={{ fontWeight: 700, fontSize: "0.85rem" }}>⚠️ 낭비 비용 (Cost Waste)</span>
                  <span style={{ fontWeight: 800, fontSize: "1.1rem", textShadow: "0 0 8px rgba(239, 68, 68, 0.5)" }}>
                    ${formatUsd(costWasteVal)} USD
                  </span>
                </div>
                <div style={{ fontSize: "0.75rem", marginTop: "0.25rem", color: "hsl(215, 20%, 75%)" }}>
                  루프 오작동 및 도구 호출 실패로 낭비된 비용이 실시간 추적되었습니다.
                </div>
              </div>
            )}

            <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", marginBottom: "1.5rem" }}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>에이전트 타입</span>
                <span style={{ fontWeight: 600, display: "flex", alignItems: "center", gap: "0.3rem" }}>
                  {selectedSess.agent_type}
                  {selectedSess.parent_session_id && (
                    <span style={{ 
                      fontSize: "0.6rem", 
                      padding: "0.05rem 0.3rem", 
                      borderRadius: "4px", 
                      background: "rgba(139, 92, 246, 0.2)", 
                      color: "var(--neon-purple)", 
                      fontWeight: "bold",
                      border: "1px solid rgba(139, 92, 246, 0.3)",
                      whiteSpace: "nowrap"
                    }}>
                      서브에이전트 세션
                    </span>
                  )}
                </span>
              </div>
              {selectedSess.parent_session_id && (
                <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                  <span style={{ color: "hsl(215, 20%, 55%)" }}>상위 세션 ID</span>
                  <span style={{ fontWeight: 600, fontFamily: "monospace", fontSize: "0.75rem" }}>{selectedSess.parent_session_id.substring(0, 16)}...</span>
                </div>
              )}
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>작업 경로 (CWD)</span>
                <span style={{ fontWeight: 600, fontFamily: "monospace", fontSize: "0.75rem" }}>{formatCwd(selectedSess.cwd)}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>사용 모델 ID</span>
                <span style={{ fontWeight: 600 }}>{selectedSess.model_id || "알 수 없음"}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>누적 토큰 사용</span>
                <span style={{ fontWeight: 600, color: "var(--neon-blue)" }}>
                  {formatTokens(selectedSess.total_input_tokens + selectedSess.total_output_tokens)} Tokens
                </span>
              </div>
            </div>

            {/* 이상 징후 시각화 컴포넌트 임베드 */}
            {selectedAnomaly && (
              <div style={{ marginBottom: "1.5rem" }}>
                <h4 style={{ fontSize: "1rem", color: "var(--neon-red)", marginBottom: "0.5rem" }}>🚨 감지된 이상 징후 분석</h4>
                <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 80%)", marginBottom: "1rem" }}>
                  {selectedAnomaly.signals.map((s, idx) => (
                    <div key={idx} style={{ marginBottom: "0.5rem", padding: "0.5rem", background: "rgba(239,68,68,0.05)", borderRadius: "6px", border: "1px solid rgba(239,68,68,0.15)" }}>
                      {s.description}
                    </div>
                  ))}
                </div>
                <LoopDirectionViewer signals={selectedAnomaly.signals} />
              </div>
            )}

            {/* 도구 호출 상세 기록 (비동기 데이터) */}
            {detailsLoading ? (
              <div style={{ textAlign: "center", padding: "2rem", color: "hsl(215, 20%, 50%)" }}>
                세션의 도구 호출 및 상세 히스토리 조회 중...
              </div>
            ) : sessionDetails ? (
              <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
                <h4 style={{ fontSize: "1rem", borderBottom: "1px solid var(--card-border)", paddingBottom: "0.5rem", marginBottom: "0.75rem" }}>
                  도구 호출 타임라인 ({sessionDetails.tool_calls.length}건)
                </h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.6rem", overflowY: "auto", flex: 1, paddingRight: "4px" }}>
                  {sessionDetails.tool_calls.map((tc, idx) => (
                    <div
                      key={idx}
                      style={{
                        padding: "0.75rem",
                        background: "rgba(255, 255, 255, 0.02)",
                        borderRadius: "8px",
                        border: "1px solid rgba(255, 255, 255, 0.04)",
                        fontSize: "0.8rem",
                      }}
                    >
                      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "0.25rem" }}>
                        <span style={{ fontWeight: 700, color: tc.success ? "var(--neon-blue)" : "var(--neon-red)" }}>
                          {tc.tool_name}
                        </span>
                        <span style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)" }}>
                          {tc.created_at.substring(11, 19)}
                        </span>
                      </div>
                      <div
                        style={{
                          fontFamily: "monospace",
                          fontSize: "0.72rem",
                          color: "hsl(215, 20%, 75%)",
                          background: "rgba(0, 0, 0, 0.2)",
                          padding: "0.4rem",
                          borderRadius: "4px",
                          overflowX: "auto",
                          whiteSpace: "pre-wrap",
                          wordBreak: "break-all",
                          maxHeight: "80px",
                        }}
                      >
                        {tc.tool_input}
                      </div>
                    </div>
                  ))}
                  {sessionDetails.tool_calls.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "2rem" }}>
                      이 세션에서 호출된 도구 이력이 없습니다.
                    </div>
                  )}
                </div>
              </div>
            ) : null}

            {/* 이상 제어 Interrupt Action */}
            <div style={{ marginTop: "1.5rem", borderTop: "1px solid var(--card-border)", paddingTop: "1.25rem" }}>
              <h5 style={{ fontSize: "0.85rem", color: "hsl(215, 20%, 55%)", margin: "0 0 0.75rem 0", textTransform: "uppercase" }}>
                위험 관리 및 이상 제어
              </h5>
              <button
                onClick={() => onInterrupt(selectedSess.agent_type, selectedSess.cwd)}
                disabled={interruptLoading}
                style={{
                  width: "100%",
                  padding: "0.75rem 1rem",
                  background: "rgba(239, 68, 68, 0.15)",
                  border: "1px solid rgba(239, 68, 68, 0.4)",
                  borderRadius: "8px",
                  color: "hsl(0, 100%, 75%)",
                  fontWeight: 700,
                  fontSize: "0.85rem",
                  cursor: "pointer",
                  transition: "all 0.2s ease",
                  boxShadow: "0 0 10px rgba(239, 68, 68, 0.1)",
                }}
                onMouseOver={(e) => {
                  e.currentTarget.style.background = "rgba(239, 68, 68, 0.25)";
                  e.currentTarget.style.boxShadow = "0 0 15px rgba(239, 68, 68, 0.3)";
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.background = "rgba(239, 68, 68, 0.15)";
                  e.currentTarget.style.boxShadow = "0 0 10px rgba(239, 68, 68, 0.1)";
                }}
              >
                {interruptLoading ? "인터럽트 신호 송신 중..." : "에이전트 강제 종료 (Interrupt)"}
              </button>

              {interruptMessage && (
                <div
                  style={{
                    marginTop: "0.75rem",
                    padding: "0.5rem 0.75rem",
                    background: "rgba(255, 255, 255, 0.03)",
                    border: "1px solid rgba(255, 255, 255, 0.05)",
                    borderRadius: "6px",
                    fontSize: "0.75rem",
                    color: "hsl(215, 20%, 80%)",
                  }}
                >
                  {interruptMessage}
                </div>
              )}
            </div>
          </>
        ) : (
          <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "5rem 0" }}>
            세션을 선택하면 디버깅 패널이 활성화됩니다.
          </div>
        )}
      </div>
    </>
  );
}
export default SessionDrawer;
