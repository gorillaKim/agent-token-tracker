import { lazy, Suspense, useCallback, useEffect, useState } from "react";

// 1. 공통 타입 및 유틸리티 임포트
import { formatTokens, formatUsd } from "./utils/formatters";

// 2. 커스텀 훅 임포트 (React Query 기반)
import { useSessionDetails } from "./hooks/useSessionDetails";
import { useQueryClient } from "@tanstack/react-query";
import { useActiveSessions, useAgentSummaries, useLoopSignals } from "./hooks/queries/useDbQueries";
import { useSyncSessions, useForceSync } from "./hooks/mutations/useSyncMutations";
import { useDbInvalidation } from "./hooks/useDbInvalidation";
import { queryKeys } from "./lib/queryKeys";

// 3. 화면 뷰는 lazy 로딩 — 메인 진입 청크를 가볍게 하고, 탭 진입 시에만 해당 청크(차트/캘린더/설정 등)를 로드.
//    트레이 팝오버는 App 라우터에서 별도 청크로 분리되어 이 무거운 뷰들을 전혀 들고 오지 않는다.
const DashboardView = lazy(() =>
  import("./views/DashboardView").then((m) => ({ default: m.DashboardView }))
);
const CalendarView = lazy(() =>
  import("./views/CalendarView").then((m) => ({ default: m.CalendarView }))
);
const SessionAnalysisView = lazy(() =>
  import("./views/SessionAnalysisView").then((m) => ({ default: m.SessionAnalysisView }))
);
const SettingsView = lazy(() =>
  import("./views/SettingsView").then((m) => ({ default: m.SettingsView }))
);

// 4. 독립 UI 컴포넌트 임포트 (메인 전용 — 항상 마운트)
import { SessionDrawer } from "./components/SessionDrawer";

// 5. 디자인 시스템 (shadcn/ui + lucide)
import {
  LayoutDashboard,
  CalendarDays,
  Search,
  Settings as SettingsIcon,
  Plug,
  Activity,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Zap,
  Loader2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Toaster } from "@/components/ui/sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

type TabKey = "dashboard" | "calendar" | "analysis" | "settings-general" | "settings-integrations";

const NAV_ITEMS: { key: TabKey; label: string; icon: typeof LayoutDashboard }[] = [
  { key: "dashboard", label: "실시간 관측판", icon: LayoutDashboard },
  { key: "calendar", label: "사용량 캘린더", icon: CalendarDays },
  { key: "analysis", label: "세션 심층 분석", icon: Search },
  { key: "settings-general", label: "화면 및 설정", icon: SettingsIcon },
  { key: "settings-integrations", label: "플랫폼 연동", icon: Plug },
];

/** lazy 뷰 청크 로딩 중 표시할 경량 폴백 */
function ViewLoading() {
  return (
    <div className="flex h-64 items-center justify-center">
      <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
    </div>
  );
}

/**
 * 메인 창(대시보드) 쉘 컴포넌트
 *
 * 사이드바·헤더·탭 라우팅·세션 드로어·동기화 다이얼로그를 총괄한다. App 라우터에서 lazy 로 로드되어
 * 트레이 모드에는 포함되지 않는다. 전역 상태(세션/요약/오작동)는 React Query 로 조회한다.
 */
