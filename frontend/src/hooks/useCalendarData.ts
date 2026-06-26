import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { DailyUsageDetail, DayCostBreakdown } from "../types";
import { monthRange } from "../utils/calendar";
import { queryKeys } from "../lib/queryKeys";
import { GC_TIME, STALE_TIME } from "../lib/queryConfig";

/**
 * 캘린더 뷰 데이터 훅 (React Query)
 *
 * 지정한 연/월(month=0~11)의 일별 토큰·비용 사용량을 `get_daily_usage_in_range` 로 조회한다.
 * - 월 변경 시 queryKey(start/end) 가 바뀌어 자동 refetch + 이전 월 캐시 유지
 * - db-updated(로그 변경/동기화)는 키가 ["db", ...] 이므로 앱 레벨 useDbInvalidation 이 자동 갱신
 *   (별도 구독 불필요)
 */
export function useCalendarData(year: number, month: number) {
  const { start, end } = monthRange(year, month);
  const query = useQuery({
    queryKey: queryKeys.dailyUsageInRange(start, end),
    queryFn: () =>
      invoke<DailyUsageDetail[]>("get_daily_usage_in_range", { startDate: start, endDate: end }),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });

  const data = query.data ?? [];

  // 날짜키("YYYY-MM-DD") → 레코드 빠른 조회용 Map
  const byDate = useMemo(() => {
    const m = new Map<string, DailyUsageDetail>();
    for (const row of data) m.set(row.date, row);
    return m;
  }, [query.data]);

  return {
    data,
    byDate,
    loading: query.isLoading,
    error: query.error ? String(query.error) : null,
  };
}

/**
 * 특정 일자의 플러그인별·도구별 비용 랭킹 조회 훅 (캘린더 상세 모달용)
 *
 * date 가 null 이면 조회하지 않는다(모달 닫힘 상태, enabled:false). date 변경 시 refetch.
 */
export function useDayCostBreakdown(date: string | null) {
  const query = useQuery({
    queryKey: queryKeys.dayCostBreakdown(date ?? "none"),
    queryFn: () => invoke<DayCostBreakdown>("get_day_cost_breakdown", { date }),
    enabled: !!date,
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });

  return { data: query.data ?? null, loading: query.isLoading };
}
