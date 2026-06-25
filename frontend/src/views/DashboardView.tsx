import {
  Session,
  AgentSummary,
  LoopDetectionResult,
  DailyTokenUsage,
  HourlyTokenUsage,
  PlanQuotaInfo,
} from "../types";
import { AgentQuotaCard } from "../components/AgentQuotaCard";
import { SplineChart } from "../components/SplineChart";
import { formatCwd, formatTokens } from "../utils/formatters";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Activity, AlertTriangle } from "lucide-react";
import { cn } from "@/lib/utils";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";

interface DashboardViewProps {
  summaries: AgentSummary[];
  dailyTokenUsage: DailyTokenUsage[];
  hourlyTokenUsage: HourlyTokenUsage[];
  quotaInfo: PlanQuotaInfo[];
  tokenDisplayMode: string;
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
 * 하단 현황 섹션은 1d/3d/7d 시간 필터(기본 1d)로 필터링하며, 변경 시마다 백엔드를 재호출합니다.
 */
export function DashboardView({
  summaries,
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
    setExpandedSummaries((prev) => ({
      ...prev,
      [agentType]: !prev[agentType],
    }));
  };

  // 하단 현황(활성 세션 + 오작동) 시간 필터 — 기본 1d
  const [bottomDays, setBottomDays] = useState<number>(1);
  const [bottomSessions, setBottomSessions] = useState<Session[]>([]);
  const [bottomAnomalies, setBottomAnomalies] = useState<LoopDetectionResult[]>([]);
  const [bottomLoading, setBottomLoading] = useState<boolean>(true);

  // 선택된 기간으로 백엔드 재조회 (필터 변경마다 API 호출)
  const fetchBottom = useCallback(async () => {
    setBottomLoading(true);
    try {
      const [sess, anoms] = await Promise.all([
        invoke<Session[]>("get_active_sessions", { days: bottomDays }),
        invoke<LoopDetectionResult[]>("get_loop_signals", { days: bottomDays }),
      ]);
      setBottomSessions(sess);
      setBottomAnomalies(anoms);
    } catch (e) {
      console.error("대시보드 현황 로드 실패:", e);
    } finally {
      setBottomLoading(false);
    }
  }, [bottomDays]);

  useEffect(() => {
    fetchBottom();
  }, [fetchBottom]);

  // 로그 변경 감지 시에도 현재 기간 기준으로 갱신
  useEffect(() => {
    const unlistenPromise = listen("db-updated", () => {
      fetchBottom();
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, [fetchBottom]);

  const showSessionSkeleton = bottomLoading && bottomSessions.length === 0;
  const showAnomalySkeleton = bottomLoading && bottomAnomalies.length === 0;

  return (
    <div className="flex flex-col gap-6">
      {/* 할당량 및 쿼터 정보 섹션 */}
      <section className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
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
      <SplineChart dailyTokenUsage={dailyTokenUsage} hourlyTokenUsage={hourlyTokenUsage} />

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
