import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./globals.css";
import { mountTauriMockIfDev } from "./dev/tauriMock";

// dev(브라우저) 환경에서 Tauri invoke 목 설치 — 실제 Tauri/프로덕션에선 no-op.
mountTauriMockIfDev();

// 트레이 팝오버는 투명 윈도우(transparent:true)에 렌더된다.
// 전역 body{bg-background}가 윈도우를 불투명하게 채우는 것을 막기 위해
// 렌더 전에 동기적으로 html.tray 클래스를 부여한다(플래시 방지).
if (new URLSearchParams(window.location.search).get("mode") === "tray") {
  document.documentElement.classList.add("tray");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
