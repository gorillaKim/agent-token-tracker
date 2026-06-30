import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import {
  Session,
  AgentSummary,
  LoopDetectionResult,
  DailyTokenUsage,
  HourlyTokenUsage,
  McpUsageTrend,
} from "../../types";
import { queryKeys } from "../../lib/queryKeys";
import { GC_TIME, STALE_TIME } from "../../lib/queryConfig";
import { usePollMs } from "./useSettingsQuery";

/** 활성 세션. days 미지정 = 전체(헤더/공유용), days 지정 = 기간 필터(대시보드 하단). */
export function useActiveSessions(days?: number) {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: queryKeys.activeSessions(days),
    queryFn: () =>
      invoke<Session[]>("get_active_sessions", days != null ? { days } : undefined),
    staleTime: STALE_TIME.DB,
    gcTime: GC_TIME.DEFAULT,
    refetchInterval,
    // 기간(1d/3d/7d) 전환 시 이전 데이터를 유지해 스켈레톤 깜빡임 방지
    placeholderData: keepPreviousData,
  });
}

export function useAgentSummaries() {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: queryKeys.agentSummaries(),
    queryFn: () => invoke<AgentSummary[]>("get_agent_summaries"),
    staleTime: STALE_TIME.DB,
    gcTime: GC_TIME.DEFAULT,
    refetchInterval,
  });
}

export function useLoopSignals(days?: number) {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: queryKeys.loopSignals(days),
    queryFn: () =>
      invoke<LoopDetectionResult[]>("get_loop_signals", days != null ? { days } : undefined),
    staleTime: STALE_TIME.DB,
    gcTime: GC_TIME.DEFAULT,
    refetchInterval,
    placeholderData: keepPreviousData,
  });
}

export function useDailyTokenUsage(days = 30) {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: queryKeys.dailyTokenUsage(days),
    queryFn: () => invoke<DailyTokenUsage[]>("get_daily_token_usage", { days }),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
    refetchInterval,
  });
}

export function useHourlyTokenUsage() {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: queryKeys.hourlyTokenUsage(),
    queryFn: () => invoke<HourlyTokenUsage[]>("get_hourly_token_usage"),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
    refetchInterval,
  });
}

export function useMcpUsageTrend(days: number) {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: ["db", "mcpUsageTrend", days],
    queryFn: () => invoke<McpUsageTrend[]>("get_mcp_usage_trend", { days }),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
    refetchInterval,
    placeholderData: keepPreviousData,
  });
}
