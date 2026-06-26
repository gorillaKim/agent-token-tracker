import { QueryClient } from "@tanstack/react-query";

/**
 * 앱 전역 단일 QueryClient (모듈 싱글톤).
 *
 * 트레이 팝오버는 별도 webview(별도 JS 모듈 그래프)이므로 자기만의 싱글톤을 갖게 되어
 * 창 간 캐시는 자연스럽게 분리된다(의도된 동작).
 *
 * refetchOnWindowFocus 는 끈다 — 포커스 시 갱신 정책은 dbUpdateBus 의
 * freeze-while-viewing 이 단독으로 소유한다. 둘 다 켜면 서로 충돌/중복 fetch 가 발생한다.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000, // 키별 override; 기본 floor
      gcTime: 5 * 60_000, // 옵저버 해제 후 5분 캐시 유지 → 탭 재진입 시 즉시 표시
      retry: 1,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
    },
    mutations: {
      retry: 0,
    },
  },
});
