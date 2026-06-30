import { useState, useMemo, useRef } from "react";
import { Session, LoopDetectionResult, SessionAnalysis, TurnTokenUsage } from "../types";
import { formatTokens, formatUsd, formatLocalTime } from "../utils/formatters";
import { LoopDirectionViewer } from "../components/LoopDirectionViewer";
import { cn } from "@/lib/utils";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { FolderOpen, Loader2, Search, AlertTriangle, CheckCircle2, Info } from "lucide-react";

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

  const anomalyMap = useMemo(() => new Map(anomalies.map((a) => [a.session_id, a])), [anomalies]);

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

  const tokenDistributionData = useMemo(() => {
    if (!analysisData || !analysisData.token_distribution) {
      return { items: [], total: 0 };
    }
    const dist = analysisData.token_distribution;
    const total =
      dist.input_tokens +
      dist.output_tokens +
      dist.thinking_tokens +
      dist.core_tool_tokens +
      dist.mcp_tool_tokens;
    
    if (total === 0) {
      return { items: [], total: 0 };
    }

    const segments = [
      { label: "생각 (Thinking)", value: dist.thinking_tokens, color: "hsl(var(--primary))" },
      { label: "MCP 도구 (MCP Tools)", value: dist.mcp_tool_tokens, color: "hsl(200 95% 42%)" },
      { label: "기본 도구 (Core Tools)", value: dist.core_tool_tokens, color: "hsl(160 80% 40%)" },
      { label: "사용자 입력 (Input)", value: dist.input_tokens, color: "hsl(35 95% 55%)" },
      { label: "최종 답변 (Output)", value: dist.output_tokens, color: "hsl(340 85% 60%)" },
    ].filter((s) => s.value > 0);

    let accumulated = 0;
    const items = segments.map((seg) => {
      const percent = (seg.value / total) * 100;
      const strokeDash = `${percent} ${100 - percent}`;
      const strokeOffset = 100 - accumulated + 25; // 12시 방향 시작 (+25 보정)
      accumulated += percent;
      return {
        ...seg,
        percent,
        strokeDash,
        strokeOffset,
      };
    });

    return { items, total };
  }, [analysisData]);

  const agentLabel = (type: string) =>
    type === "claude_code"
      ? "Claude Code"
      : type === "codex"
        ? "Codex (OpenAI)"
        : type === "antigravity"
          ? "Antigravity (Local)"
          : type;

  return (
    <TooltipProvider delayDuration={200}>
      <div className="flex h-[calc(100vh-8rem)] flex-col gap-6 overflow-hidden lg:flex-row">
        {/* 1. 좌측 세션 목록 (좁은 폭에서는 상단으로 쌓임)
            overflow-hidden + 내부 ScrollArea min-h-0 → 목록이 카드 박스를 넘쳐 하단 패널과
            겹치는 현상(세로 좁을 때)을 방지하고 내부 스크롤로 가둔다. */}
        <Card className="flex w-full shrink-0 flex-col gap-3 overflow-hidden p-4 max-lg:max-h-[38%] lg:w-80">
          <h3 className="flex items-center gap-2 text-base font-semibold">
            <FolderOpen className="h-4 w-4 text-muted-foreground" />
            세션 히스토리
            <Badge variant="secondary" className="ml-auto tabular-nums">
              {filteredSessions.length}/{sessions.length}
            </Badge>
          </h3>

          {/* 필터 및 정렬 컨트롤러 */}
          <div className="flex flex-col gap-2">
            <div className="flex gap-2">
              <Select value={filterAgent} onValueChange={setFilterAgent}>
                <SelectTrigger className="h-8 flex-1 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">모든 제공사</SelectItem>
                  {Array.from(new Set(sessions.map((s) => s.agent_type))).map((type) => (
                    <SelectItem key={type} value={type}>
                      {agentLabel(type)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Select value={sortBy} onValueChange={(v) => setSortBy(v as typeof sortBy)}>
                <SelectTrigger className="h-8 flex-1 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="date_desc">최신 날짜순</SelectItem>
                  <SelectItem value="date_asc">오래된 날짜순</SelectItem>
                  <SelectItem value="tokens_desc">토큰 많은순</SelectItem>
                  <SelectItem value="tokens_asc">토큰 적은순</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-center gap-2">
              <Checkbox
                id="anomaly-filter"
                checked={filterAnomaly}
                onCheckedChange={(c) => setFilterAnomaly(c === true)}
              />
              <Label
                htmlFor="anomaly-filter"
                className={cn(
                  "cursor-pointer text-xs",
                  filterAnomaly ? "text-destructive" : "text-muted-foreground"
                )}
              >
                이상 감지 세션만 보기
              </Label>
            </div>
          </div>

          <ScrollArea className="-mr-2 min-h-0 flex-1 pr-2">
            <div className="flex flex-col gap-2">
              {filteredSessions.map((s) => {
                const isSelected = s.session_id === analysisSessionId;
                const hasAnomaly = anomalyMap.has(s.session_id);
                return (
                  <button
                    key={s.session_id}
                    onClick={() => onSelectSession(s.session_id)}
                    className={cn(
                      "flex flex-col gap-1.5 rounded-lg border p-3 text-left transition-colors",
                      isSelected
                        ? "border-primary bg-primary/10"
                        : hasAnomaly
                          ? "border-destructive/30 bg-destructive/5 hover:bg-destructive/10"
                          : "border-border hover:bg-muted"
                    )}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span
                        title={s.session_name || s.session_id}
                        className={cn(
                          "truncate text-sm font-semibold",
                          hasAnomaly ? "text-destructive" : "text-foreground"
                        )}
                      >
                        {s.session_name || s.session_id.substring(0, 12) + "..."}
                      </span>
                      {hasAnomaly && (
                        <Badge variant="destructive" className="shrink-0 px-1.5 py-0 text-[10px]">
                          LOOP
                        </Badge>
                      )}
                    </div>
                    <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                      <span>{s.agent_type}</span>
                      {s.parent_session_id && (
                        <Badge variant="secondary" className="px-1 py-0 text-[9px]">
                          서브에이전트
                        </Badge>
                      )}
                      <span>• {formatLocalTime(s.started_at)}</span>
                    </div>
                    <span className="text-xs tabular-nums text-muted-foreground">
                      {formatTokens(s.total_input_tokens + s.total_output_tokens)} Tokens
                    </span>
                  </button>
                );
              })}
              {filteredSessions.length === 0 && (
                <div className="py-8 text-center text-sm text-muted-foreground">
                  조건에 부합하는 세션이 없습니다.
                </div>
              )}
            </div>
          </ScrollArea>
        </Card>

        {/* 2. 우측 분석 상세 패널 (@container: 패널 자체 너비 기준으로 내부 반응형)
            min-w-0: 넓은 내부 콘텐츠가 좌측 목록을 밀어내며 겹치는 현상 방지 */}
        <Card className="@container flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto p-6">
          {analysisLoading ? (
            <div className="flex flex-col gap-6">
              <Skeleton className="h-8 w-2/3" />
              <div className="grid grid-cols-2 gap-4 @2xl:grid-cols-4">
                <Skeleton className="h-20" />
                <Skeleton className="h-20" />
                <Skeleton className="h-20" />
                <Skeleton className="h-20" />
              </div>
              <Skeleton className="h-48 w-full" />
              <div className="grid grid-cols-1 gap-6 @2xl:grid-cols-2">
                <Skeleton className="h-40" />
                <Skeleton className="h-40" />
              </div>
            </div>
          ) : analysisData ? (
            <div className="flex flex-col gap-6">
              {/* 세션 개요 헤더 (좁은 폭에서는 제목/액션이 세로로 쌓임) */}
              <div className="flex flex-col gap-4 border-b border-border pb-4 @lg:flex-row @lg:items-start @lg:justify-between">
                <div className="min-w-0">
                  <h2 className="flex items-center gap-2 text-xl font-semibold">
                    <Search className="h-5 w-5 shrink-0 text-primary" />
                    <span className="min-w-0">세션 분석 보고서</span>
                    {sessions.find((s) => s.session_id === analysisSessionId)?.parent_session_id && (
                      <Badge variant="secondary" className="shrink-0 text-[10px]">
                        서브에이전트
                      </Badge>
                    )}
                  </h2>
                  <div className="mt-1 break-all font-mono text-xs text-muted-foreground">
                    {analysisData.session_name && (
                      <span className="mr-2 font-semibold text-foreground">{analysisData.session_name}</span>
                    )}
                    ID: {analysisData.session_id}
                  </div>
                </div>

                <div className="flex shrink-0 flex-wrap items-center gap-2">
                  <Badge
                    variant={analysisData.is_anomaly ? "destructive" : "secondary"}
                    className="gap-1"
                  >
                    {analysisData.is_anomaly ? (
                      <>
                        <AlertTriangle className="h-3 w-3" /> 위험 상태 감지
                      </>
                    ) : (
                      <>
                        <CheckCircle2 className="h-3 w-3" /> 안전 세션
                      </>
                    )}
                  </Badge>
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={() => onInterrupt(analysisData.agent_type, "")}
                    disabled={interruptLoading}
                  >
                    {interruptLoading && <Loader2 className="h-4 w-4 animate-spin" />}
                    {interruptLoading ? "중단 중..." : "에이전트 강제종료"}
                  </Button>
                </div>
              </div>

              {/* 기본 수치 요약 */}
              <div className="grid grid-cols-2 gap-4 @2xl:grid-cols-4">
                <div className="rounded-xl border border-border bg-muted/30 p-4">
                  <div className="text-xs text-muted-foreground">총 소비 비용</div>
                  <div className="mt-1 text-xl font-semibold tabular-nums text-primary">
                    ${formatUsd(analysisData.total_cost_usd)}
                  </div>
                </div>
                <div className="rounded-xl border border-border bg-muted/30 p-4">
                  <div className="text-xs text-muted-foreground">총 사용 토큰</div>
                  <div className="mt-1 text-xl font-semibold tabular-nums">
                    {formatTokens(analysisData.total_input_tokens + analysisData.total_output_tokens)}
                  </div>
                </div>
                <div className="rounded-xl border border-border bg-muted/30 p-4">
                  <div className="text-xs text-muted-foreground">캐시 히트율</div>
                  <div className="mt-1 text-xl font-semibold tabular-nums text-success">
                    {(analysisData.cache_hit_rate * 100).toFixed(1)}%
                  </div>
                </div>
                <div className="rounded-xl border border-border bg-muted/30 p-4">
                  <div className="text-xs text-muted-foreground">캐시 절감 비용</div>
                  <div className="mt-1 text-xl font-semibold tabular-nums text-success">
                    ${formatUsd(analysisData.cache_saved_cost)}
                  </div>
                </div>
              </div>

              {/* 턴별 토큰 소비 스택 바 차트 */}
              <div ref={chartContainerRef} className="relative rounded-xl border border-border bg-muted/30 p-5">
                <h4 className="mb-4 text-sm font-semibold">턴별 토큰 소비 분석 (턴 순서 흐름)</h4>

                {analysisData.turns.length > 0 ? (
                  <div className="flex flex-col gap-2">
                    {/* Legend */}
                    <div className="flex flex-wrap justify-end gap-x-4 gap-y-1 text-xs text-muted-foreground">
                      <div className="flex items-center gap-1.5">
                        <span className="h-2.5 w-2.5 rounded-sm bg-chart-2" />
                        <span>Input Tokens</span>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="h-2.5 w-2.5 rounded-sm bg-primary" />
                        <span>Output Tokens</span>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="h-2.5 w-2.5 rounded-sm bg-success" />
                        <span>Cache Read Tokens</span>
                      </div>
                    </div>

                    {/* Scrollable Container */}
                    <div className="mt-2 overflow-x-auto pb-2">
                      <div className="flex min-w-max flex-col gap-2">
                        {/* Bars container */}
                        <div className="flex h-[180px] items-end gap-1.5 border-b border-border pb-2">
                          {analysisData.turns.map((turn, idx) => {
                            const turnTotal = turn.input_tokens + turn.output_tokens;
                            const maxTurnTotal = Math.max(
                              ...analysisData.turns.map((t) => t.input_tokens + t.output_tokens),
                              1
                            );
                            const barHeightPct = (turnTotal / maxTurnTotal) * 100;

                            const inputPct = turnTotal > 0 ? (turn.input_tokens / turnTotal) * 100 : 0;
                            const outputPct = turnTotal > 0 ? (turn.output_tokens / turnTotal) * 100 : 0;
                            const cachePct =
                              turn.input_tokens > 0 ? (turn.cache_read_tokens / turn.input_tokens) * 100 : 0;

                            return (
                              <div
                                key={idx}
                                className="flex cursor-pointer flex-col-reverse overflow-hidden rounded-t bg-background/40 transition-opacity hover:opacity-80"
                                style={{ flex: "0 0 28px", height: `${barHeightPct}%`, minHeight: "10px" }}
                                onMouseEnter={(e) => {
                                  setHoveredTurn(turn);
                                  const rect = e.currentTarget.getBoundingClientRect();
                                  const containerRect = chartContainerRef.current?.getBoundingClientRect();
                                  if (containerRect) {
                                    setTooltipPos({
                                      x: rect.left - containerRect.left + 14,
                                      y: rect.top - containerRect.top - 10,
                                    });
                                  }
                                }}
                                onMouseLeave={() => setHoveredTurn(null)}
                              >
                                {/* Input Section */}
                                <div className="relative" style={{ height: `${inputPct}%`, background: "hsl(var(--chart-2))" }}>
                                  {/* Cache section embedded inside input */}
                                  {cachePct > 0 && (
                                    <div
                                      className="absolute bottom-0 left-0 right-0"
                                      style={{ height: `${cachePct}%`, background: "hsl(var(--success))" }}
                                    />
                                  )}
                                </div>
                                {/* Output Section */}
                                <div style={{ height: `${outputPct}%`, background: "hsl(var(--primary))" }} />
                              </div>
                            );
                          })}
                        </div>

                        {/* X Axis Labels */}
                        <div className="flex gap-1.5 text-[11px] text-muted-foreground">
                          {analysisData.turns.map((turn, idx) => (
                            <div key={idx} className="text-center" style={{ flex: "0 0 28px" }}>
                              T{turn.turn_index}
                            </div>
                          ))}
                        </div>
                      </div>
                    </div>

                    {/* Interactive Tooltip inside container */}
                    {hoveredTurn && (
                      <div
                        className="pointer-events-none absolute z-50 rounded-md border border-primary/60 bg-popover px-3 py-2 text-xs shadow-lg"
                        style={{
                          left: `${tooltipPos.x}px`,
                          top: `${tooltipPos.y}px`,
                          transform: "translate(-50%, -100%)",
                        }}
                      >
                        <div className="mb-1 font-semibold text-primary">
                          Turn {hoveredTurn.turn_index} ({hoveredTurn.role})
                        </div>
                        <div>
                          입력: <span className="font-medium tabular-nums">{hoveredTurn.input_tokens.toLocaleString()}</span>
                        </div>
                        <div>
                          출력: <span className="font-medium tabular-nums">{hoveredTurn.output_tokens.toLocaleString()}</span>
                        </div>
                        {hoveredTurn.cache_read_tokens > 0 && (
                          <div className="text-success">
                            캐시 리드: <span className="font-medium tabular-nums">{hoveredTurn.cache_read_tokens.toLocaleString()}</span>
                          </div>
                        )}
                        <div className="mt-1 font-medium text-primary">
                          비용: ${formatUsd(hoveredTurn.cost_usd)}
                        </div>
                      </div>
                    )}
                  </div>
                ) : (
                  <div className="py-8 text-center text-sm text-muted-foreground">
                    턴별 토큰 사용 내역이 없습니다.
                  </div>
                )}
              </div>

              {/* 하단 3단: 캐시 도넛, 토큰 영역 도넛 & 도구 비용 랭킹 */}
              <div className="grid grid-cols-1 gap-6 @3xl:grid-cols-3">
                {/* 캐시 히트율 도넛 차트 카드 */}
                <div className="rounded-xl border border-border bg-muted/30 p-5">
                  <div className="mb-4 flex items-center gap-1.5">
                    <h4 className="text-sm font-semibold">캐시 효율성 &amp; 히트율</h4>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <span className="cursor-help text-muted-foreground">
                          <Info className="h-3.5 w-3.5" />
                        </span>
                      </TooltipTrigger>
                      <TooltipContent className="max-w-[285px]">
                        캐시가 탑재된 Claude 모델 사용 시 입력 토큰을 최대 90% 저렴하게 처리하여 예산을 대폭 절감합니다.
                      </TooltipContent>
                    </Tooltip>
                  </div>
                  <div className="flex items-center gap-8">
                    {/* SVG 도넛 */}
                    <div className="relative h-[120px] w-[120px]">
                      <svg width="100%" height="100%" viewBox="0 0 42 42">
                        <circle cx="21" cy="21" r="15.915" fill="transparent" stroke="hsl(var(--muted))" strokeWidth="4" />
                        <circle
                          cx="21"
                          cy="21"
                          r="15.915"
                          fill="transparent"
                          stroke="hsl(var(--success))"
                          strokeWidth="4"
                          strokeDasharray={`${analysisData.cache_hit_rate * 100} ${100 - analysisData.cache_hit_rate * 100}`}
                          strokeDashoffset="25"
                          style={{ transition: "stroke-dasharray 0.5s ease" }}
                        />
                      </svg>
                      <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 text-center">
                        <div className="text-lg font-semibold tabular-nums">
                          {(analysisData.cache_hit_rate * 100).toFixed(0)}%
                        </div>
                        <div className="text-[10px] uppercase text-muted-foreground">Hit Rate</div>
                      </div>
                    </div>

                    <div className="flex flex-1 flex-col gap-2">
                      <div className="rounded-lg border border-success/20 bg-success/5 p-3">
                        <div className="text-xs text-muted-foreground">누적 캐시 사용량</div>
                        <div className="mt-0.5 text-base font-semibold tabular-nums text-success">
                          {formatTokens(analysisData.total_cache_read_tokens)} Tokens
                        </div>
                      </div>
                    </div>
                  </div>
                </div>

                {/* 토큰 소모 영역별 분포 도넛 차트 카드 */}
                <div className="rounded-xl border border-border bg-muted/30 p-5">
                  <div className="mb-4 flex items-center gap-1.5">
                    <h4 className="text-sm font-semibold">토큰 소모 영역별 분포</h4>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <span className="cursor-help text-muted-foreground">
                          <Info className="h-3.5 w-3.5" />
                        </span>
                      </TooltipTrigger>
                      <TooltipContent className="max-w-[285px]">
                        세션 내에서 소비된 토큰이 생각 과정, 기본 로컬 도구, 외부 MCP 도구, 사용자의 순수 입력 및 답변 영역 중 어디에서 가장 많이 소모되었는지 시각화합니다.
                      </TooltipContent>
                    </Tooltip>
                  </div>
                  <div className="flex flex-col items-center gap-6 sm:flex-row sm:justify-start sm:gap-6">
                    {/* SVG 멀티 세그먼트 도넛 */}
                    {tokenDistributionData.total > 0 ? (
                      <>
                        <div className="relative h-[120px] w-[120px] shrink-0">
                          <svg width="100%" height="100%" viewBox="0 0 42 42" className="-rotate-90">
                            {/* 백그라운드 기본 원 */}
                            <circle cx="21" cy="21" r="15.915" fill="transparent" stroke="hsl(var(--muted)/0.3)" strokeWidth="4.5" />
                            {tokenDistributionData.items.map((item, idx) => (
                              <circle
                                key={idx}
                                cx="21"
                                cy="21"
                                r="15.915"
                                fill="transparent"
                                stroke={item.color}
                                strokeWidth="4.5"
                                strokeDasharray={item.strokeDash}
                                strokeDashoffset={item.strokeOffset}
                                style={{ transition: "stroke-dasharray 0.5s ease" }}
                              />
                            ))}
                          </svg>
                          <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 text-center w-full px-1">
                            <div className="text-[11px] font-bold tabular-nums text-foreground truncate">
                              {formatTokens(tokenDistributionData.total)}
                            </div>
                            <div className="text-[9px] uppercase text-muted-foreground">Tokens</div>
                          </div>
                        </div>

                        {/* 범례 리스트 */}
                        <div className="flex flex-1 flex-col gap-1.5 text-xs w-full min-w-0">
                          {tokenDistributionData.items.map((item, idx) => (
                            <div key={idx} className="flex items-center justify-between gap-2 min-w-0">
                              <div className="flex items-center gap-1.5 min-w-0 overflow-hidden">
                                <span className="h-2.5 w-2.5 rounded-full shrink-0" style={{ backgroundColor: item.color }} />
                                <Tooltip>
                                  <TooltipTrigger asChild>
                                    <span className="text-muted-foreground truncate">{item.label}</span>
                                  </TooltipTrigger>
                                  <TooltipContent side="top">{item.label}</TooltipContent>
                                </Tooltip>
                              </div>
                              <div className="font-mono font-medium text-[11px] tabular-nums text-foreground/80 shrink-0">
                                {item.percent.toFixed(0)}%
                              </div>
                            </div>
                          ))}
                        </div>
                      </>
                    ) : (
                      <div className="flex flex-1 h-[120px] items-center justify-center text-xs text-muted-foreground">
                        토큰 정보 없음
                      </div>
                    )}
                  </div>
                </div>

                {/* 도구 비용 랭킹 카드 */}
                <div className="rounded-xl border border-border bg-muted/30 p-5">
                  <h4 className="mb-4 text-sm font-semibold">도구별 비용 랭킹</h4>
                  <div className="flex max-h-40 flex-col gap-3 overflow-y-auto pr-1">
                    {analysisData.tool_cost_rank.map((t) => {
                      const maxCost = Math.max(
                        ...analysisData.tool_cost_rank.map((tc) => tc.total_cost_usd),
                        0.0001
                      );
                      const barWidth = (t.total_cost_usd / maxCost) * 100;
                      return (
                        <div key={t.tool_name} className="flex flex-col gap-1">
                          <div className="flex items-center justify-between gap-3 text-xs">
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <span className="min-w-0 flex-1 cursor-help truncate text-left font-mono text-foreground/85">
                                  {t.tool_name}
                                </span>
                              </TooltipTrigger>
                              <TooltipContent className="max-w-[300px] break-all">
                                {t.tool_name}
                              </TooltipContent>
                            </Tooltip>
                            <span className="shrink-0 whitespace-nowrap font-semibold tabular-nums text-primary">
                              ${formatUsd(t.total_cost_usd)} ({t.call_count}회)
                            </span>
                          </div>
                          <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                            <div className="h-full rounded-full bg-primary" style={{ width: `${barWidth}%` }} />
                          </div>
                        </div>
                      );
                    })}
                    {analysisData.tool_cost_rank.length === 0 && (
                      <div className="py-6 text-center text-sm text-muted-foreground">
                        이 세션에 도구 호출 정보가 없습니다.
                      </div>
                    )}
                  </div>
                </div>
              </div>

              {/* 이상 제어 / 시각화 디렉션 */}
              <div className="rounded-xl border border-border bg-muted/30 p-5">
                <h4
                  className={cn(
                    "mb-3 flex items-center gap-1.5 text-sm font-semibold",
                    analysisData.is_anomaly ? "text-destructive" : "text-success"
                  )}
                >
                  {analysisData.is_anomaly ? (
                    <AlertTriangle className="h-4 w-4" />
                  ) : (
                    <CheckCircle2 className="h-4 w-4" />
                  )}
                  {analysisData.is_anomaly ? "오작동 이상 탐지 분석" : "세션 이상 탐지 분석"}
                </h4>
                {analysisData.is_anomaly ? (
                  <div>
                    <div className="mb-4 flex flex-col gap-2 text-sm">
                      {analysisData.anomaly_signals.map((s, idx) => (
                        <div
                          key={idx}
                          className="rounded-r-md border-l-2 border-destructive bg-destructive/5 px-3 py-2 text-foreground/85"
                        >
                          <strong>
                            {s.signal_type === "repeated_call" ? "자가 루프 의심" : "핑퐁 순환 호출"}:
                          </strong>{" "}
                          {s.description}
                        </div>
                      ))}
                    </div>
                    <LoopDirectionViewer signals={analysisData.anomaly_signals} />
                  </div>
                ) : (
                  <div className="text-sm text-muted-foreground">
                    현재 세션에서 동일한 도구 호출의 오작동 순환(Loop) 현상이나 급격한 토큰 폭증 이상 징후가
                    검출되지 않았습니다. 안전하게 관리되고 있습니다.
                  </div>
                )}
              </div>

              {interruptMessage && (
                <div className="rounded-lg border border-border bg-muted/40 px-4 py-3 text-xs text-muted-foreground">
                  {interruptMessage}
                </div>
              )}
            </div>
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
              <Search className="h-8 w-8 opacity-40" />
              <span className="text-sm">왼쪽 히스토리 목록에서 분석할 세션을 선택해 주세요.</span>
            </div>
          )}
        </Card>
      </div>
    </TooltipProvider>
  );
}
export default SessionAnalysisView;
