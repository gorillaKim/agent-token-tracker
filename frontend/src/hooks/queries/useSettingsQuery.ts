import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { SettingsDto } from "../../types";
import { queryKeys } from "../../lib/queryKeys";
import { GC_TIME, STALE_TIME } from "../../lib/queryConfig";

/**
 * 앱 설정(load_settings) 쿼리. token_display_mode / refresh_interval 등을 제공한다.
 * 거의 바뀌지 않고 저장 시점에만 무효화되므로 stale time 을 길게 둔다.
 */
export function useSettings() {
  return useQuery({
    queryKey: queryKeys.settings(),
    queryFn: () => invoke<SettingsDto>("load_settings"),
    staleTime: STALE_TIME.SETTINGS,
    gcTime: GC_TIME.SETTINGS,
  });
}

/**
 * load_settings.refresh_interval(분; 0=끔)을 React Query refetchInterval(ms | false)로 변환.
 * 폴링이 필요한 쿼리들이 이 값을 그대로 refetchInterval 로 사용한다.
 */
export function usePollMs(): number | false {
  const { data } = useSettings();
  const minutes = data?.refresh_interval ?? 0;
  return minutes > 0 ? minutes * 60_000 : false;
}
