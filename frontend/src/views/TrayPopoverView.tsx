import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AgentSummary, LoopDetectionResult, PlanQuotaInfo } from "../types";
import { formatTokens } from "../utils/formatters";
import { AgentQuotaCard } from "../components/AgentQuotaCard";

/**
 * 시스템 트레이 전용 팝오버 뷰 컴포넌트
 * 
 * 트레이 아이콘을 클릭했을 때 나타나는 소형 팝업 UI를 담당하며,
 * 실시간 오작동 상태 및 에이전트별 토큰 쿼터 상황을 콤팩트하게 제공합니다.
 */
export function TrayPopoverView() {
  const [summaries, setSummaries] = useState<AgentSummary[]>([]);
  const [anomalies, setAnomalies] = useState<LoopDetectionResult[]>([]);
  const [quotas, setQuotas] = useState<PlanQuotaInfo[]>([]);
  const [tokenDisplayMode, setTokenDisplayMode] = useState<string>("tokens");
  const [refreshInterval, setRefreshInterval] = useState<number>(3);
  const [loading, setLoading] = useState(true);

  const loadData = async () => {
    try {
      const sums = await invoke<AgentSummary[]>("get_agent_summaries");
      const anoms = await invoke<LoopDetectionResult[]>("get_loop_signals");
      const qts = await invoke<PlanQuotaInfo[]>("get_subscription_quota");
      
      try {
        const appSettings = await invoke<any>("load_settings");
        if (appSettings && appSettings.token_display_mode) {
          setTokenDisplayMode(appSettings.token_display_mode);
        }
        if (appSettings && typeof appSettings.refresh_interval === "number") {
          setRefreshInterval(appSettings.refresh_interval);
        }
      } catch (e) {
        console.error("설정 로드 실패:", e);
      }

      setSummaries(sums);
      setAnomalies(anoms);
      setQuotas(qts);
    } catch (e) {
      console.error("데이터 로드 실패:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
    const unlistenPromise = listen("db-updated", () => {
      loadData();
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, []);

  // 설정된 주기(분)마다 자동 갱신 (0이면 끔)
  useEffect(() => {
    if (!refreshInterval || refreshInterval <= 0) return;
    const id = setInterval(() => {
      loadData();
    }, refreshInterval * 60 * 1000);
    return () => clearInterval(id);
  }, [refreshInterval]);

  const totalAnomalies = anomalies.length;

  const handleBannerClick = async () => {
    if (anomalies.length > 0) {
      const firstAnomalySessionId = anomalies[0].session_id;
      try {
        await invoke("focus_main_window", { sessionId: firstAnomalySessionId });
      } catch (e) {
        console.error("focus_main_window 호출 실패:", e);
      }
    }
  };

  return (
    <div className="tray-popover-container">
      <div className="tray-popover-header">
        <h4 className="tray-popover-title">에이전트 토큰 관측소</h4>
        <span style={{ fontSize: '0.75rem', fontWeight: 600, color: 'hsl(190, 90%, 55%)' }}>
          LIVE
        </span>
      </div>

      {totalAnomalies > 0 ? (
        <div className="tray-popover-banner" onClick={handleBannerClick} style={{ cursor: "pointer" }}>
          <span>⚠️</span>
          <span>{totalAnomalies}개의 오작동 세션 감지됨</span>
        </div>
      ) : (
        <div className="tray-popover-banner-green">
          <span>✓</span>
          <span>모든 프로세스 정상 작동 중</span>
        </div>
      )}

      <div className="tray-popover-list" style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        {loading ? (
          <div style={{ color: 'hsl(215, 20%, 45%)', fontSize: '0.75rem', textAlign: 'center', padding: '2rem 0' }}>
            로드 중...
          </div>
        ) : summaries.map((sum) => {
            let providerKey = "antigravity";
            if (sum.agent_type === "claude_code") providerKey = "anthropic";
            else if (sum.agent_type === "codex") providerKey = "openai";

            const quota = quotas.find(q => q.provider === providerKey);

            return (
              <AgentQuotaCard
                key={sum.agent_type}
                sum={sum}
                quota={quota}
                tokenDisplayMode={tokenDisplayMode}
                isDashboard={false}
                isExpanded={false}
                onToggleExpand={() => {}}
              />
            );
          })
        }
      </div>

      <div className="tray-popover-footer" style={{ borderTop: "1px solid rgba(255,255,255,0.05)", marginTop: "0.5rem", paddingTop: "0.5rem" }}>
        <span>오늘 누적 사용 토큰</span>
        <span style={{ fontWeight: 800, color: 'var(--neon-blue)' }}>
          {formatTokens(summaries.reduce((acc, curr) => acc + (curr.total_input_tokens + curr.total_output_tokens), 0))} Tokens
        </span>
      </div>
    </div>
  );
}
export default TrayPopoverView;
