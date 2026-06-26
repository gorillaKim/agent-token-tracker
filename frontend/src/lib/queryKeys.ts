/**
 * React Query 키 팩토리 (타입드).
 *
 * **핵심 계약**: DB 파생(로컬 DB에서 읽어오는, 로그 변경=db-updated 시 갱신돼야 하는) 쿼리는
 * 반드시 키가 `["db", ...]` 로 시작한다. 외부 API(느린 구독 쿼터)는 `["external", ...]`,
 * 설정/진단은 `["config", ...]` 로 시작한다.
 *
 * dbUpdateBus 구독자(useDbInvalidation)는 `predicate: q => q.queryKey[0] === "db"` 로
 * DB 파생만 정확히 무효화하고 느린 외부 쿼터/설정은 건드리지 않는다(fast/slow 분리 유지).
 * 모든 호출부는 인라인 키 대신 이 팩토리만 사용해야 한다.
 */
export const queryKeys = {
  // --- DB 파생 (fast, db-updated 로 무효화) ---
  activeSessions: (days?: number) => ["db", "activeSessions", { days: days ?? null }] as const,
  agentSummaries: () => ["db", "agentSummaries"] as const,
  loopSignals: (days?: number) => ["db", "loopSignals", { days: days ?? null }] as const,
  dailyTokenUsage: (days: number) => ["db", "dailyTokenUsage", { days }] as const,
  hourlyTokenUsage: () => ["db", "hourlyTokenUsage"] as const,
  dailyUsageInRange: (start: string, end: string) =>
    ["db", "dailyUsageInRange", { start, end }] as const,
  dayCostBreakdown: (date: string) => ["db", "dayCostBreakdown", { date }] as const,
  sessionAnalysis: (sessionId: string) => ["db", "sessionAnalysis", { sessionId }] as const,
  sessionDetails: (sessionId: string) => ["db", "sessionDetails", { sessionId }] as const,

  // --- 느린 외부 API (db-updated 로 무효화하지 않음) ---
  subscriptionQuota: () => ["external", "subscriptionQuota"] as const,

  // --- 설정/진단 ---
  settings: () => ["config", "settings"] as const,
  apiKeysStatus: () => ["config", "apiKeysStatus"] as const,
  localCredentials: () => ["config", "localCredentials"] as const,
  detectedLogPaths: () => ["config", "detectedLogPaths"] as const,
} as const;
