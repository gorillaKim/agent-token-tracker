import { useMutation, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";

export function useDismissSessionMalfunctions() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (variables: { sessionId: string; isFp: boolean }) =>
      invoke<void>("dismiss_session_malfunctions", {
        sessionId: variables.sessionId,
        isFp: variables.isFp,
      }),
    onSuccess: (_, variables) => {
      if (variables.isFp) {
        toast.success("이상증상 오작동 마크를 해제(FP)했습니다.");
      } else {
        toast.success("오작동 탐지 상태로 복원했습니다.");
      }
      // "db" 로 시작하는 쿼리 무효화 (실시간 루프 쿼리 갱신)
      queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
    },
    onError: (e) => toast.error(`상태 변경 실패: ${String(e)}`),
  });
}
