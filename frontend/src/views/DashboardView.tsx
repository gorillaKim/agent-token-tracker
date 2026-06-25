import { 
  Session, 
  AgentSummary, 
  LoopDetectionResult, 
  DailyTokenUsage, 
  HourlyTokenUsage, 
  PlanQuotaInfo 
} from "../types";
import { AgentQuotaCard } from "../components/AgentQuotaCard";
import { SplineChart } from "../components/SplineChart";
import { formatCwd, formatTokens } from "../utils/formatters";
import { useState } from "react";

interface DashboardViewProps {
  sessions: Session[];
  summaries: AgentSummary[];
  anomalies: LoopDetectionResult[];
  dailyTokenUsage: DailyTokenUsage[];
  hourlyTokenUsage: HourlyTokenUsage[];
  quotaInfo: PlanQuotaInfo[];
  tokenDisplayMode: string;
  setSelectedSessionId: (id: string | null) => void;
}

/**
 * 대시보드 메인 화면 뷰
 * 
 * 할당량 카드 그리드, 인터랙티브 토큰 사용량 차트, 그리고 활성/오작동 세션 현황판을 렌더링합니다.
 */
export function DashboardView({
  sessions,
  summaries,
  anomalies,
  dailyTokenUsage,
  hourlyTokenUsage,
  quotaInfo,
  tokenDisplayMode,
  setSelectedSessionId,
}: DashboardViewProps) {
  // 에이전트 누적 카드 수동 확장 상태
  const [expandedSummaries, setExpandedSummaries] = useState<Record<string, boolean>>({
    claude_code: false,
    codex: false,
    antigravity: false,
  });

  const toggleSummary = (agentType: string) => {
    setExpandedSummaries(prev => ({
      ...prev,
      [agentType]: !prev[agentType]
    }));
  };

  return (
    <>
      {/* 할당량 및 쿼터 정보 섹션 */}
      <section className="cards-grid">
        {summaries.map((sum) => {
          let providerKey = "antigravity";
          if (sum.agent_type === "claude_code") providerKey = "anthropic";
          else if (sum.agent_type === "codex") providerKey = "openai";

          const quota = quotaInfo.find((q) => q.provider === providerKey);
          return (
            <AgentQuotaCard
              key={sum.agent_type}
              sum={sum}
              quota={quota}
              tokenDisplayMode={tokenDisplayMode}
              isDashboard={true}
              isExpanded={expandedSummaries[sum.agent_type]}
              onToggleExpand={() => toggleSummary(sum.agent_type)}
            />
          );
        })}
      </section>

      {/* 개선된 인터랙티브 차트 컴포넌트 마운트 */}
      <SplineChart 
        dailyTokenUsage={dailyTokenUsage}
        hourlyTokenUsage={hourlyTokenUsage}
      />

      {/* 대시보드 하단: 활성 세션 & 이상 세션 */}
      <div className="bottom-sections">
        <section className="section-card glass">
          <h3 className="section-header">🟢 활성 작업 세션 ({sessions.length})</h3>
          <div className="session-list">
            {sessions.map((s) => (
              <div
                key={s.session_id}
                onClick={() => setSelectedSessionId(s.session_id)}
                className="session-item session-item-clickable"
              >
                <div className="session-meta">
                  <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                    <span className="session-id">{s.session_name || s.session_id.substring(0, 18) + "..."}</span>
                    {s.parent_session_id && (
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
                  </div>
                  <span className="session-agent">{s.agent_type} • {formatCwd(s.cwd)}</span>
                </div>
                <span className="session-tokens">
                  {formatTokens(s.total_input_tokens + s.total_output_tokens)} Tokens
                </span>
              </div>
            ))}
            {sessions.length === 0 && (
              <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "1rem" }}>
                수집된 활성 세션이 없습니다. 로그 감시 경로 설정을 확인해 주세요.
              </div>
            )}
          </div>
        </section>

        <section className="section-card glass">
          <h3 className="section-header" style={{ color: "var(--neon-red)" }}>🚨 오작동 탐지 현황 ({anomalies.length})</h3>
          <div className="anomaly-list">
            {anomalies.map((a) => (
              <div
                key={a.session_id}
                onClick={() => setSelectedSessionId(a.session_id)}
                className="anomaly-item anomaly-item-clickable"
              >
                <span className="anomaly-id">{a.session_id.substring(0, 16)}...</span>
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
    </>
  );
}
