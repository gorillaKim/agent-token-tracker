import { AgentSummary, PlanQuotaInfo } from "../types";
import { formatTokens, formatUsd, formatResetTime } from "../utils/formatters";

interface AgentQuotaCardProps {
  sum: AgentSummary;
  quota: PlanQuotaInfo | undefined;
  tokenDisplayMode: string;
  isDashboard?: boolean;
  isExpanded: boolean;
  onToggleExpand: () => void;
}

/**
 * 에이전트 쿼터 할당량 상태 카드 컴포넌트
 * 
 * 5시간 롤링 사용량, 누적 소비 토큰, 환산 비용(USD) 등을 모니터링 게이지 바와 함께 노출합니다.
 */
export function AgentQuotaCard({
  sum,
  quota,
  tokenDisplayMode,
  isDashboard = false,
  isExpanded,
  onToggleExpand,
}: AgentQuotaCardProps) {

  let barGradient = "linear-gradient(90deg, var(--neon-blue), var(--neon-purple))";
  let remainingColor = "var(--neon-blue)";
  let agentName = "Antigravity";

  if (sum.agent_type === "claude_code") {
    barGradient = "linear-gradient(90deg, var(--neon-blue), var(--neon-purple))";
    remainingColor = "var(--neon-blue)";
    agentName = "Claude Code";
  } else if (sum.agent_type === "codex") {
    barGradient = "linear-gradient(90deg, var(--neon-purple), #9b51e0)";
    remainingColor = "var(--neon-purple)";
    agentName = "Codex (OpenAI)";
  } else {
    barGradient = "linear-gradient(90deg, var(--neon-green), #00e676)";
    remainingColor = "var(--neon-green)";
    agentName = "Antigravity";
  }

  const isPercentage = tokenDisplayMode === "percentage";

  let headerRemainingLabel = "-";
  let displayPct = 0;
  let displayWeeklyPct = 0;

  if (quota) {
    displayPct = Math.min(100, Math.round(quota.usage_pct));
    if (sum.agent_type === "claude_code") {
      displayWeeklyPct = quota.weekly_usage_pct ? Math.min(100, Math.round(quota.weekly_usage_pct)) : 0;
      
      if (quota.weekly_quota_tokens && quota.weekly_quota_tokens > 900_000_000_000_000) {
        headerRemainingLabel = "잔여: 무제한";
      } else {
        headerRemainingLabel = isPercentage 
          ? `잔여: ${100 - displayWeeklyPct}%` 
          : `잔여: ${formatTokens(quota.weekly_remaining_tokens || 0)}`;
      }
    } else {
      displayWeeklyPct = quota.weekly_usage_pct ? Math.min(100, Math.round(quota.weekly_usage_pct)) : 0;
      
      if (quota.quota_tokens > 900_000_000_000_000) {
        headerRemainingLabel = "잔여: 무제한";
      } else {
        headerRemainingLabel = isPercentage 
          ? `잔여: ${100 - displayPct}%` 
          : `잔여: ${formatTokens(quota.remaining_tokens)}`;
      }
    }
  }

  let reset1 = "";
  let reset2 = "";

  const getMonthRemainingTime = (): string => {
    const now = new Date();
    const nextMonth = new Date(now.getFullYear(), now.getMonth() + 1, 1);
    const diffMs = nextMonth.getTime() - now.getTime();
    const days = Math.floor(diffMs / 86400000);
    const hrs = Math.floor((diffMs % 86400000) / 3600000);
    return `${days}d ${hrs}h 후 초기화`;
  };

  if (sum.agent_type === "claude_code") {
    reset1 = formatResetTime(quota?.window_reset_at) || "롤링 대기 중";
    reset2 = formatResetTime(quota?.weekly_reset_at) || "롤링 대기 중";
  } else if (sum.agent_type === "codex") {
    reset1 = formatResetTime(quota?.window_reset_at) || "롤링 대기 중";
    reset2 = getMonthRemainingTime();
  } else {
    reset1 = formatResetTime(quota?.window_reset_at) || "실시간 대기 중";
    reset2 = "실시간 롤링 중";
  }

  return (
    <div className="agent-quota-card glass" style={{ padding: isDashboard ? "1.5rem" : "0.85rem", display: "flex", flexDirection: "column", gap: isDashboard ? "1rem" : "0.6rem", borderRadius: isDashboard ? "16px" : "10px", background: isDashboard ? "" : "rgba(255, 255, 255, 0.015)", border: isDashboard ? "" : "1px solid rgba(255, 255, 255, 0.05)" }}>
      
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div style={{ display: "flex", alignItems: "baseline", gap: "0.4rem" }}>
          <span className="tray-popover-agent-name" style={{ fontWeight: 700, fontSize: isDashboard ? "1.1rem" : "0.85rem", color: remainingColor }}>
            {agentName}
          </span>
          {!isDashboard && (
            <span style={{ fontSize: "0.65rem", color: "hsl(215, 20%, 50%)" }}>
              ({sum.session_count} Sessions)
            </span>
          )}
        </div>
        <span style={{ fontSize: isDashboard ? "0.8rem" : "0.75rem", fontWeight: "600", color: "hsl(215, 20%, 65%)" }}>
          {headerRemainingLabel}
        </span>
      </div>

      {quota && (
        <div style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: isDashboard ? "0.75rem" : "0.7rem", color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>
            <span>{sum.agent_type === "claude_code" || sum.agent_type === "codex" ? "세션 사용량 (5시간 롤링)" : "세션 사용량 (일간 한도)"}</span>
            <span style={{ fontWeight: "700" }}>
              {quota.quota_tokens > 900_000_000_000_000
                ? "무제한"
                : isPercentage 
                  ? `${displayPct}%` 
                  : `${formatTokens(quota.used_tokens)}`
              }
            </span>
          </div>
          <div style={{ height: isDashboard ? "6px" : "4px", background: "rgba(255,255,255,0.03)", borderRadius: "2px", overflow: "hidden", position: "relative" }}>
            <div style={{ height: "100%", width: `${displayPct}%`, background: barGradient, borderRadius: "2px", transition: "width 0.5s ease-out" }} />
          </div>
          <div style={{ fontSize: "0.6rem", color: "hsl(215, 20%, 50%)", textAlign: "right" }}>
            {reset1}
          </div>
        </div>
      )}

      {quota && quota.weekly_quota_tokens !== undefined && quota.weekly_quota_tokens !== null && (
        <div style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: isDashboard ? "0.75rem" : "0.7rem", color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>
            <span>{sum.agent_type === "claude_code" ? "모든 모델 (주간)" : sum.agent_type === "codex" ? "모든 모델 (월간)" : "모든 모델 (주간)"}</span>
            <span style={{ fontWeight: "700" }}>
              {quota.weekly_quota_tokens > 900_000_000_000_000
                ? "무제한"
                : isPercentage 
                  ? `${displayWeeklyPct}%` 
                  : `${formatTokens(quota.weekly_used_tokens || 0)}`
              }
            </span>
          </div>
          <div style={{ height: isDashboard ? "6px" : "4px", background: "rgba(255,255,255,0.03)", borderRadius: "2px", overflow: "hidden", position: "relative" }}>
            <div style={{ height: "100%", width: `${displayWeeklyPct}%`, background: barGradient, borderRadius: "2px", transition: "width 0.5s ease-out" }} />
          </div>
          <div style={{ fontSize: "0.6rem", color: "hsl(215, 20%, 50%)", textAlign: "right" }}>
            {reset2}
          </div>
        </div>
      )}

      {isDashboard && (
        <div className="quota-summary-accordion">
          <button 
            className="accordion-header-btn" 
            onClick={onToggleExpand}
          >
            <span>{isExpanded ? "▼" : "▶"}</span>
            <span>누적 사용량 및 비용 정보</span>
          </button>
          <div className={`accordion-content ${isExpanded ? "expanded" : ""}`}>
            <div className="quota-stats-grid">
              <div className="quota-stat-item">
                <span className="quota-stat-label">활성 세션</span>
                <span className="quota-stat-value">{sum.session_count} 건</span>
              </div>
              <div className="quota-stat-item">
                <span className="quota-stat-label">총 입/출력 토큰</span>
                <span className="quota-stat-value">
                  {sum.total_input_tokens + sum.total_output_tokens > 0 
                    ? formatTokens(sum.total_input_tokens + sum.total_output_tokens)
                    : "-"}
                </span>
              </div>
              <div className="quota-stat-cost">
                <span className="quota-stat-label">누적 비용</span>
                <span className="quota-stat-cost-val">${formatUsd(sum.total_cost_usd)}</span>
              </div>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}
export default AgentQuotaCard;
