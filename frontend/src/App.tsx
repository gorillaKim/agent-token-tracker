import { useEffect, useState } from "react";

// 1. 공통 타입 및 유틸리티 임포트
import { formatTokens, formatUsd } from "./utils/formatters";

// 2. 커스텀 훅 임포트
import { useTrackerData } from "./hooks/useTrackerData";
import { useSessionDetails } from "./hooks/useSessionDetails";

// 3. 분리된 화면 뷰 컴포넌트 임포트
import { DashboardView } from "./views/DashboardView";
import { CalendarView } from "./views/CalendarView";
import { SessionAnalysisView } from "./views/SessionAnalysisView";
import { SettingsView } from "./views/SettingsView";
import { TrayPopoverView } from "./views/TrayPopoverView";

// 4. 독립 UI 컴포넌트 임포트
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
import { toast } from "sonner";
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

/**
 * 에이전트 토큰 관측소(Agent Token Tracker) 메인 엔트리 컴포넌트
 * 
 * 모든 독립 컴포넌트를 통합 조정하며, 전역 상태(세션 목록, 누적 한도, 오작동 세션)와 
 * Tauri 백엔드 간 IPC(Invoke) 호출을 총괄합니다.
 */
function App() {
  const urlParams = new URLSearchParams(window.location.search);
  const isTrayMode = urlParams.get("mode") === "tray";

  if (isTrayMode) {
    return <TrayPopoverView />;
  }

  // 사이드바 접힘 상태
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(() => {
    return localStorage.getItem("sidebar-collapsed") === "true";
  });

  useEffect(() => {
    localStorage.setItem("sidebar-collapsed", isSidebarCollapsed.toString());
  }, [isSidebarCollapsed]);

  // 대시보드 활성 탭
  const [activeTab, setActiveTab] = useState<TabKey>("dashboard");
  
  // 1. 비즈니스 로직 훅 활용
  const {
    tokenDisplayMode,
    sessions,
    summaries,
    anomalies,
    dailyTokenUsage,
    hourlyTokenUsage,
    quotaInfo,
    error,
    syncLoading,
    syncStatus,
    setSyncStatus,
    showConfirmModal,
    setShowConfirmModal,
    loadData,
    handleSyncSessions,
    startForceSync,
  } = useTrackerData();

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
  } = useSessionDetails(sessions, loadData);

  const totalSessionsOverall = sessions.length;
  const totalTokensOverall = summaries.reduce((acc, curr) => acc + curr.total_input_tokens + curr.total_output_tokens, 0);

  // 동기화 상태/오류를 Sonner 토스트로 노출 (기존 손수 토스트 대체)
  // 완료 메시지는 "...실패: N개"를 포함하므로 단순 includes("실패")로 분기하면
  // 성공도 에러색으로 표시되는 버그가 있다 → 실패 '건수'로 정확히 분기.
  useEffect(() => {
    if (!syncStatus) return;
    const failedMatch = syncStatus.match(/실패:\s*(\d+)\s*개/);
    if (failedMatch) {
      const failed = Number(failedMatch[1]);
      if (failed > 0) toast.warning(syncStatus);
      else toast.success(syncStatus);
    } else if (syncStatus.includes("실패")) {
      // "동기화 실패: <에러>" 형태의 실제 예외 메시지
      toast.error(syncStatus);
    } else {
      toast.success(syncStatus);
    }
    setSyncStatus(null);
  }, [syncStatus, setSyncStatus]);

  useEffect(() => {
    if (error) toast.error(`오류: ${error}`);
  }, [error]);

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

        {/* 동기화 상태/오류는 Sonner 토스트(<Toaster/>)로 노출 — useEffect 배선 참조 */}

        {/* 탭 분기 렌더링 */}
        {activeTab === "dashboard" ? (
          <DashboardView
            summaries={summaries}
            dailyTokenUsage={dailyTokenUsage}
            hourlyTokenUsage={hourlyTokenUsage}
            quotaInfo={quotaInfo}
            tokenDisplayMode={tokenDisplayMode}
            setSelectedSessionId={setSelectedSessionId}
          />
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
          <SettingsView 
            onSettingsSaved={loadData} 
            activeSection={activeTab} 
          />
        )}
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

export default App;
