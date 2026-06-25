import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Session, SessionDetails, SessionAnalysis } from "../types";

/**
 * 특정 세션의 상세 조회, 분석 요청, 중단 요청 등의 도메인 상태와 동작을 관리하는 커스텀 훅
 */
export function useSessionDetails(sessions: Session[], loadData: () => Promise<void>) {
  // 세션 분석 전용 상태
  const [analysisSessionId, setAnalysisSessionId] = useState<string | null>(null);
  const [analysisData, setAnalysisData] = useState<SessionAnalysis | null>(null);
  const [analysisLoading, setAnalysisLoading] = useState(false);

  // 선택된 디바이스/드로어 상세 상태
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [sessionDetails, setSessionDetails] = useState<SessionDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [interruptMessage, setInterruptMessage] = useState<string | null>(null);
  const [interruptLoading, setInterruptLoading] = useState(false);

  // 분석 탭에서의 세션 선택 처리
  const handleSelectAnalysisSession = async (sessId: string) => {
    setAnalysisSessionId(sessId);
    setAnalysisLoading(true);
    setAnalysisData(null);
    try {
      const data = await invoke<SessionAnalysis>("get_session_analysis", { sessionId: sessId });
      setAnalysisData(data);
    } catch (err: any) {
      console.error("세션 분석 조회 실패:", err);
    } finally {
      setAnalysisLoading(false);
    }
  };

  // 에이전트 인터럽트(종료)
  const handleInterruptAgent = async (agentType: string, cwd: string) => {
    setInterruptLoading(true);
    setInterruptMessage(null);
    try {
      const targetCwd = cwd || (sessions.find(s => s.session_id === selectedSessionId)?.cwd) || "";
      await invoke("interrupt_agent", { agentType, cwd: targetCwd });
      setInterruptMessage("인터럽트 강제 종료 신호가 운영체제 커널에 안전하게 전송되었습니다.");
      await loadData();
    } catch (err: any) {
      setInterruptMessage(`강제 종료 실패: ${err.toString()}`);
    } finally {
      setInterruptLoading(false);
    }
  };

  // 드로어 선택 시 상세 이력 비동기 조회
  useEffect(() => {
    if (!selectedSessionId) {
      setSessionDetails(null);
      setInterruptMessage(null);
      return;
    }

    const fetchDetails = async () => {
      setDetailsLoading(true);
      try {
        const details = await invoke<SessionDetails>("get_session_details", { sessionId: selectedSessionId });
        setSessionDetails(details);
      } catch (e) {
        console.error("세션 상세 조회 실패:", e);
      } finally {
        setDetailsLoading(false);
      }
    };

    fetchDetails();
  }, [selectedSessionId]);

  return {
    analysisSessionId,
    setAnalysisSessionId,
    analysisData,
    analysisLoading,
    selectedSessionId,
    setSelectedSessionId,
    sessionDetails,
    detailsLoading,
    interruptMessage,
    setInterruptMessage,
    interruptLoading,
    handleSelectAnalysisSession,
    handleInterruptAgent,
  };
}
