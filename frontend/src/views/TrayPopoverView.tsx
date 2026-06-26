import { invoke } from "@tauri-apps/api/core";
import { useQueryClient } from "@tanstack/react-query";
import { useDbDirty } from "../lib/dbUpdateBus";
import { useDbInvalidation } from "../hooks/useDbInvalidation";
import { useAgentSummaries, useLoopSignals } from "../hooks/queries/useDbQueries";
import { useSubscriptionQuota } from "../hooks/queries/useQuotaQuery";
import { useSettings } from "../hooks/queries/useSettingsQuery";
import { queryKeys } from "../lib/queryKeys";
import { formatTokens } from "../utils/formatters";
import { AgentQuotaCard } from "../components/AgentQuotaCard";
import { AlertTriangle, CheckCircle2, RefreshCw } from "lucide-react";

/**
 * 시스템 트레이 전용 팝오버 뷰 컴포넌트
 *
 * 트레이 아이콘을 클릭했을 때 나타나는 소형 팝업 UI를 담당하며,
 * 실시간 오작동 상태 및 에이전트별 토큰 쿼터 상황을 콤팩트하게 제공합니다.
 * 데이터는 React Query 로 조회하고(폴링은 refresh_interval 기반 refetchInterval 자동),
 * 트레이 webview 자체의 db-updated 무효화는 useDbInvalidation 이 담당합니다.
 * (투명 윈도우 위에 렌더 — 이 컨테이너만 시각적으로 보인다. main.tsx의 html.tray 참조)
 */
export function TrayPopoverView() {
  const queryClient = useQueryClient();
  // 트레이 webview 자신의 db-updated → DB 파생 쿼리 무효화 (freeze-while-viewing 유지)
  useDbInvalidation();

  const summariesQ = useAgentSummaries();
  const anomaliesQ = useLoopSignals();
  const quotasQ = useSubscriptionQuota();
  const tokenDisplayMode = useSettings().data?.token_display_mode ?? "tokens";

  const summaries = summariesQ.data ?? [];
  const anomalies = anomaliesQ.data ?? [];
  const quotas = quotasQ.data ?? [];
  const loading = summariesQ.isLoading || quotasQ.isLoading;

  // 보는 중 동결 상태에서 새 변경이 쌓이면 dirty=true → "새로고침" 어포던스 노출
  const { dirty, refresh } = useDbDirty();

  const handleCardRefresh = () => {
    queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
    queryClient.invalidateQueries({ queryKey: queryKeys.subscriptionQuota() });
  };

  const totalAnomalies = anomalies.length;

  const handleBannerClick = async () => {
    if (anomalies.length > 0) {
      const firstAnomalySessionId = anomalies[0].session_id;
      try {
        await invoke("focus_main_window", { sessionId: firstAnomalySessionId });
      } catch (e) {
        console.error("focus_main_window 호출 실패:", e);
      }
    }
  };

  return (
    <div className="flex h-full flex-col gap-2.5 overflow-hidden rounded-xl border border-border bg-popover/95 p-3 text-foreground shadow-lg backdrop-blur-md">
      {/* 헤더 */}
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-semibold">에이전트 토큰 관측소</h4>
        {dirty ? (
          <button
            onClick={refresh}
            className="flex items-center gap-1 rounded-md px-1 text-[11px] font-semibold text-primary transition-opacity hover:opacity-80"
            title="보는 동안 멈춰둔 새 변경을 반영합니다"
          >
            <RefreshCw className="h-3 w-3" />
            새로고침
          </button>
        ) : (
          <span className="flex items-center gap-1 text-[11px] font-semibold text-success">
            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-success" />
            LIVE
          </span>
        )}
      </div>

      {/* 상태 배너 */}
      {totalAnomalies > 0 ? (
        <button
          onClick={handleBannerClick}
          className="flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-left text-xs font-medium text-destructive transition-colors hover:bg-destructive/15"
        >
          <AlertTriangle className="h-4 w-4 shrink-0" />
          <span>{totalAnomalies}개의 오작동 세션 감지됨</span>
        </button>
      ) : (
        <div className="flex items-center gap-2 rounded-lg border border-success/30 bg-success/10 px-3 py-2 text-xs font-medium text-success">
          <CheckCircle2 className="h-4 w-4 shrink-0" />
          <span>모든 프로세스 정상 작동 중</span>
        </div>
      )}

      {/* 에이전트 쿼터 리스트 */}
      <div className="flex flex-1 flex-col gap-2 overflow-y-auto">
        {loading ? (
          <div className="py-8 text-center text-xs text-muted-foreground">로드 중...</div>
        ) : (
          summaries.map((sum) => {
            let providerKey = "antigravity";
            if (sum.agent_type === "claude_code") providerKey = "anthropic";
            else if (sum.agent_type === "codex") providerKey = "openai";

            const quota = quotas.find((q) => q.provider === providerKey);

            return (
              <AgentQuotaCard
                key={sum.agent_type}
                sum={sum}
                quota={quota}
                tokenDisplayMode={tokenDisplayMode}
                isDashboard={false}
                isExpanded={false}
                onToggleExpand={() => {}}
                onRefresh={handleCardRefresh}
              />
            );
          })
        )}
      </div>

      {/* 푸터: 오늘 누적 토큰 */}
      <div className="flex items-center justify-between border-t border-border pt-2 text-xs">
        <span className="text-muted-foreground">오늘 누적 사용 토큰</span>
        <span className="font-semibold tabular-nums text-primary">
          {formatTokens(
            summaries.reduce((acc, curr) => acc + (curr.total_input_tokens + curr.total_output_tokens), 0)
          )}{" "}
          Tokens
        </span>
      </div>
    </div>
  );
}
export default TrayPopoverView;
