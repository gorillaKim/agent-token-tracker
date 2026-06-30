import { AgentQuotaCard } from "../components/AgentQuotaCard";
import { SplineChart } from "../components/SplineChart";
import { McpTrendChart } from "../components/McpTrendChart";
import { formatCwd, formatTokens } from "../utils/formatters";
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Activity, AlertTriangle, RefreshCw } from "lucide-react";
import { cn } from "@/lib/utils";
import { useDbDirty } from "@/lib/dbUpdateBus";
import { queryKeys } from "@/lib/queryKeys";
import {
  useActiveSessions,
  useAgentSummaries,
  useDailyTokenUsage,
  useHourlyTokenUsage,
  useLoopSignals,
} from "@/hooks/queries/useDbQueries";
import { useSubscriptionQuota } from "@/hooks/queries/useQuotaQuery";
import { useSettings } from "@/hooks/queries/useSettingsQuery";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";

interface DashboardViewProps {
  setSelectedSessionId: (id: string | null) => void;
}

const DAY_OPTIONS = [
  { label: "1d", value: 1 },
  { label: "3d", value: 3 },
  { label: "7d", value: 7 },
];

/**
 * 대시보드 메인 화면 뷰
 *
 * 할당량 카드 그리드, 인터랙티브 토큰 사용량 차트, 그리고 활성/오작동 세션 현황판을 렌더링합니다.
 * 모든 데이터는 React Query 로 섹션별 독립 로딩/캐싱됩니다 — 빠른 DB 카드는 즉시 뜨고,
 * 느린 구독 쿼터 게이지는 자기 로딩 상태로 따로 채워집니다. 하단 현황은 1d/3d/7d 필터로
 * 조회하며, 로그 변경(db-updated)은 앱 레벨 무효화(useDbInvalidation)로 자동 갱신됩니다.
 */
