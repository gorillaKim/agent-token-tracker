import React, { Suspense } from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./globals.css";
import { mountTauriMockIfDev } from "./dev/tauriMock";
import { queryClient } from "./lib/queryClient";

// dev(브라우저) 환경에서 Tauri invoke 목 설치 — 실제 Tauri/프로덕션에선 no-op.
mountTauriMockIfDev();

// 트레이 팝오버는 투명 윈도우(transparent:true)에 렌더된다.
// 전역 body{bg-background}가 윈도우를 불투명하게 채우는 것을 막기 위해
// 렌더 전에 동기적으로 html.tray 클래스를 부여한다(플래시 방지).
const isTrayMode = new URLSearchParams(window.location.search).get("mode") === "tray";
if (isTrayMode) {
  document.documentElement.classList.add("tray");
}

// React Query Devtools 는 dev + 메인 창에서만 로드한다(프로덕션 번들 제외, 트레이 레이아웃 보호).
const ReactQueryDevtools = import.meta.env.DEV
  ? React.lazy(() =>
      import("@tanstack/react-query-devtools").then((m) => ({
        default: m.ReactQueryDevtools,
      }))
    )
  : null;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
      {ReactQueryDevtools && !isTrayMode && (
        <Suspense fallback={null}>
          <ReactQueryDevtools initialIsOpen={false} />
        </Suspense>
      )}
    </QueryClientProvider>
  </React.StrictMode>
);
