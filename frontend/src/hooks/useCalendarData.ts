import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DailyUsageDetail, DayCostBreakdown } from "../types";
import { monthRange } from "../utils/calendar";
import { dbUpdateBus } from "../lib/dbUpdateBus";

/**
 * 캘린더 뷰 데이터 훅
 *
 * 지정한 연/월(month=0~11)의 일별 토큰·비용 사용량을 백엔드 `get_daily_usage_in_range` 로 조회한다.
 * - 월 변경 시 자동 refetch
 * - 백엔드 `db-updated` 이벤트(증분/강제 동기화, 로그 변경 감지) 발생 시 현재 월 자동 갱신
 *   (useTrackerData 의 리스너 패턴과 동일)
 */
export function useCalendarData(year: number, month: number) {
  const [data, setData] = useState<DailyUsageDetail[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setLoading(true);
      try {
        const { start, end } = monthRange(year, month);
        const rows = await invoke<DailyUsageDetail[]>("get_daily_usage_in_range", {
          startDate: start,
          endDate: end,
        });
        if (!cancelled) {
          setData(rows);
          setError(null);
        }
      } catch (err: any) {
        if (!cancelled) setError(err?.toString() ?? "캘린더 데이터 로드 실패");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    load();

    // db-updated(증분/강제 동기화, 로그 변경 감지)는 디바운스 버스를 통해 갱신
    const unsubscribe = dbUpdateBus.subscribe(load);

    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [year, month]);

  // 날짜키("YYYY-MM-DD") → 레코드 빠른 조회용 Map
  const byDate = useMemo(() => {
    const m = new Map<string, DailyUsageDetail>();
    for (const row of data) m.set(row.date, row);
    return m;
  }, [data]);

  return { data, byDate, loading, error };
}

/**
 * 특정 일자의 플러그인별·도구별 비용 랭킹 조회 훅 (캘린더 상세 모달용)
 *
 * date 가 null 이면 조회하지 않는다(모달 닫힘 상태). date 변경 시마다 refetch.
 */
export function useDayCostBreakdown(date: string | null) {
  const [data, setData] = useState<DayCostBreakdown | null>(null);
  const [loading, setLoading] = useState<boolean>(false);

  useEffect(() => {
    if (!date) {
      setData(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    invoke<DayCostBreakdown>("get_day_cost_breakdown", { date })
      .then((res) => {
        if (!cancelled) setData(res);
      })
      .catch(() => {
        if (!cancelled) setData(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [date]);

  return { data, loading };
}
