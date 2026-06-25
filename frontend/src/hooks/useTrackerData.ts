import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { 
  Session, 
  AgentSummary, 
  LoopDetectionResult, 
  DailyTokenUsage, 
  HourlyTokenUsage, 
  PlanQuotaInfo 
} from "../types";

/**
 * 대시보드 및 앱 전반에서 사용되는 토큰 트래커 실시간 정보 상태와 비즈니스 로직을 제공하는 커스텀 훅
 */
export function useTrackerData() {
  const [tokenDisplayMode, setTokenDisplayMode] = useState<string>("tokens");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [summaries, setSummaries] = useState<AgentSummary[]>([]);
  const [anomalies, setAnomalies] = useState<LoopDetectionResult[]>([]);
  const [dailyTokenUsage, setDailyTokenUsage] = useState<DailyTokenUsage[]>([]);
  const [hourlyTokenUsage, setHourlyTokenUsage] = useState<HourlyTokenUsage[]>([]);
  const [quotaInfo, setQuotaInfo] = useState<PlanQuotaInfo[]>([]);

  const [error, setError] = useState<string | null>(null);
  const [syncLoading, setSyncLoading] = useState(false);
  const [syncStatus, setSyncStatus] = useState<string | null>(null);
  const [showConfirmModal, setShowConfirmModal] = useState(false);

  // 백엔드 데이터 동기화
  async function loadData() {
    try {
      const sessList = await invoke<Session[]>("get_active_sessions");
      setSessions(sessList);

      const sumList = await invoke<AgentSummary[]>("get_agent_summaries");
      setSummaries(sumList);

      const anomalyList = await invoke<LoopDetectionResult[]>("get_loop_signals");
      setAnomalies(anomalyList);

      const dailyTokens = await invoke<DailyTokenUsage[]>("get_daily_token_usage", { days: 30 });
      setDailyTokenUsage(dailyTokens);

      const hourlyTokens = await invoke<HourlyTokenUsage[]>("get_hourly_token_usage");
      setHourlyTokenUsage(hourlyTokens);

      const quota = await invoke<PlanQuotaInfo[]>("get_subscription_quota");
      setQuotaInfo(quota);

      const settings = await invoke<{ token_display_mode: string }>("load_settings");
      setTokenDisplayMode(settings.token_display_mode || "tokens");
    } catch (err: any) {
      setError(err.toString());
    }
  }

  // 수동 증분 동기화
  const handleSyncSessions = async () => {
    setSyncLoading(true);
    setSyncStatus(null);
    try {
      const res = await invoke<{
        files_total: number;
        sessions_inserted: number;
        sessions_skipped: number;
        sessions_failed: number;
      }>("sync_local_sessions");
      
      setSyncStatus(
        `수동 증분 동기화 완료! ` +
        `(총 발견: ${res.files_total}개, ` +
        `신규 적재: ${res.sessions_inserted}개, ` +
        `중복 스킵: ${res.sessions_skipped}개, ` +
        `실패: ${res.sessions_failed}개)`
      );
      await loadData();
    } catch (e: any) {
      setSyncStatus(`동기화 실패: ${e.toString()}`);
    } finally {
      setSyncLoading(false);
    }
  };

  // 강제 전체 동기화
  const startForceSync = async () => {
    setSyncLoading(true);
    setSyncStatus(null);
    setShowConfirmModal(false);
    try {
      const res = await invoke<{
        files_total: number;
        sessions_inserted: number;
        sessions_skipped: number;
        sessions_failed: number;
      }>("force_sync_local_sessions");
      
      setSyncStatus(
        `강제 전체 동기화 완료! ` +
        `(총 발견: ${res.files_total}개, ` +
        `신규 적재: ${res.sessions_inserted}개, ` +
        `중복 스킵: ${res.sessions_skipped}개, ` +
        `실패: ${res.sessions_failed}개)`
      );
      await loadData();
    } catch (e: any) {
      setSyncStatus(`동기화 실패: ${e.toString()}`);
    } finally {
      setSyncLoading(false);
    }
  };

  // 동기화 완료 알림 자동 닫기 (5초)
  useEffect(() => {
    if (syncStatus) {
      const timer = setTimeout(() => {
        setSyncStatus(null);
      }, 5000);
      return () => clearTimeout(timer);
    }
  }, [syncStatus]);

  // 마운트 시 초기화 및 백엔드 스캔 리스너 연동
  useEffect(() => {
    loadData();
    const unlistenPromise = listen("db-updated", () => {
      loadData();
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, []);

  return {
    tokenDisplayMode,
    sessions,
    summaries,
    anomalies,
    dailyTokenUsage,
    hourlyTokenUsage,
    quotaInfo,
    error,
    setError,
    syncLoading,
    syncStatus,
    setSyncStatus,
    showConfirmModal,
    setShowConfirmModal,
    loadData,
    handleSyncSessions,
    startForceSync,
  };
}
