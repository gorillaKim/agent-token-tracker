import type { ReactNode } from "react";
import { Session, LoopDetectionResult, SessionDetails } from "../types";
import { formatCwd, formatTokens, formatUsd, formatLocalTime } from "../utils/formatters";
import { LoopDirectionViewer } from "./LoopDirectionViewer";
import { cn } from "@/lib/utils";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertTriangle, Loader2 } from "lucide-react";

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

/** 세션 메타데이터 한 줄 (라벨 ↔ 값) */
function MetaRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 text-sm">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="flex min-w-0 items-center gap-1.5 text-right font-medium">{children}</span>
    </div>
  );
}

/**
 * 세션 상세 정보 디버깅을 위한 우측 사이드 드로어 패널 컴포넌트 (shadcn Sheet 기반)
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
    <Sheet
      open={selectedSessionId != null}
      onOpenChange={(open) => {
        if (!open) setSelectedSessionId(null);
      }}
    >
      <SheetContent side="right" className="flex w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-[520px]">
        <SheetHeader className="space-y-1 border-b border-border px-6 py-4 text-left">
          <SheetTitle>세션 상세 디버거</SheetTitle>
          <SheetDescription className="sr-only">
            선택한 세션의 상세 정보와 도구 호출 타임라인을 표시합니다.
          </SheetDescription>
          {selectedSess && (
            <>
              <p className="text-sm font-semibold text-foreground">
                {selectedSess.session_name || "이름 없음"}
              </p>
              <p className="break-all font-mono text-xs text-muted-foreground">
                ID: {selectedSess.session_id}
              </p>
            </>
          )}
        </SheetHeader>

        {selectedSess ? (
          <>
            <ScrollArea className="min-h-0 flex-1">
              <div className="flex flex-col gap-6 px-6 py-5">
                {/* 낭비 비용(Cost Waste) 경고 */}
                {selectedAnomaly && (
                  <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-4">
                    <div className="flex items-center justify-between">
                      <span className="flex items-center gap-1.5 text-sm font-semibold text-destructive">
                        <AlertTriangle className="h-4 w-4" />
                        낭비 비용 (Cost Waste)
                      </span>
                      <span className="text-lg font-semibold tabular-nums text-destructive">
                        ${formatUsd(costWasteVal)} USD
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      루프 오작동 및 도구 호출 실패로 낭비된 비용이 실시간 추적되었습니다.
                    </p>
                  </div>
                )}

                {/* 세션 메타데이터 */}
                <div className="flex flex-col gap-3">
                  <MetaRow label="에이전트 타입">
                    {selectedSess.agent_type}
                    {selectedSess.parent_session_id && (
                      <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
                        서브에이전트
                      </Badge>
                    )}
                  </MetaRow>
                  {selectedSess.parent_session_id && (
                    <MetaRow label="상위 세션 ID">
                      <span className="truncate font-mono text-xs">
                        {selectedSess.parent_session_id.substring(0, 16)}...
                      </span>
                    </MetaRow>
                  )}
                  <MetaRow label="작업 경로 (CWD)">
                    <span className="truncate font-mono text-xs">{formatCwd(selectedSess.cwd)}</span>
                  </MetaRow>
                  <MetaRow label="사용 모델 ID">{selectedSess.model_id || "알 수 없음"}</MetaRow>
                  <MetaRow label="누적 토큰 사용">
                    <span className="tabular-nums text-primary">
                      {formatTokens(selectedSess.total_input_tokens + selectedSess.total_output_tokens)} Tokens
                    </span>
                  </MetaRow>
                </div>

                {/* 이상 징후 시각화 */}
                {selectedAnomaly && (
                  <div className="flex flex-col gap-3">
                    <h4 className="flex items-center gap-1.5 text-sm font-semibold text-destructive">
                      <AlertTriangle className="h-4 w-4" />
                      감지된 이상 징후 분석
                    </h4>
                    <div className="flex flex-col gap-2">
                      {selectedAnomaly.signals.map((s, idx) => (
                        <div
                          key={idx}
                          className="rounded-md border border-destructive/15 bg-destructive/5 p-2 text-xs text-foreground/80"
                        >
                          {s.description}
                        </div>
                      ))}
                    </div>
                    <LoopDirectionViewer signals={selectedAnomaly.signals} />
                  </div>
                )}

                {/* 도구 호출 타임라인 */}
                <div className="flex flex-col gap-3">
                  <h4 className="border-b border-border pb-2 text-sm font-semibold">
                    도구 호출 타임라인
                    {!detailsLoading && sessionDetails && ` (${sessionDetails.tool_calls.length}건)`}
                  </h4>
                  {detailsLoading ? (
                    <div className="flex flex-col gap-2">
                      <Skeleton className="h-16 w-full" />
                      <Skeleton className="h-16 w-full" />
                      <Skeleton className="h-16 w-full" />
                    </div>
                  ) : sessionDetails ? (
                    <div className="flex flex-col gap-2">
                      {sessionDetails.tool_calls.map((tc, idx) => (
                        <div key={idx} className="rounded-lg border border-border bg-muted/30 p-3 text-xs">
                          <div className="mb-1 flex items-center justify-between">
                            <span
                              className={cn(
                                "font-semibold",
                                tc.success ? "text-primary" : "text-destructive"
                              )}
                            >
                              {tc.tool_name}
                            </span>
                            <span className="font-mono text-[11px] text-muted-foreground">
                              {formatLocalTime(tc.created_at)}
                            </span>
                          </div>
                          <div className="max-h-20 overflow-x-auto whitespace-pre-wrap break-all rounded bg-background/60 p-2 font-mono text-[11px] text-muted-foreground">
                            {tc.tool_input}
                          </div>
                        </div>
                      ))}
                      {sessionDetails.tool_calls.length === 0 && (
                        <div className="py-8 text-center text-sm text-muted-foreground">
                          이 세션에서 호출된 도구 이력이 없습니다.
                        </div>
                      )}
                    </div>
                  ) : null}
                </div>
              </div>
            </ScrollArea>

            {/* 이상 제어 Interrupt Action (하단 고정) */}
            <div className="border-t border-border px-6 py-4">
              <p className="mb-2 text-xs uppercase tracking-wide text-muted-foreground">
                위험 관리 및 이상 제어
              </p>
              <Button
                variant="destructive"
                className="w-full"
                onClick={() => onInterrupt(selectedSess.agent_type, selectedSess.cwd)}
                disabled={interruptLoading}
              >
                {interruptLoading && <Loader2 className="h-4 w-4 animate-spin" />}
                {interruptLoading ? "인터럽트 신호 송신 중..." : "에이전트 강제 종료 (Interrupt)"}
              </Button>
              {interruptMessage && (
                <div className="mt-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
                  {interruptMessage}
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center p-10 text-center text-sm text-muted-foreground">
            세션 정보를 찾을 수 없습니다.
          </div>
        )}
      </SheetContent>
    </Sheet>
  );
}
export default SessionDrawer;
