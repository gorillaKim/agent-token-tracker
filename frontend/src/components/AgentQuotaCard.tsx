import { AgentSummary, PlanQuotaInfo } from "../types";
import { formatTokens, formatUsd, formatResetTime } from "../utils/formatters";
import { cn } from "@/lib/utils";
import { Progress } from "@/components/ui/progress";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ChevronRight } from "lucide-react";

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
  // 에이전트별 식별: 이름 + 색 토큰 클래스 (텍스트 / Progress 인디케이터)
  let agentName = "Antigravity";
  let agentText = "text-agent-antigravity";
  let agentBar = "[&_[data-slot=progress-indicator]]:bg-agent-antigravity";

  if (sum.agent_type === "claude_code") {
    agentName = "Claude Code";
    agentText = "text-agent-claude";
    agentBar = "[&_[data-slot=progress-indicator]]:bg-agent-claude";
  } else if (sum.agent_type === "codex") {
    agentName = "Codex (OpenAI)";
    agentText = "text-agent-codex";
    agentBar = "[&_[data-slot=progress-indicator]]:bg-agent-codex";
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

  const barHeight = isDashboard ? "h-1.5" : "h-1";
  const labelSize = isDashboard ? "text-xs" : "text-[11px]";

  return (
    <div
      className={cn(
        "flex flex-col rounded-xl border border-border bg-card",
        isDashboard ? "gap-4 p-5" : "gap-2.5 rounded-lg bg-card/50 p-3"
      )}
    >
      {/* 헤더: 에이전트명 + 잔여 */}
      <div className="flex items-center justify-between">
        <div className="flex items-baseline gap-1.5">
          <span className={cn("font-semibold", agentText, isDashboard ? "text-base" : "text-sm")}>
            {agentName}
          </span>
          {!isDashboard && (
            <span className="text-[11px] text-muted-foreground">({sum.session_count} Sessions)</span>
          )}
        </div>
        <span className={cn("font-medium text-muted-foreground", isDashboard ? "text-sm" : "text-xs")}>
          {headerRemainingLabel}
        </span>
      </div>

      {/* 세션 사용량 (5시간 롤링 / 일간) */}
      {quota && (
        <div className="flex flex-col gap-1.5">
          <div className={cn("flex justify-between font-medium text-muted-foreground", labelSize)}>
            <span>
              {sum.agent_type === "claude_code" || sum.agent_type === "codex"
                ? "세션 사용량 (5시간 롤링)"
                : "세션 사용량 (일간 한도)"}
            </span>
            <span className="font-semibold tabular-nums text-foreground">
              {quota.quota_tokens > 900_000_000_000_000
                ? "무제한"
                : isPercentage
                  ? `${displayPct}%`
                  : `${formatTokens(quota.used_tokens)}`}
            </span>
          </div>
          <Progress value={displayPct} className={cn("bg-muted", barHeight, agentBar)} />
          <div className="text-right text-[10px] text-muted-foreground/70">{reset1}</div>
        </div>
      )}

      {/* 주간 / 월간 한도 */}
      {quota && quota.weekly_quota_tokens !== undefined && quota.weekly_quota_tokens !== null && (
        <div className="flex flex-col gap-1.5">
          <div className={cn("flex justify-between font-medium text-muted-foreground", labelSize)}>
            <span>
              {sum.agent_type === "claude_code"
                ? "모든 모델 (주간)"
                : sum.agent_type === "codex"
                  ? "모든 모델 (월간)"
                  : "모든 모델 (주간)"}
            </span>
            <span className="font-semibold tabular-nums text-foreground">
              {quota.weekly_quota_tokens > 900_000_000_000_000
                ? "무제한"
                : isPercentage
                  ? `${displayWeeklyPct}%`
                  : `${formatTokens(quota.weekly_used_tokens || 0)}`}
            </span>
          </div>
          <Progress value={displayWeeklyPct} className={cn("bg-muted", barHeight, agentBar)} />
          <div className="text-right text-[10px] text-muted-foreground/70">{reset2}</div>
        </div>
      )}

      {/* 누적 사용량 및 비용 (대시보드만) */}
      {isDashboard && (
        <Collapsible open={isExpanded} onOpenChange={onToggleExpand} className="border-t border-border pt-3">
          <CollapsibleTrigger className="flex w-full items-center gap-1.5 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground">
            <ChevronRight className={cn("h-3.5 w-3.5 transition-transform", isExpanded && "rotate-90")} />
            누적 사용량 및 비용 정보
          </CollapsibleTrigger>
          <CollapsibleContent className="pt-3">
            <div className="grid grid-cols-3 gap-3">
              <div className="flex flex-col gap-0.5">
                <span className="text-[11px] text-muted-foreground">활성 세션</span>
                <span className="text-sm font-semibold tabular-nums">{sum.session_count} 건</span>
              </div>
              <div className="flex flex-col gap-0.5">
                <span className="text-[11px] text-muted-foreground">총 입/출력 토큰</span>
                <span className="text-sm font-semibold tabular-nums">
                  {sum.total_input_tokens + sum.total_output_tokens > 0
                    ? formatTokens(sum.total_input_tokens + sum.total_output_tokens)
                    : "-"}
                </span>
              </div>
              <div className="flex flex-col gap-0.5">
                <span className="text-[11px] text-muted-foreground">누적 비용</span>
                <span className="text-sm font-semibold tabular-nums text-primary">
                  ${formatUsd(sum.total_cost_usd)}
                </span>
              </div>
            </div>
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  );
}
export default AgentQuotaCard;
