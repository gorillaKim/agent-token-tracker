import { useEffect, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { Session, SessionDetails, SessionAnalysis } from "../types";
import { queryKeys } from "../lib/queryKeys";
import { GC_TIME, STALE_TIME } from "../lib/queryConfig";

/**
 * 특정 세션의 상세 조회·분석·중단을 관리하는 훅 (React Query 기반).
 *
 * 선택 상태(analysisSessionId / selectedSessionId)는 로컬 useState 로 유지하고,
 * 분석/상세는 enabled 게이팅 쿼리로, 인터럽트는 mutation 으로 처리한다.
 * 반환 shape 는 기존과 동일해 SessionAnalysisView / SessionDrawer / App 은 수정 불필요.
 *
 * @param sessions 활성 세션 목록(인터럽트 시 cwd fallback 용)
 * @param invalidateData 인터럽트 성공 후 호출할 데이터 갱신(=쿼리 무효화) 콜백
 */
export function useSessionDetails(sessions: Session[], invalidateData: () => Promise<void>) {
  const [analysisSessionId, setAnalysisSessionId] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [interruptMessage, setInterruptMessage] = useState<string | null>(null);

  // 분석 탭 세션 분석 (선택 시에만 조회)
  const analysisQuery = useQuery({
    queryKey: queryKeys.sessionAnalysis(analysisSessionId ?? "none"),
    queryFn: () => invoke<SessionAnalysis>("get_session_analysis", { sessionId: analysisSessionId }),
    enabled: !!analysisSessionId,
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });

  // 드로어 세션 상세 (선택 시에만 조회)
  const detailsQuery = useQuery({
    queryKey: queryKeys.sessionDetails(selectedSessionId ?? "none"),
    queryFn: () => invoke<SessionDetails>("get_session_details", { sessionId: selectedSessionId }),
    enabled: !!selectedSessionId,
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });

  // 드로어를 닫으면(선택 해제) 인터럽트 메시지도 초기화 (기존 동작 보존)
  useEffect(() => {
    if (!selectedSessionId) setInterruptMessage(null);
  }, [selectedSessionId]);

  const handleSelectAnalysisSession = (sessId: string) => {
    setAnalysisSessionId(sessId);
  };

  // 에이전트 인터럽트(종료) mutation
  const interruptMutation = useMutation({
    mutationFn: (vars: { agentType: string; cwd: string }) =>
      invoke("interrupt_agent", { agentType: vars.agentType, cwd: vars.cwd }),
    onSuccess: async () => {
      setInterruptMessage("인터럽트 강제 종료 신호가 운영체제 커널에 안전하게 전송되었습니다.");
      await invalidateData();
    },
    onError: (err) => {
      setInterruptMessage(`강제 종료 실패: ${String(err)}`);
    },
  });

  const handleInterruptAgent = async (agentType: string, cwd: string) => {
    setInterruptMessage(null);
    const targetCwd =
      cwd || sessions.find((s) => s.session_id === selectedSessionId)?.cwd || "";
    interruptMutation.mutate({ agentType, cwd: targetCwd });
  };

  return {
    analysisSessionId,
    setAnalysisSessionId,
    analysisData: analysisSessionId ? analysisQuery.data ?? null : null,
    analysisLoading: analysisQuery.isLoading,
    selectedSessionId,
    setSelectedSessionId,
    sessionDetails: selectedSessionId ? detailsQuery.data ?? null : null,
    detailsLoading: detailsQuery.isLoading,
    interruptMessage,
    setInterruptMessage,
    interruptLoading: interruptMutation.isPending,
    handleSelectAnalysisSession,
    handleInterruptAgent,
  };
}
