import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { SyncResult } from "../../types";
import { queryKeys } from "../../lib/queryKeys";

// 동기화 후에는 DB 파생 전체 + 느린 쿼터까지 갱신한다(기존 loadData=fast+slow 동작과 동일).
function invalidateAfterSync(queryClient: QueryClient) {
  queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
  queryClient.invalidateQueries({ queryKey: queryKeys.subscriptionQuota() });
}

// 기존 useTrackerData 의 결과 메시지/분기 로직을 그대로 보존(실패 건수>0 → warning).
function toastSyncResult(prefix: string, res: SyncResult) {
  const msg =
    `${prefix} ` +
    `(총 발견: ${res.files_total}개, ` +
    `신규 적재: ${res.sessions_inserted}개, ` +
    `중복 스킵: ${res.sessions_skipped}개, ` +
    `실패: ${res.sessions_failed}개)`;
  if (res.sessions_failed > 0) toast.warning(msg);
  else toast.success(msg);
}

/** 수동 증분 동기화 (sync_local_sessions) */
export function useSyncSessions() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => invoke<SyncResult>("sync_local_sessions"),
    onSuccess: (res) => {
      toastSyncResult("수동 증분 동기화 완료!", res);
      invalidateAfterSync(queryClient);
    },
    onError: (e) => toast.error(`동기화 실패: ${String(e)}`),
  });
}

/** 강제 전체 재스캔 (force_sync_local_sessions) */
export function useForceSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => invoke<SyncResult>("force_sync_local_sessions"),
    onSuccess: (res) => {
      toastSyncResult("강제 전체 동기화 완료!", res);
      invalidateAfterSync(queryClient);
    },
    onError: (e) => toast.error(`동기화 실패: ${String(e)}`),
  });
}
