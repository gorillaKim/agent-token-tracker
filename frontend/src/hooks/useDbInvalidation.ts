import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { dbUpdateBus } from "../lib/dbUpdateBus";

/**
 * dbUpdateBus(freeze-while-viewing 게이팅 포함)의 단일 데이터 구독자.
 *
 * 백엔드 db-updated 가 (창을 보고 있지 않을 때 / blur flush 시 / 새로고침 버튼 클릭 시) 발화하면
 * DB 파생 쿼리(키[0] === "db")만 무효화한다. 느린 외부 쿼터/설정 쿼리는 건드리지 않아
 * fast/slow 분리가 유지된다. 무효화는 활성 옵저버가 있는 쿼리만 refetch 시키므로
 * 닫혀 있는 화면(예: 캘린더 모달)은 다음에 열릴 때 갱신된다.
 *
 * 창(webview)마다 한 번 호출한다 — 메인 창은 App, 트레이는 TrayPopoverView.
 */
export function useDbInvalidation() {
  const queryClient = useQueryClient();
  useEffect(() => {
    return dbUpdateBus.subscribe(() => {
      queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
    });
  }, [queryClient]);
}
