import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { PlanQuotaInfo } from "../../types";
import { queryKeys } from "../../lib/queryKeys";
import { GC_TIME, STALE_TIME } from "../../lib/queryConfig";
import { usePollMs } from "./useSettingsQuery";

/**
 * 구독 쿼터(get_subscription_quota) — 느린 외부 API(1~3초).
 * 자기만의 로딩 상태를 가지므로 대시보드에서 빠른 DB 카드와 독립적으로 채워진다.
 * db-updated 로는 무효화되지 않고 폴링/수동 갱신/설정 저장 시에만 갱신된다.
 */
export function useSubscriptionQuota() {
  const refetchInterval = usePollMs();
  return useQuery({
    queryKey: queryKeys.subscriptionQuota(),
    queryFn: () => invoke<PlanQuotaInfo[]>("get_subscription_quota"),
    staleTime: STALE_TIME.QUOTA,
    gcTime: GC_TIME.QUOTA,
    refetchInterval,
  });
}
