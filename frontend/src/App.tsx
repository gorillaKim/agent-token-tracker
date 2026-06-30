import { lazy, Suspense } from "react";

/**
 * 앱 진입 라우터.
 *
 * URL의 `?mode=tray` 여부로 트레이 팝오버와 메인 창을 분기한다. 두 화면을 각각 별도 청크로
 * lazy 로딩해, 작은 트레이 팝오버가 무거운 메인 쉘(대시보드/차트/캘린더/설정)을 들고 오지 않게 한다.
 */
const MainApp = lazy(() => import("./MainApp"));
const TrayPopoverView = lazy(() =>
  import("./views/TrayPopoverView").then((m) => ({ default: m.TrayPopoverView }))
);

function App() {
  const isTrayMode = new URLSearchParams(window.location.search).get("mode") === "tray";

  return (
    <Suspense fallback={null}>
      {isTrayMode ? <TrayPopoverView /> : <MainApp />}
    </Suspense>
  );
}

export default App;
