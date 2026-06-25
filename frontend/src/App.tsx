import { useEffect, useState } from "react";

// 1. 공통 타입 및 유틸리티 임포트
import { formatTokens, formatUsd } from "./utils/formatters";

// 2. 커스텀 훅 임포트
import { useTrackerData } from "./hooks/useTrackerData";
import { useSessionDetails } from "./hooks/useSessionDetails";

// 3. 분리된 화면 뷰 컴포넌트 임포트
import { DashboardView } from "./views/DashboardView";
import { SessionAnalysisView } from "./views/SessionAnalysisView";
import { SettingsView } from "./views/SettingsView";
import { TrayPopoverView } from "./views/TrayPopoverView";

// 4. 독립 UI 컴포넌트 임포트
import { SessionDrawer } from "./components/SessionDrawer";

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
  const [activeTab, setActiveTab] = useState<"dashboard" | "analysis" | "settings-general" | "settings-integrations">("dashboard");
  
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

  return (
    <div className="app-container" style={{ display: "flex", width: "100vw", height: "100vh", overflow: "hidden" }}>
      
      {/* 1. 좌측 메인 사이드바 네비게이션 */}
      <aside className={`sidebar-container ${isSidebarCollapsed ? "collapsed" : ""}`} style={{ width: isSidebarCollapsed ? "80px" : "260px", background: "rgba(10, 12, 18, 0.95)", borderRight: "1px solid var(--card-border)", transition: "width 0.3s cubic-bezier(0.25, 0.8, 0.25, 1)" }}>
        <div className="sidebar-header" style={{ padding: "1.5rem 1rem", borderBottom: "1px solid rgba(255,255,255,0.05)", display: "flex", alignItems: "center", gap: "0.75rem" }}>
          <div className="sidebar-logo">
            <span className="logo-icon" style={{ fontSize: "1.5rem" }}>🛸</span>
            {!isSidebarCollapsed && <span className="sidebar-title-text" style={{ fontWeight: 800, fontSize: "1rem" }}>Token Tracker</span>}
          </div>
        </div>

        <nav className="sidebar-content" style={{ padding: "1rem 0.5rem" }}>
          <div className="sidebar-menu">
            <button onClick={() => setActiveTab("dashboard")} className={`sidebar-menu-button ${activeTab === "dashboard" ? "active" : ""}`}>
              <span className="menu-icon">📊</span>
              {!isSidebarCollapsed && <span className="menu-text">실시간 관측판</span>}
            </button>
            <button onClick={() => { setActiveTab("analysis"); if (sessions.length > 0 && !analysisSessionId) { handleSelectAnalysisSession(sessions[0].session_id); } }} className={`sidebar-menu-button ${activeTab === "analysis" ? "active" : ""}`}>
              <span className="menu-icon">🔍</span>
              {!isSidebarCollapsed && <span className="menu-text">세션 심층 분석</span>}
            </button>
            <button onClick={() => setActiveTab("settings-general")} className={`sidebar-menu-button ${activeTab === "settings-general" ? "active" : ""}`}>
              <span className="menu-icon">⚙️</span>
              {!isSidebarCollapsed && <span className="menu-text">화면 및 설정</span>}
            </button>
            <button onClick={() => setActiveTab("settings-integrations")} className={`sidebar-menu-button ${activeTab === "settings-integrations" ? "active" : ""}`}>
              <span className="menu-icon">🔗</span>
              {!isSidebarCollapsed && <span className="menu-text">플랫폼 연동</span>}
            </button>
          </div>
        </nav>

        <div className="sidebar-footer" style={{ padding: "1rem 0.5rem" }}>
          <button onClick={() => setIsSidebarCollapsed(!isSidebarCollapsed)} className="sidebar-toggle-button">
            {isSidebarCollapsed ? "▶" : "◀ 접기"}
          </button>
          {!isSidebarCollapsed && <div className="version-text" style={{ fontSize: "0.65rem", opacity: 0.4 }}>Version 0.1.0 (TDD Build)</div>}
        </div>
      </aside>

      {/* 2. 우측 메인 콘텐츠 뷰 영역 */}
      <main className={`main-content ${isSidebarCollapsed ? "expanded" : ""}`} style={{ flex: 1, overflowY: "auto", padding: "2rem", background: "hsl(240, 5%, 4%)" }}>
        
        {/* 상단 통합 상태 메트릭 바 */}
        <header className="statusbar">
          <div className="statusbar-metrics">
            <div className="metric-item">
              <span className="metric-label">연동 활성 세션</span>
              <span className="metric-value">{totalSessionsOverall} 건</span>
            </div>
            <div className="metric-item">
              <span className="metric-label">총 토큰 누적량</span>
              <span className="metric-value">{formatTokens(totalTokensOverall)} Tokens</span>
            </div>
            <div className="metric-item">
              <span className="metric-label">추정 누적 비용</span>
              <span className="metric-value" style={{ color: "var(--neon-purple)" }}>
                ${formatUsd(summaries.reduce((acc, curr) => acc + curr.total_cost_usd, 0))} USD
              </span>
            </div>
          </div>

          <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
            <button onClick={handleSyncSessions} disabled={syncLoading} className="btn" style={{ padding: "0.4rem 1rem", fontSize: "0.8rem", background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.1)", color: "#fff", fontWeight: 700 }}>
              {syncLoading ? "동기화 중..." : "🔄 증분 동기화"}
            </button>
            <button onClick={() => setShowConfirmModal(true)} disabled={syncLoading} className="btn" style={{ padding: "0.4rem 1rem", fontSize: "0.8rem", background: "rgba(239, 68, 68, 0.1)", border: "1px solid rgba(239, 68, 68, 0.2)", color: "hsl(0, 100%, 75%)", fontWeight: 700 }}>
              ⚡ 강제 재스캔
            </button>
          </div>
        </header>

        {/* 토스트 피드백 메시지 */}
        <div className="toast-container">
          {syncStatus && (
            <div className={`toast ${syncStatus.includes("실패") ? "toast-error" : "toast-success"}`}>
              <div className="toast-content">
                <span className="toast-icon">{syncStatus.includes("실패") ? "❌" : "✅"}</span>
                <span>{syncStatus}</span>
              </div>
              <button className="toast-close-btn" onClick={() => setSyncStatus(null)}>×</button>
            </div>
          )}
        </div>

        {error && <div style={{ color: "hsl(0, 100%, 65%)", marginBottom: "1rem", fontWeight: "600" }}>⚠️ 오류: {error}</div>}

        {/* 탭 분기 렌더링 */}
        {activeTab === "dashboard" ? (
          <DashboardView 
            sessions={sessions}
            summaries={summaries}
            anomalies={anomalies}
            dailyTokenUsage={dailyTokenUsage}
            hourlyTokenUsage={hourlyTokenUsage}
            quotaInfo={quotaInfo}
            tokenDisplayMode={tokenDisplayMode}
            setSelectedSessionId={setSelectedSessionId}
          />
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

      {/* 강제 동기화 확인 컨펌 모달 */}
      {showConfirmModal && (
        <div style={{ position: "fixed", top: 0, left: 0, width: "100vw", height: "100vh", background: "rgba(4,6,12,0.8)", zIndex: 200, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <div className="glass" style={{ width: "380px", padding: "1.5rem", borderRadius: "12px", border: "1px solid rgba(255,255,255,0.08)", background: "rgba(10,12,18,0.98)" }}>
            <h4 style={{ margin: "0 0 1rem 0", fontSize: "1rem", color: "#fff", fontWeight: 700 }}>⚡ 강제 재스캔 경고</h4>
            <p style={{ margin: "0 0 1.5rem 0", fontSize: "0.8rem", color: "hsl(215, 20%, 75%)", lineHeight: 1.5 }}>
              강제 재스캔 시 이미 분석/저장된 에이전트 로그를 디스크에서 전부 새로 파싱하여 덮어씁니다. 다소 시간이 소요될 수 있습니다. 진행하시겠습니까?
            </p>
            <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end" }}>
              <button onClick={() => setShowConfirmModal(false)} className="btn" style={{ padding: "0.35rem 0.85rem", fontSize: "0.75rem", background: "rgba(255,255,255,0.03)", color: "hsl(215, 20%, 75%)" }}>
                취소
              </button>
              <button onClick={startForceSync} className="btn" style={{ padding: "0.35rem 0.85rem", fontSize: "0.75rem", background: "var(--neon-blue)", color: "#0a0c10", fontWeight: 700 }}>
                확인
              </button>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}

export default App;
