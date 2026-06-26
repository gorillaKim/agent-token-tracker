import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { DetectedCredential, DetectedLogPath } from "../../types";
import { queryKeys } from "../../lib/queryKeys";
import { GC_TIME, STALE_TIME } from "../../lib/queryConfig";

/** 저장된 API 키 보유 여부 (provider별 boolean) */
export function useApiKeysStatus() {
  return useQuery({
    queryKey: queryKeys.apiKeysStatus(),
    queryFn: () => invoke<Record<string, boolean>>("get_api_keys_status"),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });
}

/** 로컬 키체인/설정파일에서 자동 감지된 자격 증명 */
export function useLocalCredentials() {
  return useQuery({
    queryKey: queryKeys.localCredentials(),
    queryFn: () => invoke<DetectedCredential[]>("get_local_credentials"),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });
}

/** 에이전트별 세션 로그 경로 자동 감지 결과 */
export function useDetectedLogPaths() {
  return useQuery({
    queryKey: queryKeys.detectedLogPaths(),
    queryFn: () => invoke<DetectedLogPath[]>("get_detected_log_paths"),
    staleTime: STALE_TIME.USAGE,
    gcTime: GC_TIME.DEFAULT,
  });
}