export default function MainApp() {
  // 사이드바 접힘 상태
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(() => {
    return localStorage.getItem("sidebar-collapsed") === "true";
  });

  useEffect(() => {
    localStorage.setItem("sidebar-collapsed", isSidebarCollapsed.toString());
  }, [isSidebarCollapsed]);

  // 대시보드 활성 탭
  const [activeTab, setActiveTab] = useState<TabKey>("dashboard");

  // 1. 헤더/공유 데이터 — React Query (각 섹션 독립 로딩·캐싱)
  const queryClient = useQueryClient();
  const sessions = useActiveSessions().data ?? [];
  const summaries = useAgentSummaries().data ?? [];
  const anomalies = useLoopSignals().data ?? [];

  // db-updated → DB 파생 쿼리 무효화 (freeze-while-viewing 게이팅 유지)
  useDbInvalidation();

  // 세션 인터럽트/설정 저장 후 호출할 데이터 갱신(=무효화) 콜백
  const invalidateData = useCallback(async () => {
    await queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
    queryClient.invalidateQueries({ queryKey: queryKeys.subscriptionQuota() });
    queryClient.invalidateQueries({ queryKey: queryKeys.settings() });
  }, [queryClient]);

  // 동기화 mutation (loading/토스트는 mutation 내부에서 처리)
  const syncMutation = useSyncSessions();
  const forceSyncMutation = useForceSync();
  const syncLoading = syncMutation.isPending || forceSyncMutation.isPending;
  const [showConfirmModal, setShowConfirmModal] = useState(false);

  const handleSyncSessions = () => syncMutation.mutate();
  const startForceSync = () => {
    setShowConfirmModal(false);
    forceSyncMutation.mutate();
  };

  const {
    analysisSessionId,
    analysisData,
    analysisLoading,
    selectedSessionId,
    setSelectedSessionId,
    sessionDetails,
    detailsLoading,
    interruptMessage,
    interruptLoading,
    handleSelectAnalysisSession,
    handleInterruptAgent,
  } = useSessionDetails(sessions, invalidateData);

  const totalSessionsOverall = sessions.length;
  const totalTokensOverall = summaries.reduce((acc, curr) => acc + curr.total_input_tokens + curr.total_output_tokens, 0);

  // 동기화 성공/실패 토스트는 sync mutation(useSyncMutations) 내부에서 직접 노출한다.

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">

      {/* 1. 좌측 메인 사이드바 네비게이션 */}
      <aside
        className={cn(
          "flex shrink-0 flex-col border-r border-border bg-card/40 transition-[width] duration-300 ease-out",
          isSidebarCollapsed ? "w-[72px]" : "w-60"
        )}
      >
        {/* 로고 헤더 */}
        <div className="flex h-16 items-center gap-2.5 border-b border-border px-4">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/15 text-primary">
            <Activity className="h-[18px] w-[18px]" />
          </div>
          {!isSidebarCollapsed && (
            <span className="truncate text-sm font-semibold tracking-tight">Token Tracker</span>
          )}
        </div>

        {/* 네비게이션 메뉴 */}
        <nav className="flex flex-1 flex-col gap-1 p-2">
          {NAV_ITEMS.map(({ key, label, icon: Icon }) => {
            const active = activeTab === key;
            return (
              <button
                key={key}
                title={isSidebarCollapsed ? label : undefined}
                onClick={() => {
                  setActiveTab(key);
                  if (key === "analysis" && sessions.length > 0 && !analysisSessionId) {
                    handleSelectAnalysisSession(sessions[0].session_id);
                  }
                }}
                className={cn(
                  "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                  isSidebarCollapsed && "justify-center px-0",
                  active
                    ? "bg-accent text-foreground"
                    : "text-muted-foreground hover:bg-accent/60 hover:text-foreground"
                )}
              >
                <Icon className="h-[18px] w-[18px] shrink-0" />
                {!isSidebarCollapsed && <span className="truncate">{label}</span>}
              </button>
            );
          })}
        </nav>

        {/* 푸터: 접기 토글 + 버전 */}
        <div className="flex flex-col gap-1.5 border-t border-border p-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setIsSidebarCollapsed((v) => !v)}
            className={cn(
              "text-muted-foreground hover:text-foreground",
              isSidebarCollapsed ? "justify-center px-0" : "justify-start"
            )}
          >
            {isSidebarCollapsed ? (
              <PanelLeftOpen className="h-[18px] w-[18px]" />
            ) : (
              <>
                <PanelLeftClose className="h-[18px] w-[18px]" />
                <span>접기</span>
              </>
            )}
          </Button>
          {!isSidebarCollapsed && (
            <span className="px-2 text-[10px] text-muted-foreground/60">Version 0.1.0</span>
          )}
        </div>
      </aside>

      {/* 2. 우측 메인 콘텐츠 뷰 영역 */}
      <main className="flex-1 overflow-y-auto bg-background p-6 md:p-8">

        {/* 상단 통합 상태 메트릭 바 */}
        <header className="mb-6 flex flex-wrap items-center justify-between gap-4 rounded-xl border border-border bg-card/40 px-5 py-3.5">
          <div className="flex flex-wrap items-center gap-x-8 gap-y-2">
            <div className="flex flex-col">
              <span className="text-xs text-muted-foreground">연동 활성 세션</span>
              <span className="text-xl font-semibold tabular-nums">
                {totalSessionsOverall}
                <span className="ml-1 text-sm font-normal text-muted-foreground">건</span>
              </span>
            </div>
            <div className="flex flex-col">
              <span className="text-xs text-muted-foreground">총 토큰 누적량</span>
              <span className="text-xl font-semibold tabular-nums">
                {formatTokens(totalTokensOverall)}
                <span className="ml-1 text-sm font-normal text-muted-foreground">Tokens</span>
              </span>
            </div>
            <div className="flex flex-col">
              <span className="text-xs text-muted-foreground">추정 누적 비용</span>
              <span className="text-xl font-semibold tabular-nums text-primary">
                ${formatUsd(summaries.reduce((acc, curr) => acc + curr.total_cost_usd, 0))}
                <span className="ml-1 text-sm font-normal text-muted-foreground">USD</span>
              </span>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={handleSyncSessions} disabled={syncLoading}>
              {syncLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
              {syncLoading ? "동기화 중..." : "증분 동기화"}
            </Button>
            <Button variant="destructive" size="sm" onClick={() => setShowConfirmModal(true)} disabled={syncLoading}>
              <Zap className="h-4 w-4" />
              강제 재스캔
            </Button>
          </div>
        </header>

        {/* 동기화 상태/오류는 Sonner 토스트(<Toaster/>)로 노출 */}

        {/* 탭 분기 렌더링 (lazy 뷰 — Suspense 폴백) */}
        <Suspense fallback={<ViewLoading />}>
          {activeTab === "dashboard" ? (
            <DashboardView setSelectedSessionId={setSelectedSessionId} />
          ) : activeTab === "calendar" ? (
            <CalendarView />
          ) : activeTab === "analysis" ? (
            <SessionAnalysisView
              sessions={sessions}
              anomalies={anomalies}
              analysisSessionId={analysisSessionId}
              analysisData={analysisData}
              analysisLoading={analysisLoading}
              onSelectSession={handleSelectAnalysisSession}
              onInterrupt={handleInterruptAgent}
              interruptLoading={interruptLoading}
              interruptMessage={interruptMessage}
            />
          ) : (
            <SettingsView activeSection={activeTab} />
          )}
        </Suspense>
      </main>

      {/* 우측 세션 상세 정보 사이드 드로어 컴포넌트 마운트 */}
      <SessionDrawer
        selectedSessionId={selectedSessionId}
        setSelectedSessionId={setSelectedSessionId}
        sessions={sessions}
        anomalies={anomalies}
        sessionDetails={sessionDetails}
        detailsLoading={detailsLoading}
        interruptLoading={interruptLoading}
        interruptMessage={interruptMessage}
        onInterrupt={handleInterruptAgent}
      />

      {/* 강제 동기화 확인 AlertDialog */}
      <AlertDialog open={showConfirmModal} onOpenChange={setShowConfirmModal}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <Zap className="h-4 w-4 text-destructive" />
              강제 재스캔 경고
            </AlertDialogTitle>
            <AlertDialogDescription>
              강제 재스캔 시 이미 분석/저장된 에이전트 로그를 디스크에서 전부 새로 파싱하여
              덮어씁니다. 다소 시간이 소요될 수 있습니다. 진행하시겠습니까?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>취소</AlertDialogCancel>
            <AlertDialogAction
              onClick={startForceSync}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              확인
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* 동기화 진행 풀스크린 오버레이 */}
      {syncLoading && (
        <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-background/80 backdrop-blur-sm">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
          <p className="text-sm text-muted-foreground">세션 로그 동기화 중…</p>
        </div>
      )}

      {/* 전역 토스트 (Sonner) */}
      <Toaster richColors position="top-right" />
    </div>
  );
}
