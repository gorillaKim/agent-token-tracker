/**
 * React Query 캐시 시간 공용 상수 (ms).
 *
 * 모든 쿼리 훅이 staleTime/gcTime 을 여기서 가져와 재사용한다 — 캐시 정책을 한 곳에서 튜닝.
 * (DB 파생 쿼리의 실제 신선도는 db-updated 무효화(useDbInvalidation)가 보장하고,
 *  staleTime 은 폴링/탭 재진입 시 백그라운드 refetch 를 유발하는 보조 장치다.)
 */
export const STALE_TIME = {
  /** 세션/요약/루프 등 DB 파생 — 짧게 */
  DB: 10_000,
  /** 일별/시간별 토큰 사용량 */
  USAGE: 30_000,
  /** 느린 외부 구독 쿼터 */
  QUOTA: 60_000,
  /** 설정 — 저장 시점에만 변경 */
  SETTINGS: 5 * 60_000,
} as const;

export const GC_TIME = {
  /** 기본: 옵저버 해제 후 5분 유지 → 탭 재진입 시 즉시 캐시 표시 */
  DEFAULT: 5 * 60_000,
  QUOTA: 10 * 60_000,
  SETTINGS: 30 * 60_000,
} as const;