export function DashboardView({ setSelectedSessionId }: DashboardViewProps) {
  const queryClient = useQueryClient();

  // 상단 카드/차트 데이터 (섹션별 독립 로딩)
  const summariesQ = useAgentSummaries();
  const quotaQ = useSubscriptionQuota();
  const dailyQ = useDailyTokenUsage(30);
  const hourlyQ = useHourlyTokenUsage();
  const tokenDisplayMode = useSettings().data?.token_display_mode ?? "tokens";

  const summaries = summariesQ.data ?? [];
  const quotaInfo = quotaQ.data ?? [];

  // 에이전트 누적 카드 수동 확장 상태
  const [expandedSummaries, setExpandedSummaries] = useState<Record<string, boolean>>({
    claude_code: false,
    codex: false,
    antigravity: false,
  });

  const toggleSummary = (agentType: string) => {
    setExpandedSummaries((prev) => ({
      ...prev,
      [agentType]: !prev[agentType],
    }));
  };

  // 카드별 수동 새로고침 → DB 파생 + 쿼터 무효화(백그라운드 refetch)
  const handleCardRefresh = () => {
    queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
    queryClient.invalidateQueries({ queryKey: queryKeys.subscriptionQuota() });
  };

  // 하단 현황(활성 세션 + 오작동) 시간 필터 — 기본 1d
  const [bottomDays, setBottomDays] = useState<number>(1);
  const bottomSessionsQ = useActiveSessions(bottomDays);
  const bottomAnomaliesQ = useLoopSignals(bottomDays);

  const bottomSessions = bottomSessionsQ.data ?? [];
  const bottomAnomalies = bottomAnomaliesQ.data ?? [];

  // 메인 창을 보는 동안 동결되어 미반영된 변경 존재 여부 → 상단 새로고침 배너
  const { dirty, refresh } = useDbDirty();

  // 스켈레톤은 캐시 없는 최초 로드(isLoading)에서만 — keepPreviousData 로 기간 전환 시엔 깜빡임 없음
  const showSessionSkeleton = bottomSessionsQ.isLoading;
  const showAnomalySkeleton = bottomAnomaliesQ.isLoading;

  return (
    <div className="flex flex-col gap-6">
      {/* 보는 동안 동결된 새 변경 알림 (dirty일 때만 노출) */}
      {dirty && (
        <button
          onClick={refresh}
          className="flex items-center justify-center gap-2 rounded-lg border border-primary/30 bg-primary/10 px-3 py-1.5 text-xs font-medium text-primary transition-colors hover:bg-primary/15"
          title="보는 동안 멈춰둔 새 변경을 반영합니다"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          새 데이터가 있습니다 · 클릭하여 새로고침
        </button>
      )}

      {/* 할당량 및 쿼터 정보 섹션 — 요약(DB)은 즉시, 쿼터(외부 API)는 카드 내부에서 독립 로딩 */}
      <section className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
        {summariesQ.isLoading ? (
          <>
            <Skeleton className="h-40 w-full" />
            <Skeleton className="h-40 w-full" />
            <Skeleton className="h-40 w-full" />
          </>
        ) : (
          summaries.map((sum) => {
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
                onRefresh={handleCardRefresh}
              />
            );
          })
        )}
      </section>

      {/* 개선된 인터랙티브 차트 컴포넌트 마운트 */}
      <SplineChart dailyTokenUsage={dailyQ.data ?? []} hourlyTokenUsage={hourlyQ.data ?? []} />

      {/* MCP 종류별 호출 추이 차트 마운트 */}
      <McpTrendChart />

      {/* 대시보드 하단: 활성 세션 & 이상 세션 (1d/3d/7d 필터) */}
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-end gap-2">
          <span className="mr-1 text-xs text-muted-foreground">기간</span>
          <div className="flex items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
            {DAY_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                onClick={() => setBottomDays(opt.value)}
                className={cn(
                  "rounded-md px-3 py-1 text-xs font-medium transition-colors",
                  bottomDays === opt.value
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                )}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
          <Card className="lg:col-span-2">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-base">
                <Activity className="h-4 w-4 text-success" />
                활성 작업 세션
                <Badge variant="secondary" className="ml-auto tabular-nums">
                  {bottomSessions.length}
                </Badge>
              </CardTitle>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-[280px] pr-3">
                <div className="flex flex-col gap-1">
                  {showSessionSkeleton ? (
                    <div className="flex flex-col gap-2">
                      <Skeleton className="h-14 w-full" />
                      <Skeleton className="h-14 w-full" />
                      <Skeleton className="h-14 w-full" />
                    </div>
                  ) : (
                    <>
                      {bottomSessions.map((s) => (
                        <button
                          key={s.session_id}
                          onClick={() => setSelectedSessionId(s.session_id)}
                          className="flex w-full items-center justify-between gap-3 rounded-md px-3 py-2.5 text-left transition-colors hover:bg-muted"
                        >
                          <div className="flex min-w-0 flex-col gap-0.5">
                            <div className="flex items-center gap-2">
                              <span className="truncate text-sm font-medium">
                                {s.session_name || s.session_id.substring(0, 18) + "..."}
                              </span>
                              {s.parent_session_id && (
                                <Badge variant="secondary" className="shrink-0 px-1.5 py-0 text-[10px]">
                                  서브에이전트
                                </Badge>
                              )}
                            </div>
                            <span className="truncate text-xs text-muted-foreground">
                              {s.agent_type} • {formatCwd(s.cwd)}
                            </span>
                          </div>
                          <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground">
                            {formatTokens(s.total_input_tokens + s.total_output_tokens)} Tokens
                          </span>
                        </button>
                      ))}
                      {bottomSessions.length === 0 && (
                        <div className="py-8 text-center text-sm text-muted-foreground">
                          선택한 기간에 활성 세션이 없습니다.
                        </div>
                      )}
                    </>
                  )}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-base">
                <AlertTriangle className="h-4 w-4 text-destructive" />
                오작동 탐지 현황
                <Badge
                  variant={bottomAnomalies.length > 0 ? "destructive" : "secondary"}
                  className="ml-auto tabular-nums"
                >
                  {bottomAnomalies.length}
                </Badge>
              </CardTitle>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-[280px] pr-3">
                <div className="flex flex-col gap-1">
                  {showAnomalySkeleton ? (
                    <div className="flex flex-col gap-2">
                      <Skeleton className="h-12 w-full" />
                      <Skeleton className="h-12 w-full" />
                      <Skeleton className="h-12 w-full" />
                    </div>
                  ) : (
                    <>
                      {bottomAnomalies.map((a) => (
                        <button
                          key={a.session_id}
                          onClick={() => setSelectedSessionId(a.session_id)}
                          className="flex w-full flex-col gap-0.5 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2.5 text-left transition-colors hover:bg-destructive/10"
                        >
                          <span className="truncate font-mono text-xs">
                            {a.session_id.substring(0, 16)}...
                          </span>
                          <span className="text-xs text-destructive">
                            오작동 시그널 {a.signals.length}개 검출됨
                          </span>
                        </button>
                      ))}
                      {bottomAnomalies.length === 0 && (
                        <div className="py-8 text-center text-sm text-muted-foreground">
                          선택한 기간에 오작동 세션이 없습니다.
                        </div>
                      )}
                    </>
                  )}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
