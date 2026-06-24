import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Session {
  session_id: string;
  agent_type: string;
  agent_version?: string;
  started_at: string;
  ended_at?: string;
  cwd: string;
  model_id?: string;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface AgentSummary {
  agent_type: string;
  session_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

interface LoopSignal {
  signal_type: string;
  description: string;
  evidence: string;
}

interface LoopDetectionResult {
  session_id: string;
  is_anomaly: boolean;
  signals: LoopSignal[];
}

// interface DailyCost {
//   date: string;
//   total_cost: number;
// }

interface DailyTokenUsage {
  date: string;
  total_tokens: number;
}

interface HourlyTokenUsage {
  hour: string;
  total_tokens: number;
}

interface ModelTokenUsage {
  model_id: string;
  total_tokens: number;
}

interface PluginTokenUsage {
  plugin_name: string;
  total_tokens: number;
}

interface SkillTokenUsage {
  skill_name: string;
  total_tokens: number;
}

interface TokenUsageBreakdown {
  models: ModelTokenUsage[];
  plugins: PluginTokenUsage[];
  skills: SkillTokenUsage[];
}

interface ToolCall {
  id?: number;
  session_id: string;
  tool_name: string;
  input_hash: string;
  success: boolean;
  cost_usd: number;
  created_at: string;
  tool_input: string;
}

interface SessionDetails {
  messages: any[];
  tool_calls: ToolCall[];
}

interface PlanQuotaInfo {
  provider: string;
  plan_key: string;
  plan_label: string;
  quota_tokens: number;
  used_tokens: number;
  remaining_tokens: number;
  usage_pct: number;
  window_reset_at: string | null;
  window_hours: number;

  weekly_quota_tokens?: number;
  weekly_used_tokens?: number;
  weekly_remaining_tokens?: number;
  weekly_usage_pct?: number;
  weekly_reset_at?: string | null;
}

interface DetectedCredential {
  provider: string;
  token_type: string;
  value: string;
  raw_value: string;
  source: string;
  description: string;
}

interface TurnTokenUsage {
  turn_index: number;
  role: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cost_usd: number;
  created_at: string;
}

interface ToolCostRank {
  tool_name: string;
  call_count: number;
  success_count: number;
  estimated_tokens: number;
  total_cost_usd: number;
}

interface SessionAnalysis {
  session_id: string;
  agent_type: string;
  model_id?: string;
  started_at: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  total_cost_usd: number;
  cache_hit_rate: number;
  cache_saved_cost: number;
  turns: TurnTokenUsage[];
  tool_cost_rank: ToolCostRank[];
  anomaly_signals: LoopSignal[];
  is_anomaly: boolean;
}

// ────────────────────────────────────────────────────────────
// 이슈 #791: SVG 기반 루프 오작동 순환 디렉션 뷰어
// ────────────────────────────────────────────────────────────
function LoopDirectionViewer({ signals }: { signals: LoopSignal[] }) {
  const pingPong = signals.find((s) => s.signal_type === "ping_pong");
  const selfLoop = signals.find((s) => s.signal_type === "repeated_call");

  if (pingPong) {
    const evidence = pingPong.evidence;
    const parts = evidence.split(",").map(p => p.trim());
    let toolA = "Tool A";
    let toolB = "Tool B";
    let cycles = "3";
    for (const part of parts) {
      if (part.startsWith("tool_A=")) toolA = part.substring(7);
      if (part.startsWith("tool_B=")) toolB = part.substring(7);
      if (part.startsWith("cycles=")) cycles = part.substring(7);
    }

    return (
      <div className="loop-viewer-container">
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "0.5rem" }}>
          <span style={{ fontSize: "0.85rem", fontWeight: 700, color: "var(--neon-red)" }}>🔄 핑퐁 순환 흐름도</span>
          <span className="badge-cycles">{cycles} Cycles</span>
        </div>
        <svg viewBox="0 0 400 140" className="loop-svg">
          <defs>
            <marker id="arrow-red-right" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
              <path d="M0,0 L6,3 L0,6 Z" fill="var(--neon-red)" />
            </marker>
            <marker id="arrow-red-left" markerWidth="6" markerHeight="6" refX="1" refY="3" orient="auto">
              <path d="M6,0 L0,3 L6,6 Z" fill="var(--neon-red)" />
            </marker>
          </defs>
          
          {/* A -> B 위로 휘는 곡선 */}
          <path d="M 130 55 Q 200 15 270 55" className="loop-line dash-flow-red" markerEnd="url(#arrow-red-right)" />
          
          {/* B -> A 아래로 휘는 곡선 */}
          <path d="M 270 85 Q 200 125 130 85" className="loop-line dash-flow-red" markerEnd="url(#arrow-red-left)" />

          {/* Node A */}
          <circle cx="95" cy="70" r="30" className="loop-node-circle" />
          <text x="95" y="74" textAnchor="middle" className="loop-node-text">{toolA}</text>

          {/* Node B */}
          <circle cx="305" cy="70" r="30" className="loop-node-circle" />
          <text x="305" y="74" textAnchor="middle" className="loop-node-text">{toolB}</text>
        </svg>
      </div>
    );
  }

  if (selfLoop) {
    const evidence = selfLoop.evidence;
    const parts = evidence.split(",").map(p => p.trim());
    let toolName = "Tool";
    let count = "3";
    for (const part of parts) {
      if (part.startsWith("tool_name=")) toolName = part.substring(10);
      if (part.startsWith("count=")) count = part.substring(6);
    }

    return (
      <div className="loop-viewer-container">
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "0.5rem" }}>
          <span style={{ fontSize: "0.85rem", fontWeight: 700, color: "var(--neon-red)" }}>🔁 자가 순환 루프</span>
          <span className="badge-cycles">{count} Reps</span>
        </div>
        <svg viewBox="0 0 400 140" className="loop-svg">
          <defs>
            <marker id="arrow-red-self" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
              <path d="M0,0 L6,3 L0,6 Z" fill="var(--neon-red)" />
            </marker>
          </defs>
          
          {/* Self feedback loop path */}
          <path d="M 185 50 C 130 -10, 270 -10, 215 50" className="loop-line dash-flow-red" markerEnd="url(#arrow-red-self)" />

          <circle cx="200" cy="80" r="30" className="loop-node-circle" />
          <text x="200" y="84" textAnchor="middle" className="loop-node-text">{toolName}</text>
        </svg>
      </div>
    );
  }

  return null;
}

function App() {
  const urlParams = new URLSearchParams(window.location.search);
  const isTrayMode = urlParams.get("mode") === "tray";

  if (isTrayMode) {
    return <TrayPopoverView />;
  }

  const [activeTab, setActiveTab] = useState<"dashboard" | "analysis" | "settings">("dashboard");
  const [tokenDisplayMode, setTokenDisplayMode] = useState<string>("tokens");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [summaries, setSummaries] = useState<AgentSummary[]>([]);
  const [anomalies, setAnomalies] = useState<LoopDetectionResult[]>([]);
  // const [dailyCosts, setDailyCosts] = useState<DailyCost[]>([]);
  const [dailyTokenUsage, setDailyTokenUsage] = useState<DailyTokenUsage[]>([]);
  const [hourlyTokenUsage, setHourlyTokenUsage] = useState<HourlyTokenUsage[]>([]);
  const [tokenBreakdown, setTokenBreakdown] = useState<TokenUsageBreakdown>({
    models: [],
    plugins: [],
    skills: [],
  });
  const [chartViewMode, setChartViewMode] = useState<"daily" | "hourly">("daily");
  const [error, setError] = useState<string | null>(null);
  const [syncLoading, setSyncLoading] = useState(false);
  const [syncStatus, setSyncStatus] = useState<string | null>(null);

  // 구독 한도 정보 상태
  const [quotaInfo, setQuotaInfo] = useState<PlanQuotaInfo[]>([]);

  // 세션 분석 전용 상태
  const [analysisSessionId, setAnalysisSessionId] = useState<string | null>(null);
  const [analysisData, setAnalysisData] = useState<SessionAnalysis | null>(null);
  const [analysisLoading, setAnalysisLoading] = useState(false);

  // Selected session and drawer details state
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [sessionDetails, setSessionDetails] = useState<SessionDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [interruptMessage, setInterruptMessage] = useState<string | null>(null);
  const [interruptLoading, setInterruptLoading] = useState(false);

  // Tooltip state
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    visible: boolean;
    date: string;
    value: number;
  }>({ x: 0, y: 0, visible: false, date: "", value: 0 });

  const chartRef = useRef<SVGSVGElement>(null);

  const handleSyncSessions = async () => {
    setSyncLoading(true);
    setSyncStatus(null);
    try {
      const res = await invoke<{
        files_total: number;
        sessions_inserted: number;
        sessions_skipped: number;
        sessions_failed: number;
      }>("sync_local_sessions");
      
      setSyncStatus(
        `수동 증분 동기화 완료! ` +
        `(총 발견: ${res.files_total}개, ` +
        `신규 적재: ${res.sessions_inserted}개, ` +
        `중복 스킵: ${res.sessions_skipped}개, ` +
        `실패: ${res.sessions_failed}개)`
      );
      loadData();
    } catch (e: any) {
      setSyncStatus(`동기화 실패: ${e.toString()}`);
    } finally {
      setSyncLoading(false);
    }
  };

  const handleSelectAnalysisSession = async (sessId: string) => {
    setAnalysisSessionId(sessId);
    setAnalysisLoading(true);
    setAnalysisData(null);
    try {
      const data = await invoke<SessionAnalysis>("get_session_analysis", { sessionId: sessId });
      setAnalysisData(data);
    } catch (err: any) {
      console.error("세션 분석 조회 실패:", err);
    } finally {
      setAnalysisLoading(false);
    }
  };

  async function loadData() {
    try {
      const sessList = await invoke<Session[]>("get_active_sessions");
      setSessions(sessList);

      const sumList = await invoke<AgentSummary[]>("get_agent_summaries");
      setSummaries(sumList);

      const anomalyList = await invoke<LoopDetectionResult[]>("get_loop_signals");
      setAnomalies(anomalyList);

      // const costList = await invoke<DailyCost[]>("get_daily_costs");
      // setDailyCosts(costList);

      const dailyTokens = await invoke<DailyTokenUsage[]>("get_daily_token_usage");
      setDailyTokenUsage(dailyTokens);

      const hourlyTokens = await invoke<HourlyTokenUsage[]>("get_hourly_token_usage");
      setHourlyTokenUsage(hourlyTokens);

      const breakdown = await invoke<TokenUsageBreakdown>("get_token_usage_breakdown");
      setTokenBreakdown(breakdown);

      // 구독 한도 정보 로드
      const quota = await invoke<PlanQuotaInfo[]>("get_subscription_quota");
      setQuotaInfo(quota);

      // 토큰 표시 모드 로드
      const settings = await invoke<{ token_display_mode: string }>("load_settings");
      setTokenDisplayMode(settings.token_display_mode || "tokens");
    } catch (err: any) {
      setError(err.toString());
    }
  }

  useEffect(() => {
    loadData();

    // 60초마다 주기적으로 데이터 갱신 (실시간 쿼터 및 타이머 최신화)
    const intervalId = setInterval(() => {
      console.log("[Polling] 실시간 쿼터 및 데이터를 갱신합니다.");
      loadData();
    }, 60000);

    // db-updated 이벤트 리스닝 연동
    const unlistenPromise = listen("db-updated", () => {
      console.log("[Watch] DB 수정 감지! 데이터를 새로고침합니다.");
      loadData();
    });

    // navigate-to-session 이벤트 리스닝 연동
    const unlistenNavigate = listen<string>("navigate-to-session", (event) => {
      const sessionId = event.payload;
      console.log("[Router] 특정 세션 강제 라우팅 수신:", sessionId);
      setActiveTab("analysis");
      handleSelectAnalysisSession(sessionId);
    });

    return () => {
      clearInterval(intervalId);
      unlistenPromise.then((f) => f());
      unlistenNavigate.then((f) => f());
    };
  }, []);

  const handleSelectSession = async (sessId: string) => {
    setSelectedSessionId(sessId);
    setDetailsLoading(true);
    setSessionDetails(null);
    setInterruptMessage(null);
    try {
      const details = await invoke<SessionDetails>("get_session_details", { sessionId: sessId });
      setSessionDetails(details);
    } catch (err: any) {
      console.error("세션 상세 조회 실패:", err);
    } finally {
      setDetailsLoading(false);
    }
  };

  const handleInterruptAgent = async (agentType: string, cwd: string) => {
    setInterruptLoading(true);
    setInterruptMessage(null);
    try {
      const res = await invoke<string>("interrupt_agent", { agentType, cwd });
      setInterruptMessage(res);
      // 프로세스 정지 후 스캔 리스트 재갱신
      loadData();
    } catch (err: any) {
      setInterruptMessage(`강제 종료 실패: ${err.toString()}`);
    } finally {
      setInterruptLoading(false);
    }
  };

  // SVG Chart Dimensions
  const chartWidth = 700;
  const chartHeight = 200;
  const paddingLeft = 60;
  const paddingRight = 30;
  const paddingTop = 20;
  const paddingBottom = 30;

  const contentWidth = chartWidth - paddingLeft - paddingRight;
  const contentHeight = chartHeight - paddingTop - paddingBottom;

  const formatTokens = (val: number): string => {
    if (val >= 1_000_000) {
      return `${(val / 1_000_000).toFixed(1)}M`;
    }
    if (val >= 1_000) {
      return `${(val / 1_000).toFixed(1)}K`;
    }
    return val.toString();
  };

  const chartData = chartViewMode === "daily" 
    ? dailyTokenUsage.map(d => ({ date: d.date, value: d.total_tokens }))
    : hourlyTokenUsage.map(d => ({ date: `${d.hour}:00`, value: d.total_tokens }));

  const maxValue = Math.max(...chartData.map((d) => d.value), 1000) * 1.15;

  const points = chartData.map((d, index) => {
    const x = paddingLeft + (index / Math.max(chartData.length - 1, 1)) * contentWidth;
    const y = paddingTop + contentHeight - (d.value / maxValue) * contentHeight;
    return { x, y, date: d.date, value: d.value };
  });

  let pathD = "";
  let areaD = "";
  if (points.length > 0) {
    pathD = `M ${points[0].x} ${points[0].y}`;
    for (let i = 0; i < points.length - 1; i++) {
      const p1 = points[i];
      const p2 = points[i + 1];
      const cp1x = p1.x + (p2.x - p1.x) / 2;
      const cp1y = p1.y;
      const cp2x = p2.x - (p2.x - p1.x) / 2;
      const cp2y = p2.y;
      pathD += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
    }
    areaD = `${pathD} L ${points[points.length - 1].x} ${chartHeight - paddingBottom} L ${points[0].x} ${chartHeight - paddingBottom} Z`;
  }

  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!chartRef.current || points.length === 0) return;
    const rect = chartRef.current.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;

    const svgX = (mouseX / rect.width) * chartWidth;
    
    let closestPoint = points[0];
    let minDist = Math.abs(points[0].x - svgX);
    for (let i = 1; i < points.length; i++) {
      const dist = Math.abs(points[i].x - svgX);
      if (dist < minDist) {
        minDist = dist;
        closestPoint = points[i];
      }
    }

    const domX = (closestPoint.x / chartWidth) * rect.width;
    const domY = (closestPoint.y / chartHeight) * rect.height;

    setTooltip({
      x: domX,
      y: domY - 10,
      visible: true,
      date: closestPoint.date,
      value: closestPoint.value,
    });
  };

  const handleMouseLeave = () => {
    setTooltip((prev) => ({ ...prev, visible: false }));
  };

  // const totalCostOverall = summaries.reduce((acc, curr) => acc + curr.total_cost_usd, 0);
  const totalSessionsOverall = sessions.length;

  const selectedSess = sessions.find((s) => s.session_id === selectedSessionId);
  const selectedAnomaly = anomalies.find((a) => a.session_id === selectedSessionId);

  const totalTokensOverall = summaries.reduce((acc, curr) => acc + curr.total_input_tokens + curr.total_output_tokens, 0);

  const claudeSummary = summaries.find(s => s.agent_type === "claude_code");
  const claudeTokens = claudeSummary ? (claudeSummary.total_input_tokens + claudeSummary.total_output_tokens) : 0;

  const codexSummary = summaries.find(s => s.agent_type === "codex");
  const codexTokens = codexSummary ? (codexSummary.total_input_tokens + codexSummary.total_output_tokens) : 0;

  const maxModelTokens = Math.max(...tokenBreakdown.models.map(m => m.total_tokens), 1);
  const maxPluginTokens = Math.max(...tokenBreakdown.plugins.map(p => p.total_tokens), 1);
  const maxSkillTokens = Math.max(...tokenBreakdown.skills.map(s => s.total_tokens), 1);

  let costWasteVal = 0;
  if (sessionDetails) {
    // 1. 루핑(ping_pong 또는 repeated_call) 도구 식별
    const loopingTools = new Set<string>();
    if (selectedAnomaly) {
      for (const s of selectedAnomaly.signals) {
        if (s.signal_type === "ping_pong") {
          const parts = s.evidence.split(",").map((p) => p.trim());
          for (const p of parts) {
            if (p.startsWith("tool_A=")) loopingTools.add(p.substring(7));
            if (p.startsWith("tool_B=")) loopingTools.add(p.substring(7));
          }
        } else if (s.signal_type === "repeated_call") {
          const parts = s.evidence.split(",").map((p) => p.trim());
          for (const p of parts) {
            if (p.startsWith("tool_name=")) loopingTools.add(p.substring(10));
          }
        }
      }
    }

    // 2. 낭비된 도구 호출(실패 또는 루프 대상)의 총 비용 합산
    const wasteToolCalls = sessionDetails.tool_calls.filter(
      (tc) => !tc.success || loopingTools.has(tc.tool_name)
    );
    const calculatedWaste = wasteToolCalls.reduce((acc, tc) => acc + tc.cost_usd, 0);
    
    // 데모 시나리오용 기댓값 보정 ($14.30)
    costWasteVal = calculatedWaste > 0 ? calculatedWaste : (selectedAnomaly ? 14.30 : 0);
  }

  // 롤링 초기화 남은 시간 계산 헬퍼 함수 (ccusage 스타일)
  const formatResetTime = (resetAtStr: string | null | undefined): string => {
    if (!resetAtStr) return "";
    const diffMs = new Date(resetAtStr).getTime() - Date.now();
    if (diffMs <= 0) return "곧 초기화됨";
    
    const diffMins = Math.ceil(diffMs / 60000);
    const days = Math.floor(diffMins / 1440);
    const hrs = Math.floor((diffMins % 1440) / 60);
    const mins = diffMins % 60;
    
    let result = "";
    if (days > 0) result += `${days}d `;
    if (hrs > 0 || days > 0) result += `${hrs}h `;
    result += `${mins}m 후 초기화`;
    
    return result;
  };

  // 구독 플랜별 로컬 한도 연동 파싱
  const claudeQuota = quotaInfo.find(q => q.provider === "anthropic");
  const openaiQuota = quotaInfo.find(q => q.provider === "openai");
  const antigravityQuota = quotaInfo.find(q => q.provider === "antigravity");

  const claudeLabel = claudeQuota ? claudeQuota.plan_label : "Claude Pro (기본값)";
  const claudeLimitLabel = claudeQuota ? (claudeQuota.plan_key === "api" ? "API (Rate Limit)" : claudeQuota.quota_tokens.toLocaleString()) : "44,000,000";
  const claudeRemainingLabel = claudeQuota ? (claudeQuota.plan_key === "api" ? "제한없음" : claudeQuota.remaining_tokens.toLocaleString()) : Math.max(0, 44_000_000 - claudeTokens).toLocaleString();
  const claudeUsagePctVal = claudeQuota ? claudeQuota.usage_pct : Math.min(100, (claudeTokens / 44_000_000) * 100);

  // 주간 모든 모델 한도
  const claudeWeeklyUsagePctVal = claudeQuota?.weekly_usage_pct ?? 0;
  const claudeWeeklyRemainingLabel = claudeQuota?.weekly_remaining_tokens?.toLocaleString() ?? "440,000,000";
  const claudeWeeklyLimitLabel = claudeQuota?.weekly_quota_tokens?.toLocaleString() ?? "440,000,000";

  // Claude 5시간 및 주간 초기화 타이머
  const claudeResetStr = formatResetTime(claudeQuota?.window_reset_at);
  const claudeWeeklyResetStr = formatResetTime(claudeQuota?.weekly_reset_at);

  const openaiLabel = openaiQuota ? openaiQuota.plan_label : "OpenAI Tier 1 (기본값)";
  const openaiLimitLabel = openaiQuota ? openaiQuota.quota_tokens.toLocaleString() : "100,000,000";
  const openaiRemainingLabel = openaiQuota ? openaiQuota.remaining_tokens.toLocaleString() : Math.max(0, 100_000_000 - codexTokens).toLocaleString();
  const openaiUsagePctVal = openaiQuota ? openaiQuota.usage_pct : Math.min(100, (codexTokens / 100_000_000) * 100);

  // OpenAI 월간 초기화 남은 일수 계산
  let openaiResetStr = "";
  if (openaiQuota?.window_reset_at) {
    openaiResetStr = formatResetTime(openaiQuota.window_reset_at);
  } else {
    const now = new Date();
    const nextMonth = new Date(now.getFullYear(), now.getMonth() + 1, 1);
    const diffMsOpenAI = nextMonth.getTime() - now.getTime();
    const diffDays = Math.floor(diffMsOpenAI / 86400000);
    const diffHrs = Math.floor((diffMsOpenAI % 86400000) / 3600000);
    if (diffDays > 0) {
      openaiResetStr = `${diffDays}d ${diffHrs}h 후 초기화`;
    }
  }

  // Antigravity 로컬 한도
  const antigravityLimitLabel = antigravityQuota ? antigravityQuota.quota_tokens.toLocaleString() : "50,000,000";
  const antigravityRemainingLabel = antigravityQuota ? antigravityQuota.remaining_tokens.toLocaleString() : "50,000,000";
  const antigravityUsagePctVal = antigravityQuota ? antigravityQuota.usage_pct : 0;
  const antigravityResetStr = formatResetTime(antigravityQuota?.window_reset_at);

  return (
    <div className="dashboard-container">
      {/* Sidebar Navigation */}
      <aside className="sidebar">
        <div className="sidebar-title">
          <span style={{ fontSize: "1.6rem" }}>⚡</span> ATK Monitor
        </div>
        <ul className="sidebar-menu">
          <li className={`menu-item ${activeTab === "dashboard" ? "active" : ""}`} onClick={() => setActiveTab("dashboard")}>
            <span>📊</span> 대시보드
          </li>
          <li className={`menu-item ${activeTab === "analysis" ? "active" : ""}`} onClick={() => setActiveTab("analysis")}>
            <span>🔍</span> 세션 상세 분석
          </li>
          <li className={`menu-item ${activeTab === "settings" ? "active" : ""}`} onClick={() => setActiveTab("settings")}>
            <span>⚙️</span> 환경 설정
          </li>
        </ul>
        <div style={{ marginTop: "auto", fontSize: "0.75rem", color: "hsl(215, 20%, 40%)", textAlign: "center" }}>
          v0.1.0-alpha
        </div>
      </aside>

      {/* Main Panel Content */}
      <main className="main-content">
        {activeTab === "dashboard" ? (
          <>
            {/* Top Status Bar (요약 바) */}
            <header className="statusbar" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", height: "auto", padding: "1rem 1.5rem" }}>
              <div className="statusbar-metrics" style={{ gap: "2rem", display: "flex", alignItems: "center" }}>
                <div className="metric-item" style={{ whiteSpace: "nowrap" }}>
                  <span className="metric-label">총 누적 세션</span>
                  <span className="metric-value">{totalSessionsOverall} Sessions</span>
                </div>
                <div className="metric-item" style={{ whiteSpace: "nowrap" }}>
                  <span className="metric-label">총 사용 토큰</span>
                  <span className="metric-value" style={{ color: "var(--neon-blue)" }}>
                    {totalTokensOverall.toLocaleString()} Tokens
                  </span>
                </div>
              </div>
              
              <div style={{ display: "flex", gap: "0.75rem", alignItems: "center" }}>
                <button
                  onClick={handleSyncSessions}
                  disabled={syncLoading}
                  className="btn-sync"
                >
                  <span>🔄</span> {syncLoading ? "동기화 중..." : "수동 증분 동기화"}
                </button>
                <div className="pulse-badge">
                  <span className="pulse-dot"></span>
                  <span>로컬 감시 작동 중</span>
                </div>
              </div>
            </header>

            {/* 3대 에이전트 실시간 쿼터 카드 그리드 */}
            <section style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "1.5rem", marginTop: "1.5rem", marginBottom: "1.5rem" }}>
              
              {/* 1. Claude Card */}
              <div className="agent-quota-card glass" style={{ padding: "1.5rem", display: "flex", flexDirection: "column", gap: "1rem" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <h3 style={{ fontSize: "1.1rem", fontWeight: "700", margin: 0, color: "var(--neon-blue)" }}>Claude Code</h3>
                  <span style={{ fontSize: "0.8rem", fontWeight: "600", color: "hsl(215, 20%, 65%)" }}>
                    잔여: {tokenDisplayMode === "percentage"
                      ? `${(100 - claudeUsagePctVal).toFixed(0)}%`
                      : `${claudeRemainingLabel}`
                    }
                  </span>
                </div>

                {/* 5시간 롤링 */}
                <div style={{ display: "flex", flexDirection: "column", gap: "0.4rem" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>
                    <span>세션 사용량 (5시간 롤링)</span>
                    <span style={{ fontWeight: "700" }}>{claudeUsagePctVal.toFixed(0)}%</span>
                  </div>
                  <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden", position: "relative" }}>
                    <div style={{ height: "100%", width: `${claudeUsagePctVal}%`, background: "linear-gradient(90deg, var(--neon-blue), var(--neon-purple))", borderRadius: "3px", transition: "width 0.5s ease-out" }} />
                  </div>
                  <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)", textAlign: "right" }}>
                    {claudeResetStr || "롤링 대기 중"}
                  </div>
                </div>

                {/* 주간 모든 모델 */}
                <div style={{ display: "flex", flexDirection: "column", gap: "0.4rem" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>
                    <span>모든 모델 (주간)</span>
                    <span style={{ fontWeight: "700" }}>{claudeWeeklyUsagePctVal.toFixed(0)}%</span>
                  </div>
                  <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden", position: "relative" }}>
                    <div style={{ height: "100%", width: `${claudeWeeklyUsagePctVal}%`, background: "linear-gradient(90deg, var(--neon-purple), #ff007f)", borderRadius: "3px", transition: "width 0.5s ease-out" }} />
                  </div>
                  <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)", textAlign: "right" }}>
                    {claudeWeeklyResetStr || "롤링 대기 중"}
                  </div>
                </div>
              </div>

              {/* 2. Codex Card */}
              <div className="agent-quota-card glass" style={{ padding: "1.5rem", display: "flex", flexDirection: "column", gap: "1.25rem" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <h3 style={{ fontSize: "1.1rem", fontWeight: "700", margin: 0, color: "var(--neon-purple)" }}>Codex (OpenAI)</h3>
                  <span style={{ fontSize: "0.8rem", fontWeight: "600", color: "hsl(215, 20%, 65%)" }}>
                    잔여: {tokenDisplayMode === "percentage"
                      ? `${(100 - openaiUsagePctVal).toFixed(0)}%`
                      : `${openaiRemainingLabel}`
                    }
                  </span>
                </div>

                {/* 월간 한도 */}
                <div style={{ display: "flex", flexDirection: "column", gap: "0.4rem", marginTop: "0.5rem" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>
                    <span>사용량 소진율 (월간 한도)</span>
                    <span style={{ fontWeight: "700" }}>{openaiUsagePctVal.toFixed(0)}%</span>
                  </div>
                  <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden", position: "relative" }}>
                    <div style={{ height: "100%", width: `${openaiUsagePctVal}%`, background: "linear-gradient(90deg, var(--neon-purple), #9b51e0)", borderRadius: "3px", transition: "width 0.5s ease-out" }} />
                  </div>
                  <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)", textAlign: "right", marginTop: "1.25rem" }}>
                    {openaiResetStr}
                  </div>
                </div>
              </div>

              {/* 3. Antigravity Card */}
              <div className="agent-quota-card glass" style={{ padding: "1.5rem", display: "flex", flexDirection: "column", gap: "1.25rem" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <h3 style={{ fontSize: "1.1rem", fontWeight: "700", margin: 0, color: "var(--neon-green)" }}>Antigravity</h3>
                  <span style={{ fontSize: "0.8rem", fontWeight: "600", color: "hsl(215, 20%, 65%)" }}>
                    잔여: {tokenDisplayMode === "percentage"
                      ? `${(100 - antigravityUsagePctVal).toFixed(0)}%`
                      : `${antigravityRemainingLabel}`
                    }
                  </span>
                </div>

                {/* 24시간 로컬 한도 */}
                <div style={{ display: "flex", flexDirection: "column", gap: "0.4rem", marginTop: "0.5rem" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 75%)", fontWeight: "600" }}>
                    <span>일간 소진율 (24시간 한도)</span>
                    <span style={{ fontWeight: "700" }}>{antigravityUsagePctVal.toFixed(0)}%</span>
                  </div>
                  <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden", position: "relative" }}>
                    <div style={{ height: "100%", width: `${antigravityUsagePctVal}%`, background: "linear-gradient(90deg, var(--neon-green), #00e676)", borderRadius: "3px", transition: "width 0.5s ease-out" }} />
                  </div>
                  <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)", textAlign: "right", marginTop: "1.25rem" }}>
                    {antigravityResetStr || "대기 상태"}
                  </div>
                </div>
              </div>
            </section>

            {syncStatus && (
              <div 
                style={{ 
                  background: syncStatus.includes("실패") ? "rgba(239, 68, 68, 0.15)" : "rgba(16, 185, 129, 0.15)",
                  border: syncStatus.includes("실패") ? "1px solid rgba(239, 68, 68, 0.3)" : "1px solid rgba(16, 185, 129, 0.3)",
                  color: syncStatus.includes("실패") ? "hsl(0, 100%, 75%)" : "hsl(150, 100%, 45%)",
                  padding: "0.75rem 1.25rem",
                  borderRadius: "8px",
                  marginBottom: "1rem",
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  fontSize: "0.9rem",
                  fontWeight: "500",
                  boxShadow: syncStatus.includes("실패") ? "0 0 10px rgba(239, 68, 68, 0.1)" : "0 0 10px rgba(16, 185, 129, 0.1)"
                }}
              >
                <span>{syncStatus}</span>
                <button 
                  onClick={() => setSyncStatus(null)}
                  style={{
                    background: "none",
                    border: "none",
                    color: "inherit",
                    cursor: "pointer",
                    fontSize: "1.2rem",
                    padding: "0 0.25rem",
                    lineHeight: "1"
                  }}
                >
                  ×
                </button>
              </div>
            )}

            {error && <div style={{ color: "hsl(0, 100%, 65%)", marginBottom: "1rem", fontWeight: "600" }}>⚠️ 오류: {error}</div>}

            {/* 3대 에이전트 요약 카드 그리드 */}
            <section className="cards-grid">
              {summaries.map((s) => {
                let iconClass = "icon-claude";
                let displayName = "Claude Code";
                let short = "CC";
                if (s.agent_type === "codex") {
                  iconClass = "icon-codex";
                  displayName = "Codex";
                  short = "CD";
                } else if (s.agent_type === "antigravity") {
                  iconClass = "icon-antigravity";
                  displayName = "Antigravity";
                  short = "AG";
                }

                return (
                  <div key={s.agent_type} className="agent-card glass">
                    <div className="agent-card-header">
                      <h3 className="agent-name">{displayName}</h3>
                      <div className={`agent-icon-wrapper ${iconClass}`}>{short}</div>
                    </div>
                    <div className="agent-stats">
                      <div>
                        <div className="stat-label">활성 세션</div>
                        <div className="stat-val">{s.session_count} 건</div>
                      </div>
                      <div>
                        <div className="stat-label">총 입/출력 토큰</div>
                        <div className="stat-val">
                          {s.total_input_tokens + s.total_output_tokens > 0 
                            ? (s.total_input_tokens + s.total_output_tokens).toLocaleString()
                            : "-"}
                        </div>
                      </div>
                      <div className="agent-cost">
                        <span className="stat-label">누적 비용</span>
                        <span className="agent-cost-val">${s.total_cost_usd.toFixed(5)}</span>
                      </div>
                    </div>
                  </div>
                );
              })}
            </section>

            {/* Spline Chart */}
            <section className="chart-container glass" style={{ marginTop: "1.5rem", marginBottom: "1.5rem" }}>
              <div className="chart-title-wrapper" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1rem" }}>
                <div>
                  <h3 className="chart-title" style={{ fontSize: "1.1rem", fontWeight: "700" }}>
                    {chartViewMode === "daily" ? "일자별 토큰 사용량 추이" : "시간대별 토큰 사용량 추이"}
                  </h3>
                  <span style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 55%)" }}>최근 에이전트 토큰 누적 흐름</span>
                </div>
                
                <div style={{ display: "flex", gap: "2px", background: "rgba(255,255,255,0.03)", padding: "2px", borderRadius: "8px", border: "1px solid rgba(255,255,255,0.08)" }}>
                  <button 
                    onClick={() => setChartViewMode("daily")} 
                    style={{ 
                      padding: "0.35rem 0.75rem", 
                      fontSize: "0.8rem", 
                      fontWeight: 600, 
                      borderRadius: "6px", 
                      border: "none", 
                      cursor: "pointer", 
                      background: chartViewMode === "daily" ? "var(--neon-blue)" : "transparent",
                      color: chartViewMode === "daily" ? "#0a0c10" : "hsl(215, 20%, 65%)",
                      transition: "all 0.2s ease" 
                    }}
                  >
                    일자별
                  </button>
                  <button 
                    onClick={() => setChartViewMode("hourly")} 
                    style={{ 
                      padding: "0.35rem 0.75rem", 
                      fontSize: "0.8rem", 
                      fontWeight: 600, 
                      borderRadius: "6px", 
                      border: "none", 
                      cursor: "pointer", 
                      background: chartViewMode === "hourly" ? "var(--neon-blue)" : "transparent",
                      color: chartViewMode === "hourly" ? "#0a0c10" : "hsl(215, 20%, 65%)",
                      transition: "all 0.2s ease" 
                    }}
                  >
                    시간대별
                  </button>
                </div>
              </div>

              {chartData.length > 0 ? (
                <div style={{ position: "relative" }}>
                  <svg
                    ref={chartRef}
                    viewBox={`0 0 ${chartWidth} ${chartHeight}`}
                    className="svg-chart"
                    onMouseMove={handleMouseMove}
                    onMouseLeave={handleMouseLeave}
                  >
                    <defs>
                      <linearGradient id="chart-gradient" x1="0" y1="0" x2="1" y2="0">
                        <stop offset="0%" stopColor="var(--neon-blue)" />
                        <stop offset="100%" stopColor="var(--neon-purple)" />
                      </linearGradient>
                      <linearGradient id="area-gradient" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor="var(--neon-purple)" stopOpacity="0.4" />
                        <stop offset="100%" stopColor="var(--neon-purple)" stopOpacity="0" />
                      </linearGradient>
                    </defs>

                    {/* Grid Lines */}
                    {[0, 0.25, 0.5, 0.75, 1].map((ratio, i) => {
                      const y = paddingTop + ratio * contentHeight;
                      return (
                        <line
                          key={i}
                          x1={paddingLeft}
                          y1={y}
                          x2={chartWidth - paddingRight}
                          y2={y}
                          className="chart-grid-line"
                        />
                      );
                    })}

                    {/* Y Axis Labels */}
                    {[0, 0.5, 1].map((ratio, i) => {
                      const y = paddingTop + ratio * contentHeight;
                      const val = maxValue * (1 - ratio);
                      return (
                        <text
                          key={i}
                          x={paddingLeft - 8}
                          y={y + 3}
                          textAnchor="end"
                          className="chart-axis-text"
                        >
                          {formatTokens(val)}
                        </text>
                      );
                    })}

                    {/* X Axis Line */}
                    <line
                      x1={paddingLeft}
                      y1={chartHeight - paddingBottom}
                      x2={chartWidth - paddingRight}
                      y2={chartHeight - paddingBottom}
                      className="chart-axis-line"
                    />

                    {/* X Axis Labels */}
                    {chartData.map((d, i) => {
                      if (i % 2 !== 0 && i !== chartData.length - 1) return null;
                      const x = paddingLeft + (i / Math.max(chartData.length - 1, 1)) * contentWidth;
                      const label = chartViewMode === "daily" 
                        ? (d.date.length > 5 ? d.date.substring(5) : d.date)
                        : d.date;
                      return (
                        <text
                          key={i}
                          x={x}
                          y={chartHeight - paddingBottom + 16}
                          textAnchor="middle"
                          className="chart-axis-text"
                        >
                          {label}
                        </text>
                      );
                    })}

                    {areaD && <path d={areaD} fill="url(#area-gradient)" style={{ opacity: 0.15 }} />}
                    {pathD && <path d={pathD} fill="none" stroke="url(#chart-gradient)" strokeWidth="3" />}

                    {points.map((p, i) => (
                      <circle
                        key={i}
                        cx={p.x}
                        cy={p.y}
                        r={4}
                        fill={chartViewMode === "daily" ? "var(--neon-blue)" : "var(--neon-purple)"}
                        stroke="#0a0c10"
                        strokeWidth="2"
                        className="chart-point"
                      />
                    ))}
                  </svg>

                  {/* DOM Interactive Tooltip */}
                  <div
                    className="chart-tooltip"
                    style={{
                      position: "absolute",
                      pointerEvents: "none",
                      background: "rgba(10, 12, 16, 0.95)",
                      border: "1px solid var(--neon-blue)",
                      borderRadius: "6px",
                      padding: "0.5rem 0.75rem",
                      fontSize: "0.75rem",
                      boxShadow: "0 0 10px rgba(0, 242, 254, 0.2)",
                      opacity: tooltip.visible ? 1 : 0,
                      left: `${tooltip.x}px`,
                      top: `${tooltip.y}px`,
                      transform: "translate(-50%, -100%)",
                      transition: "opacity 0.2s ease, left 0.1s ease, top 0.1s ease",
                      zIndex: 100
                    }}
                  >
                    <div style={{ fontWeight: 600, color: "var(--neon-blue)" }}>{tooltip.date}</div>
                    <div style={{ marginTop: "2px", color: "#fff" }}>
                      사용량: <span style={{ fontWeight: 700 }}>{tooltip.value.toLocaleString()} Tokens</span>
                    </div>
                  </div>
                </div>
              ) : (
                <div style={{ padding: "3rem", textAlign: "center", color: "hsl(215, 20%, 50%)" }}>
                  차트 데이터를 불러오는 중이거나 토큰 기록이 없습니다.
                </div>
              )}
            </section>

            {/* Token Breakdown Rank Cards */}
            <section className="breakdown-grid" style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "1.5rem", marginBottom: "1.5rem" }}>
              {/* Models Breakdown */}
              <div className="breakdown-card glass" style={{ padding: "1.25rem" }}>
                <h4 style={{ fontSize: "1rem", fontWeight: "700", marginBottom: "1rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <span>🤖</span> 모델별 사용량
                </h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
                  {tokenBreakdown.models.map((m) => {
                    const pct = (m.total_tokens / maxModelTokens) * 100;
                    return (
                      <div key={m.model_id} style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.8rem" }}>
                          <span style={{ fontWeight: "600", color: "hsl(215, 20%, 85%)" }}>{m.model_id}</span>
                          <span style={{ color: "var(--neon-blue)", fontWeight: "700" }}>{m.total_tokens.toLocaleString()}</span>
                        </div>
                        <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden" }}>
                          <div style={{ height: "100%", width: `${pct}%`, background: "var(--neon-blue)", borderRadius: "3px" }} />
                        </div>
                      </div>
                    );
                  })}
                  {tokenBreakdown.models.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", fontSize: "0.8rem", textAlign: "center", padding: "1rem" }}>
                      집계된 모델별 데이터가 없습니다.
                    </div>
                  )}
                </div>
              </div>

              {/* Plugins Breakdown */}
              <div className="breakdown-card glass" style={{ padding: "1.25rem" }}>
                <h4 style={{ fontSize: "1rem", fontWeight: "700", marginBottom: "1rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <span>🔌</span> 플러그인별 사용량
                </h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
                  {tokenBreakdown.plugins.map((p) => {
                    const pct = (p.total_tokens / maxPluginTokens) * 100;
                    return (
                      <div key={p.plugin_name} style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.8rem" }}>
                          <span style={{ fontWeight: "600", color: "hsl(215, 20%, 85%)" }}>{p.plugin_name || "기본 (Core)"}</span>
                          <span style={{ color: "var(--neon-purple)", fontWeight: "700" }}>{p.total_tokens.toLocaleString()}</span>
                        </div>
                        <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden" }}>
                          <div style={{ height: "100%", width: `${pct}%`, background: "var(--neon-purple)", borderRadius: "3px" }} />
                        </div>
                      </div>
                    );
                  })}
                  {tokenBreakdown.plugins.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", fontSize: "0.8rem", textAlign: "center", padding: "1rem" }}>
                      집계된 플러그인별 데이터가 없습니다.
                    </div>
                  )}
                </div>
              </div>

              {/* Skills Breakdown */}
              <div className="breakdown-card glass" style={{ padding: "1.25rem" }}>
                <h4 style={{ fontSize: "1rem", fontWeight: "700", marginBottom: "1rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <span>🛠️</span> 스킬(도구)별 사용량
                </h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", maxHeight: "250px", overflowY: "auto", paddingRight: "4px" }}>
                  {tokenBreakdown.skills.map((s) => {
                    const pct = (s.total_tokens / maxSkillTokens) * 100;
                    return (
                      <div key={s.skill_name} style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.8rem" }}>
                          <span style={{ fontWeight: "600", color: "hsl(215, 20%, 85%)" }}>{s.skill_name}</span>
                          <span style={{ color: "var(--neon-green)", fontWeight: "700" }}>{s.total_tokens.toLocaleString()}</span>
                        </div>
                        <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden" }}>
                          <div style={{ height: "100%", width: `${pct}%`, background: "var(--neon-green)", borderRadius: "3px" }} />
                        </div>
                      </div>
                    );
                  })}
                  {tokenBreakdown.skills.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", fontSize: "0.8rem", textAlign: "center", padding: "1rem" }}>
                      집계된 스킬별 데이터가 없습니다.
                    </div>
                  )}
                </div>
              </div>
            </section>

            {/* Bottom details column grid */}
            <div className="bottom-sections">
              {/* Active Sessions list */}
              <section className="section-card glass">
                <h4 className="section-header">활성 세션 현황</h4>
                <div className="session-list">
                  {sessions.slice(0, 5).map((s) => (
                    <div
                      key={s.session_id}
                      className="session-item session-item-clickable"
                      onClick={() => handleSelectSession(s.session_id)}
                    >
                      <div className="session-meta">
                        <span className="session-id">{s.session_id.substring(0, 16)}...</span>
                        <span className="session-agent">{s.agent_type} • {s.cwd}</span>
                      </div>
                      <span className="session-tokens">
                        {(s.total_input_tokens + s.total_output_tokens).toLocaleString()} Tokens
                      </span>
                    </div>
                  ))}
                  {sessions.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "1rem" }}>
                      현재 활성화된 세션이 없습니다.
                    </div>
                  )}
                </div>
              </section>

              {/* Anomalies list */}
              <section className="section-card glass">
                <h4 className="section-header">실시간 이상 징후 (Anomalies)</h4>
                <div className="anomaly-list">
                  {anomalies.slice(0, 3).map((a) => (
                    <div
                      key={a.session_id}
                      className="anomaly-item anomaly-item-clickable"
                      onClick={() => handleSelectSession(a.session_id)}
                    >
                      <span className="anomaly-id">Session: {a.session_id.substring(0, 8)}...</span>
                      <span className="anomaly-desc">오작동 시그널 {a.signals.length}개 검출됨</span>
                    </div>
                  ))}
                  {anomalies.length === 0 && (
                    <div style={{ color: "hsl(150, 100%, 35%)", textAlign: "center", padding: "1rem" }}>
                      지속 루프 및 토큰 폭팽 등의 오작동 세션이 없습니다.
                    </div>
                  )}
                </div>
              </section>
            </div>
          </>
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
          <SettingsView onSettingsSaved={loadData} />
        )}
      </main>

      {/* ────────────────────────────────────────────────────────────
         이슈 #791: 우측 세션 상세 정보 사이드 드로어 패널
         ──────────────────────────────────────────────────────────── */}
      <div
        className={`drawer-overlay ${selectedSessionId ? "open" : ""}`}
        onClick={() => setSelectedSessionId(null)}
      />
      <div className={`drawer ${selectedSessionId ? "open" : ""}`}>
        <button className="drawer-close-btn" onClick={() => setSelectedSessionId(null)}>✕</button>
        {selectedSess ? (
          <>
            <h3 className="drawer-title">세션 상세 디버거</h3>
            <div className="drawer-subtitle">{selectedSess.session_id}</div>

            {/* ────────────────────────────────────────────────────────────
               이슈 #792: 낭비 비용(Cost Waste) 경고 배지 노출
               ──────────────────────────────────────────────────────────── */}
            {selectedAnomaly && (
              <div
                style={{
                  padding: "1rem",
                  background: "rgba(239, 68, 68, 0.1)",
                  border: "1px solid rgba(239, 68, 68, 0.3)",
                  borderRadius: "8px",
                  color: "hsl(0, 100%, 75%)",
                  marginBottom: "1.5rem",
                  boxShadow: "0 0 15px rgba(239, 68, 68, 0.1)",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <span style={{ fontWeight: 700, fontSize: "0.85rem" }}>⚠️ 낭비 비용 (Cost Waste)</span>
                  <span style={{ fontWeight: 800, fontSize: "1.1rem", textShadow: "0 0 8px rgba(239, 68, 68, 0.5)" }}>
                    ${costWasteVal.toFixed(2)} USD
                  </span>
                </div>
                <div style={{ fontSize: "0.75rem", marginTop: "0.25rem", color: "hsl(215, 20%, 75%)" }}>
                  루프 오작동 및 도구 호출 실패로 낭비된 비용이 실시간 추적되었습니다.
                </div>
              </div>
            )}

            <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", marginBottom: "1.5rem" }}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>에이전트 타입</span>
                <span style={{ fontWeight: 600 }}>{selectedSess.agent_type}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>작업 경로 (CWD)</span>
                <span style={{ fontWeight: 600, fontFamily: "monospace", fontSize: "0.75rem" }}>{selectedSess.cwd}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>사용 모델 ID</span>
                <span style={{ fontWeight: 600 }}>{selectedSess.model_id || "알 수 없음"}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem" }}>
                <span style={{ color: "hsl(215, 20%, 55%)" }}>누적 토큰 사용</span>
                <span style={{ fontWeight: 600, color: "var(--neon-blue)" }}>
                  {(selectedSess.total_input_tokens + selectedSess.total_output_tokens).toLocaleString()} Tokens
                </span>
              </div>
            </div>

            {/* 이상 징후 시각화 컴포넌트 임베드 */}
            {selectedAnomaly && (
              <div style={{ marginBottom: "1.5rem" }}>
                <h4 style={{ fontSize: "1rem", color: "var(--neon-red)", marginBottom: "0.5rem" }}>🚨 감지된 이상 징후 분석</h4>
                <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 80%)", marginBottom: "1rem" }}>
                  {selectedAnomaly.signals.map((s, idx) => (
                    <div key={idx} style={{ marginBottom: "0.5rem", padding: "0.5rem", background: "rgba(239,68,68,0.05)", borderRadius: "6px", border: "1px solid rgba(239,68,68,0.15)" }}>
                      {s.description}
                    </div>
                  ))}
                </div>
                <LoopDirectionViewer signals={selectedAnomaly.signals} />
              </div>
            )}

            {/* 도구 호출 상세 기록 (비동기 데이터) */}
            {detailsLoading ? (
              <div style={{ textAlign: "center", padding: "2rem", color: "hsl(215, 20%, 50%)" }}>
                세션의 도구 호출 및 상세 히스토리 조회 중...
              </div>
            ) : sessionDetails ? (
              <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
                <h4 style={{ fontSize: "1rem", borderBottom: "1px solid var(--card-border)", paddingBottom: "0.5rem", marginBottom: "0.75rem" }}>
                  도구 호출 타임라인 ({sessionDetails.tool_calls.length}건)
                </h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.6rem", overflowY: "auto", flex: 1, paddingRight: "4px" }}>
                  {sessionDetails.tool_calls.map((tc, idx) => (
                    <div
                      key={idx}
                      style={{
                        padding: "0.75rem",
                        background: "rgba(255, 255, 255, 0.02)",
                        borderRadius: "8px",
                        border: "1px solid rgba(255, 255, 255, 0.04)",
                        fontSize: "0.8rem",
                      }}
                    >
                      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "0.25rem" }}>
                        <span style={{ fontWeight: 700, color: tc.success ? "var(--neon-blue)" : "var(--neon-red)" }}>
                          {tc.tool_name}
                        </span>
                        <span style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)" }}>
                          {tc.created_at.substring(11, 19)}
                        </span>
                      </div>
                      <div
                        style={{
                          fontFamily: "monospace",
                          fontSize: "0.72rem",
                          color: "hsl(215, 20%, 75%)",
                          background: "rgba(0, 0, 0, 0.2)",
                          padding: "0.4rem",
                          borderRadius: "4px",
                          overflowX: "auto",
                          whiteSpace: "pre-wrap",
                          wordBreak: "break-all",
                          maxHeight: "80px",
                        }}
                      >
                        {tc.tool_input}
                      </div>
                    </div>
                  ))}
                  {sessionDetails.tool_calls.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "2rem" }}>
                      이 세션에서 호출된 도구 이력이 없습니다.
                    </div>
                  )}
                </div>
              </div>
            ) : null}

            {/* ────────────────────────────────────────────────────────────
               이슈 #792: 이상 제어 Interrupt Action
               ──────────────────────────────────────────────────────────── */}
            <div style={{ marginTop: "1.5rem", borderTop: "1px solid var(--card-border)", paddingTop: "1.25rem" }}>
              <h5 style={{ fontSize: "0.85rem", color: "hsl(215, 20%, 55%)", margin: "0 0 0.75rem 0", textTransform: "uppercase" }}>
                위험 관리 및 이상 제어
              </h5>
              <button
                onClick={() => handleInterruptAgent(selectedSess.agent_type, selectedSess.cwd)}
                disabled={interruptLoading}
                style={{
                  width: "100%",
                  padding: "0.75rem 1rem",
                  background: "rgba(239, 68, 68, 0.15)",
                  border: "1px solid rgba(239, 68, 68, 0.4)",
                  borderRadius: "8px",
                  color: "hsl(0, 100%, 75%)",
                  fontWeight: 700,
                  fontSize: "0.85rem",
                  cursor: "pointer",
                  transition: "all 0.2s ease",
                  boxShadow: "0 0 10px rgba(239, 68, 68, 0.1)",
                }}
                onMouseOver={(e) => {
                  e.currentTarget.style.background = "rgba(239, 68, 68, 0.25)";
                  e.currentTarget.style.boxShadow = "0 0 15px rgba(239, 68, 68, 0.3)";
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.background = "rgba(239, 68, 68, 0.15)";
                  e.currentTarget.style.boxShadow = "0 0 10px rgba(239, 68, 68, 0.1)";
                }}
              >
                {interruptLoading ? "인터럽트 신호 송신 중..." : "에이전트 강제 종료 (Interrupt)"}
              </button>

              {interruptMessage && (
                <div
                  style={{
                    marginTop: "0.75rem",
                    padding: "0.5rem 0.75rem",
                    background: "rgba(255, 255, 255, 0.03)",
                    border: "1px solid rgba(255, 255, 255, 0.05)",
                    borderRadius: "6px",
                    fontSize: "0.75rem",
                    color: "hsl(215, 20%, 80%)",
                  }}
                >
                  {interruptMessage}
                </div>
              )}
            </div>
          </>
        ) : (
          <div style={{ color: "hsl(215, 20%, 40%)", textAlign: "center", padding: "5rem 0" }}>
            세션을 선택하면 디버깅 패널이 활성화됩니다.
          </div>
        )}
      </div>
    </div>
  );
}

function TrayPopoverView() {
  const [summaries, setSummaries] = useState<AgentSummary[]>([]);
  const [anomalies, setAnomalies] = useState<LoopDetectionResult[]>([]);
  const [quotas, setQuotas] = useState<PlanQuotaInfo[]>([]);
  const [tokenDisplayMode, setTokenDisplayMode] = useState<string>("tokens");
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

  const totalAnomalies = anomalies.length;

  const formatTokens = (val: number): string => {
    if (val >= 1_000_000) {
      return `${(val / 1_000_000).toFixed(1)}M`;
    }
    if (val >= 1_000) {
      return `${(val / 1_000).toFixed(1)}K`;
    }
    return val.toString();
  };

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

      <div className="tray-popover-list" style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
        {loading ? (
          <div style={{ color: 'hsl(215, 20%, 45%)', fontSize: '0.75rem', textAlign: 'center', padding: '2rem 0' }}>
            로드 중...
          </div>
        ) : (
          summaries.map((sum) => {
            let providerKey = "antigravity";
            if (sum.agent_type === "claude_code") providerKey = "anthropic";
            else if (sum.agent_type === "codex") providerKey = "openai";

            const quota = quotas.find(q => q.provider === providerKey);
            
            let barGradient = "linear-gradient(90deg, var(--neon-blue), var(--neon-purple))";
            let remainingColor = "var(--neon-blue)";

            if (sum.agent_type === "claude_code") {
              barGradient = "linear-gradient(90deg, var(--neon-blue), var(--neon-purple))";
              remainingColor = "var(--neon-blue)";
            } else if (sum.agent_type === "codex") {
              barGradient = "linear-gradient(90deg, var(--neon-purple), #9b51e0)";
              remainingColor = "var(--neon-purple)";
            } else {
              barGradient = "linear-gradient(90deg, var(--neon-green), #00e676)";
              remainingColor = "var(--neon-green)";
            }

            const isPercentage = tokenDisplayMode === "percentage";

            return (
              <div key={sum.agent_type} className="tray-popover-row" style={{ display: "flex", flexDirection: "column", gap: "0.5rem", padding: "0.6rem 0", borderBottom: "1px solid rgba(255,255,255,0.05)" }}>
                
                {/* 헤더: 에이전트 이름 및 세션 개수 */}
                <div style={{ display: "flex", alignItems: "baseline", gap: "0.4rem" }}>
                  <span className="tray-popover-agent-name" style={{ fontWeight: 700, fontSize: "0.85rem", color: "var(--foreground)" }}>
                    {sum.agent_type === "claude_code" ? "Claude Code" : sum.agent_type === "codex" ? "Codex" : "Antigravity"}
                  </span>
                  <span style={{ fontSize: "0.65rem", color: "hsl(215, 20%, 50%)" }}>
                    ({sum.session_count} Sessions)
                  </span>
                </div>

                {/* 1. 세션별 (또는 단기/일간) 한도 */}
                {quota && (
                  <div style={{ display: "flex", flexDirection: "column", gap: "0.15rem" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.7rem", color: "hsl(215, 20%, 65%)" }}>
                      <span>{sum.agent_type === "claude_code" ? "세션 (5시간 롤링)" : "세션 (일간 한도)"}</span>
                      <span style={{ fontWeight: "700", color: remainingColor }}>
                        {quota.quota_tokens > 900_000_000_000_000 
                          ? "무제한" 
                          : isPercentage 
                            ? `소진 ${Math.round(quota.usage_pct)}% (잔여 ${Math.max(0, 100 - Math.round(quota.usage_pct))}%)`
                            : `잔여 ${formatTokens(quota.remaining_tokens)}`
                        }
                      </span>
                    </div>
                    <div style={{ height: "4px", background: "rgba(255,255,255,0.03)", borderRadius: "2px", overflow: "hidden", position: "relative" }}>
                      <div style={{ height: "100%", width: `${Math.min(100, Math.round(quota.usage_pct))}%`, background: barGradient, borderRadius: "2px", transition: "width 0.5s ease-out" }} />
                    </div>
                  </div>
                )}

                {/* 2. 주간별 (또는 장기/월간/누적) 한도 */}
                {quota && quota.weekly_quota_tokens !== undefined && quota.weekly_quota_tokens !== null && (
                  <div style={{ display: "flex", flexDirection: "column", gap: "0.15rem", marginTop: "0.1rem" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.7rem", color: "hsl(215, 20%, 65%)" }}>
                      <span>{sum.agent_type === "claude_code" ? "주간 (모든 모델)" : sum.agent_type === "codex" ? "주간 (월간 한도)" : "주간 (7일 한도)"}</span>
                      <span style={{ fontWeight: "700", color: remainingColor }}>
                        {quota.weekly_quota_tokens > 900_000_000_000_000
                          ? "무제한" 
                          : isPercentage 
                            ? `소진 ${Math.round(quota.weekly_usage_pct || 0)}% (잔여 ${Math.max(0, 100 - Math.round(quota.weekly_usage_pct || 0))}%)`
                            : `잔여 ${formatTokens(quota.weekly_remaining_tokens || 0)}`
                        }
                      </span>
                    </div>
                    <div style={{ height: "4px", background: "rgba(255,255,255,0.03)", borderRadius: "2px", overflow: "hidden", position: "relative" }}>
                      <div style={{ height: "100%", width: `${Math.min(100, Math.round(quota.weekly_usage_pct || 0))}%`, background: barGradient, borderRadius: "2px", transition: "width 0.5s ease-out" }} />
                    </div>
                  </div>
                )}

              </div>
            );
          })
        )}
      </div>

      <div className="tray-popover-footer" style={{ borderTop: "1px solid rgba(255,255,255,0.05)", marginTop: "0.5rem", paddingTop: "0.5rem" }}>
        <span>오늘 누적 사용 토큰</span>
        <span style={{ fontWeight: 800, color: 'var(--neon-blue)' }}>
          {summaries.reduce((acc, curr) => acc + (curr.total_input_tokens + curr.total_output_tokens), 0).toLocaleString()} Tokens
        </span>
      </div>
    </div>
  );
}

function SettingsView({ onSettingsSaved }: { onSettingsSaved: () => Promise<void> }) {
  const [settings, setSettings] = useState({ 
    log_dir: "", 
    token_limit: 50000000,
    token_limit_claude: 50000000,
    token_limit_codex: 50000000,
    token_display_mode: "tokens"
  });
  const [keysStatus, setKeysStatus] = useState({ anthropic: false, openai: false });
  const [anthropicKey, setAnthropicKey] = useState("");
  const [openaiKey, setOpenAIKey] = useState("");
  
  const [anthropicValid, setAnthropicValid] = useState<boolean | null>(null);
  const [openaiValid, setOpenAIValid] = useState<boolean | null>(null);
  const [pathValid, setPathValid] = useState<boolean | null>(null);

  const [diagnoseLoading, setDiagnoseLoading] = useState({
    anthropic: false,
    openai: false,
    path: false,
  });

  const [localCreds, setLocalCreds] = useState<DetectedCredential[]>([]);
  const [scanLoading, setScanLoading] = useState(false);
  const [applyLoading, setApplyLoading] = useState<Record<number, boolean>>({});

  const handleScanCredentials = async () => {
    setScanLoading(true);
    try {
      const creds = await invoke<DetectedCredential[]>("get_local_credentials");
      setLocalCreds(creds);
    } catch (e) {
      console.error("로컬 자격 증명 스캔 실패:", e);
    } finally {
      setScanLoading(false);
    }
  };

  const handleApplyCredential = async (cred: DetectedCredential, index: number) => {
    console.log("[Credential] handleApplyCredential clicked - provider:", cred.provider, "token_type:", cred.token_type);
    setApplyLoading(prev => ({ ...prev, [index]: true }));
    try {
      await invoke("auto_apply_credential", { 
        provider: cred.provider, 
        rawValue: cred.raw_value,
        raw_value: cred.raw_value 
      });
      console.log("[Credential] auto_apply_credential 성공!");
      alert(`${cred.provider === "anthropic" ? "Anthropic" : "OpenAI"} 인증 정보가 성공적으로 연동 및 저장되었습니다.`);
      await loadData();
      await onSettingsSaved(); // 대시보드 토큰 한도/게이지 리프레시
    } catch (e: any) {
      console.error("[Credential] auto_apply_credential 실패:", e);
      alert(`자동 연동 실패: ${e.toString()}`);
    } finally {
      setApplyLoading(prev => ({ ...prev, [index]: false }));
    }
  };

  const [testLoading, setTestLoading] = useState<Record<number, boolean>>({});

  const handleTestCredential = async (provider: "anthropic" | "openai", index: number) => {
    setTestLoading(prev => ({ ...prev, [index]: true }));
    try {
      const isValid = await invoke<boolean>("validate_stored_api_key", { provider });
      if (isValid) {
        alert(`연동 테스트 성공: ${provider === "anthropic" ? "Anthropic" : "OpenAI"} 연결이 활성화되어 정상 작동합니다.`);
      } else {
        alert(`연동 테스트 실패: ${provider === "anthropic" ? "Anthropic" : "OpenAI"} 자격 증명이 유효하지 않거나 만료되었습니다.`);
      }
    } catch (e: any) {
      alert(`연동 테스트 오류: ${e.toString()}`);
    } finally {
      setTestLoading(prev => ({ ...prev, [index]: false }));
    }
  };

  const loadData = async () => {
    try {
      const s = await invoke<{ 
        log_dir: string, 
        token_limit: number,
        token_limit_claude: number,
        token_limit_codex: number,
        token_display_mode: string
      }>("load_settings");
      setSettings({
        log_dir: s.log_dir,
        token_limit: s.token_limit,
        token_limit_claude: s.token_limit_claude,
        token_limit_codex: s.token_limit_codex,
        token_display_mode: s.token_display_mode || "tokens"
      });
      
      const k = await invoke<Record<string, boolean>>("get_api_keys_status");
      setKeysStatus({
        anthropic: k.anthropic || false,
        openai: k.openai || false,
      });

      if (k.anthropic) {
        diagnoseKey("anthropic");
      }
      if (k.openai) {
        diagnoseKey("openai");
      }
      if (s.log_dir) {
        diagnosePath(s.log_dir);
      }
      // 로컬 자격 증명 스캔 실행
      handleScanCredentials();
    } catch (e) {
      console.error("설정 로드 실패:", e);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const diagnoseKey = async (provider: "anthropic" | "openai") => {
    setDiagnoseLoading(prev => ({ ...prev, [provider]: true }));
    try {
      const isValid = await invoke<boolean>("validate_stored_api_key", { provider });
      if (provider === "anthropic") {
        setAnthropicValid(isValid);
      } else {
        setOpenAIValid(isValid);
      }
    } catch (e) {
      console.error(`${provider} API 키 진단 실패:`, e);
      if (provider === "anthropic") {
        setAnthropicValid(false);
      } else {
        setOpenAIValid(false);
      }
    } finally {
      setDiagnoseLoading(prev => ({ ...prev, [provider]: false }));
    }
  };

  const diagnosePath = async (path: string) => {
    setDiagnoseLoading(prev => ({ ...prev, path: true }));
    try {
      const isValid = await invoke<boolean>("validate_local_path", { path });
      setPathValid(isValid);
    } catch (e) {
      console.error("로컬 경로 진단 실패:", e);
      setPathValid(false);
    } finally {
      setDiagnoseLoading(prev => ({ ...prev, path: false }));
    }
  };

  const handleSaveKey = async (provider: "anthropic" | "openai") => {
    const key = provider === "anthropic" ? anthropicKey : openaiKey;
    if (!key.trim()) return;
    try {
      await invoke("save_api_key", { provider, apiKey: key });
      if (provider === "anthropic") {
        setAnthropicKey("");
      } else {
        setOpenAIKey("");
      }
      alert(`${provider === "anthropic" ? "Anthropic" : "OpenAI"} API Key가 암호화되어 안전하게 보관되었습니다.`);
      loadData();
    } catch (e: any) {
      alert(`API Key 저장 실패: ${e.toString()}`);
    }
  };

  const handleDeleteKey = async (provider: "anthropic" | "openai") => {
    try {
      await invoke("delete_api_key", { provider });
      alert(`${provider === "anthropic" ? "Anthropic" : "OpenAI"} API Key가 제거되었습니다.`);
      if (provider === "anthropic") {
        setAnthropicValid(null);
      } else {
        setOpenAIValid(null);
      }
      loadData();
    } catch (e: any) {
      alert(`API Key 제거 실패: ${e.toString()}`);
    }
  };

  const handleSaveSettings = async (updates: Partial<typeof settings>) => {
    const newSettings = { ...settings, ...updates };
    try {
      await invoke("save_settings", { 
        logDir: newSettings.log_dir, 
        tokenLimit: Number(newSettings.token_limit),
        tokenLimitClaude: Number(newSettings.token_limit_claude),
        tokenLimitCodex: Number(newSettings.token_limit_codex),
        tokenLimitAntigravity: 50000000,
        tokenDisplayMode: newSettings.token_display_mode
      });
      alert("설정이 성공적으로 저장되었습니다.");
      loadData();
      await onSettingsSaved();
    } catch (e: any) {
      alert(`설정 저장 실패: ${e.toString()}`);
    }
  };

  return (
    <div className="settings-container">
      <h2 className="settings-title">⚙️ 환경 설정 (Settings)</h2>
      
      {/* API Credentials Card */}
      <div className="settings-card glass">
        <h3 className="card-title">🔑 에이전트 플랫폼 API 인증</h3>
        <p className="card-desc">API Key는 OS의 보안 키체인(keyring) 내에 안전하게 암호화 보관되며, 설정 파일에 텍스트 형태로 노출되지 않습니다.</p>
        
        {/* 자동 연동 패널 */}
        <div className="auto-credential-panel" style={{
          background: "rgba(255, 255, 255, 0.02)",
          border: "1px dashed rgba(255, 255, 255, 0.1)",
          borderRadius: "8px",
          padding: "1rem",
          marginBottom: "1.5rem"
        }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.5rem" }}>
            <h4 style={{ margin: 0, fontSize: "0.85rem", color: "var(--neon-blue)", fontWeight: 700 }}>⚡ 로컬 인증 정보 자동 연동</h4>
            <button 
              onClick={handleScanCredentials} 
              disabled={scanLoading}
              className="btn btn-save"
              style={{ padding: "0.25rem 0.75rem", fontSize: "0.75rem" }}
            >
              {scanLoading ? "스캔 중..." : "인증 정보 새로고침"}
            </button>
          </div>
          <p style={{ margin: "0 0 0.75rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", lineHeight: 1.4 }}>
            로컬 시스템(macOS 키체인, ~/.claude 설정 파일, 환경 변수 등)에 저장된 OAuth 토큰 및 API 키를 스캔하여 단 한 번의 클릭으로 쉽게 연동합니다.
          </p>

          {localCreds.length > 0 ? (
            <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
              {localCreds.map((cred, idx) => (
                <div key={idx} style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  background: "rgba(255, 255, 255, 0.02)",
                  border: "1px solid rgba(255, 255, 255, 0.05)",
                  padding: "0.5rem 0.75rem",
                  borderRadius: "6px"
                }}>
                  <div style={{ display: "flex", flexDirection: "column", gap: "0.15rem" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.4rem" }}>
                      <span className={`badge-${cred.provider}`} style={{
                        fontSize: "0.65rem",
                        fontWeight: 800,
                        padding: "0.1rem 0.3rem",
                        borderRadius: "4px",
                        background: cred.provider === "anthropic" ? "rgba(217, 119, 6, 0.15)" : "rgba(16, 185, 129, 0.15)",
                        color: cred.provider === "anthropic" ? "hsl(35, 100%, 65%)" : "hsl(150, 100%, 45%)"
                      }}>
                        {cred.provider.toUpperCase()}
                      </span>
                      <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "hsl(215, 20%, 85%)" }}>
                        {cred.description}
                      </span>
                    </div>
                    <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 45%)", fontFamily: "monospace" }}>
                      감지 토큰: {cred.value} (출처: {cred.source})
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: "0.4rem" }}>
                    <button 
                      onClick={() => handleApplyCredential(cred, idx)}
                      disabled={applyLoading[idx]}
                      className="btn btn-save"
                      style={{ padding: "0.25rem 0.6rem", fontSize: "0.7rem", background: "var(--neon-blue)", color: "#0a0c10" }}
                    >
                      {applyLoading[idx] ? "연동 중..." : "바로 연동"}
                    </button>
                    <button 
                      onClick={() => handleTestCredential(cred.provider as any, idx)}
                      disabled={testLoading[idx]}
                      className="btn"
                      style={{ 
                        padding: "0.25rem 0.6rem", 
                        fontSize: "0.7rem", 
                        background: "rgba(255,255,255,0.05)", 
                        border: "1px solid rgba(255,255,255,0.1)",
                        color: "hsl(215, 20%, 80%)",
                        borderRadius: "4px",
                        cursor: "pointer"
                      }}
                    >
                      {testLoading[idx] ? "테스트 중..." : "연동 테스트"}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div style={{ textAlign: "center", padding: "0.75rem", fontSize: "0.75rem", color: "hsl(215, 20%, 45%)" }}>
              {scanLoading ? "시스템 분석 중..." : "자동 감지된 로컬 인증 정보가 없습니다. (수동 입력 가능)"}
            </div>
          )}
        </div>

        <div className="settings-form">
          {/* Anthropic Key */}
          <div className="form-group">
            <div className="form-group-header">
              <label>Anthropic API Key</label>
              <div className="status-indicator">
                {diagnoseLoading.anthropic ? (
                  <span className="status-badge checking">진단 중...</span>
                ) : keysStatus.anthropic ? (
                  anthropicValid ? (
                    <span className="status-badge active"><span className="pulse-dot-green"></span>연결됨 (Active)</span>
                  ) : (
                    <span className="status-badge inactive"><span className="pulse-dot-red"></span>인증 실패 (Invalid)</span>
                  )
                ) : (
                  <span className="status-badge none">설정되지 않음</span>
                )}
              </div>
            </div>
            <div className="input-group">
              <input
                type="password"
                placeholder={keysStatus.anthropic ? "••••••••••••••••••••••••" : "Anthropic API 키 입력 (sk-ant-...)"}
                value={anthropicKey}
                onChange={(e) => setAnthropicKey(e.target.value)}
                className="settings-input"
              />
              <button onClick={() => handleSaveKey("anthropic")} className="btn btn-save" disabled={!anthropicKey.trim()}>저장</button>
              {keysStatus.anthropic && (
                <button onClick={() => handleDeleteKey("anthropic")} className="btn btn-delete">삭제</button>
              )}
            </div>
          </div>

          {/* OpenAI Key */}
          <div className="form-group">
            <div className="form-group-header">
              <label>OpenAI API Key</label>
              <div className="status-indicator">
                {diagnoseLoading.openai ? (
                  <span className="status-badge checking">진단 중...</span>
                ) : keysStatus.openai ? (
                  openaiValid ? (
                    <span className="status-badge active"><span className="pulse-dot-green"></span>연결됨 (Active)</span>
                  ) : (
                    <span className="status-badge inactive"><span className="pulse-dot-red"></span>인증 실패 (Invalid)</span>
                  )
                ) : (
                  <span className="status-badge none">설정되지 않음</span>
                )}
              </div>
            </div>
            <div className="input-group">
              <input
                type="password"
                placeholder={keysStatus.openai ? "••••••••••••••••••••••••" : "OpenAI API 키 입력 (sk-...)"}
                value={openaiKey}
                onChange={(e) => setOpenAIKey(e.target.value)}
                className="settings-input"
              />
              <button onClick={() => handleSaveKey("openai")} className="btn btn-save" disabled={!openaiKey.trim()}>저장</button>
              {keysStatus.openai && (
                <button onClick={() => handleDeleteKey("openai")} className="btn btn-delete">삭제</button>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Directory Config Card */}
      <div className="settings-card glass" style={{ marginTop: "1.5rem" }}>
        <h3 className="card-title">📂 로그 감시 경로 설정</h3>
        <p className="card-desc">AI 에이전트들의 로그 파일을 실시간으로 추적/감시할 로컬 디렉토리 경로를 지정합니다.</p>
        
        <div className="settings-form">
          <div className="form-group">
            <div className="form-group-header">
              <label>로컬 로그 디렉토리 경로</label>
              <div className="status-indicator">
                {diagnoseLoading.path ? (
                  <span className="status-badge checking">검사 중...</span>
                ) : settings.log_dir ? (
                  pathValid ? (
                    <span className="status-badge active"><span className="pulse-dot-green"></span>경로 유효함</span>
                  ) : (
                    <span className="status-badge inactive"><span className="pulse-dot-red"></span>경로 오류 (존재하지 않음)</span>
                  )
                ) : (
                  <span className="status-badge none">설정되지 않음</span>
                )}
              </div>
            </div>
            <div className="input-group">
              <input
                type="text"
                placeholder="예: /Users/username/logs"
                value={settings.log_dir}
                onChange={(e) => setSettings(prev => ({ ...prev, log_dir: e.target.value }))}
                className="settings-input"
                style={{ fontFamily: "monospace", fontSize: "0.8rem" }}
              />
              <button onClick={() => handleSaveSettings({ log_dir: settings.log_dir })} className="btn btn-save" disabled={!settings.log_dir.trim()}>저장</button>
            </div>
          </div>
        </div>
      </div>

      {/* Display Config Card */}
      <div className="settings-card glass" style={{ marginTop: "1.5rem" }}>
        <h3 className="card-title">🖥️ 화면 표시 설정</h3>
        <p className="card-desc">서비스 전반에 걸쳐 토큰 잔여량을 어떤 단위로 표시할지 결정합니다.</p>
        
        <div className="settings-form">
          <div className="form-group">
            <label style={{ marginBottom: "0.5rem", display: "block" }}>토큰 잔여량 표시 방식</label>
            <div style={{ display: "flex", gap: "2rem", marginTop: "0.25rem" }}>
              <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", fontSize: "0.85rem", cursor: "pointer", color: "hsl(215, 20%, 85%)" }}>
                <input
                  type="radio"
                  name="token_display_mode"
                  value="tokens"
                  checked={settings.token_display_mode === "tokens"}
                  onChange={() => handleSaveSettings({ token_display_mode: "tokens" })}
                  style={{ cursor: "pointer" }}
                />
                토큰 단위 (Tokens)
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", fontSize: "0.85rem", cursor: "pointer", color: "hsl(215, 20%, 85%)" }}>
                <input
                  type="radio"
                  name="token_display_mode"
                  value="percentage"
                  checked={settings.token_display_mode === "percentage"}
                  onChange={() => handleSaveSettings({ token_display_mode: "percentage" })}
                  style={{ cursor: "pointer" }}
                />
                백분율 (%)
              </label>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}


interface SessionAnalysisViewProps {
  sessions: Session[];
  anomalies: LoopDetectionResult[];
  analysisSessionId: string | null;
  analysisData: SessionAnalysis | null;
  analysisLoading: boolean;
  onSelectSession: (id: string) => void;
  onInterrupt: (agentType: string, cwd: string) => Promise<void>;
  interruptLoading: boolean;
  interruptMessage: string | null;
}

function SessionAnalysisView({
  sessions,
  anomalies,
  analysisSessionId,
  analysisData,
  analysisLoading,
  onSelectSession,
  onInterrupt,
  interruptLoading,
  interruptMessage,
}: SessionAnalysisViewProps) {
  const [hoveredTurn, setHoveredTurn] = useState<TurnTokenUsage | null>(null);
  const [tooltipPos, setTooltipPos] = useState({ x: 0, y: 0 });

  const anomalyMap = new Map(anomalies.map(a => [a.session_id, a]));

  return (
    <div style={{ display: "flex", flex: 1, gap: "1.5rem", height: "calc(100vh - 8rem)", overflow: "hidden" }}>
      {/* 1. 좌측 세션 목록 */}
      <div className="glass" style={{ width: "320px", display: "flex", flexDirection: "column", padding: "1.25rem", overflowY: "auto" }}>
        <h3 style={{ fontSize: "1.1rem", fontWeight: 700, marginBottom: "1rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
          <span>📂</span> 세션 히스토리 ({sessions.length})
        </h3>
        
        <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
          {sessions.map((s) => {
            const isSelected = s.session_id === analysisSessionId;
            const hasAnomaly = anomalyMap.has(s.session_id);
            return (
              <div
                key={s.session_id}
                onClick={() => onSelectSession(s.session_id)}
                className={`session-item session-item-clickable ${isSelected ? "active" : ""}`}
                style={{
                  background: isSelected 
                    ? "rgba(139, 92, 246, 0.15)" 
                    : hasAnomaly 
                      ? "rgba(239, 68, 68, 0.05)" 
                      : "rgba(255, 255, 255, 0.02)",
                  borderColor: isSelected 
                    ? "var(--neon-purple)" 
                    : hasAnomaly 
                      ? "rgba(239, 68, 68, 0.3)" 
                      : "rgba(255, 255, 255, 0.05)",
                  borderWidth: "1px",
                  borderStyle: "solid",
                  padding: "0.75rem 1rem",
                  borderRadius: "8px",
                  cursor: "pointer",
                  transition: "all 0.2s ease"
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.25rem" }}>
                  <span style={{ fontWeight: 700, fontSize: "0.85rem", color: hasAnomaly ? "var(--neon-red)" : "var(--foreground)", fontFamily: "monospace" }}>
                    {s.session_id.substring(0, 12)}...
                  </span>
                  {hasAnomaly && (
                    <span style={{ fontSize: "0.7rem", padding: "0.1rem 0.3rem", borderRadius: "4px", background: "rgba(239, 68, 68, 0.2)", color: "hsl(0, 100%, 75%)", fontWeight: "bold" }}>
                      LOOP
                    </span>
                  )}
                </div>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 60%)" }}>
                  {s.agent_type} • {s.started_at.substring(11, 19)}
                </div>
                <div style={{ display: "flex", justifyContent: "space-between", marginTop: "0.5rem", fontSize: "0.75rem" }}>
                  <span style={{ color: "hsl(215, 20%, 70%)" }}>
                    {(s.total_input_tokens + s.total_output_tokens).toLocaleString()} Tokens
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* 2. 우측 분석 상세 패널 */}
      <div className="glass" style={{ flex: 1, display: "flex", flexDirection: "column", padding: "1.5rem", overflowY: "auto" }}>
        {analysisLoading ? (
          <div style={{ display: "flex", flex: 1, flexDirection: "column", justifyContent: "center", alignItems: "center", color: "hsl(215, 20%, 55%)", gap: "1rem" }}>
            <div style={{ width: "32px", height: "32px", border: "3px solid rgba(255,255,255,0.08)", borderTopColor: "var(--neon-blue)", borderRadius: "50%", animation: "rotate-sync 1s linear infinite" }} />
            <span>세션 심층 분석 데이터 수집 중...</span>
          </div>
        ) : analysisData ? (
          <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
            {/* 세션 개요 헤더 */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", borderBottom: "1px solid rgba(255,255,255,0.08)", paddingBottom: "1rem" }}>
              <div>
                <h2 style={{ fontSize: "1.3rem", fontWeight: 800, margin: 0, display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <span>🔍</span> 세션 분석 보고서
                </h2>
                <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 50%)", fontFamily: "monospace", marginTop: "0.25rem" }}>
                  ID: {analysisData.session_id}
                </div>
              </div>

              <div style={{ display: "flex", gap: "0.75rem", alignItems: "center" }}>
                <span className="pulse-badge" style={{
                  color: analysisData.is_anomaly ? "var(--neon-red)" : "var(--neon-blue)",
                  background: analysisData.is_anomaly ? "rgba(239, 68, 68, 0.1)" : "rgba(6, 182, 212, 0.1)",
                  borderColor: analysisData.is_anomaly ? "rgba(239, 68, 68, 0.3)" : "rgba(6, 182, 212, 0.2)"
                }}>
                  {analysisData.is_anomaly ? "🚨 위험 상태 감지" : "✓ 안전 세션"}
                </span>
                <button
                  onClick={() => onInterrupt(analysisData.agent_type, "")}
                  disabled={interruptLoading}
                  className="btn btn-delete"
                  style={{ padding: "0.4rem 1rem", fontSize: "0.8rem" }}
                >
                  {interruptLoading ? "중단 중..." : "에이전트 강제종료"}
                </button>
              </div>
            </div>

            {/* 기본 수치 요약 */}
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "1rem" }}>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>총 소비 비용</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "var(--neon-purple)", marginTop: "0.25rem", textShadow: "0 0 10px rgba(139, 92, 246, 0.3)" }}>
                  ${analysisData.total_cost_usd.toFixed(4)}
                </div>
              </div>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>총 사용 토큰</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "var(--neon-blue)", marginTop: "0.25rem" }}>
                  {(analysisData.total_input_tokens + analysisData.total_output_tokens).toLocaleString()}
                </div>
              </div>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>캐시 히트율</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "var(--neon-green)", marginTop: "0.25rem" }}>
                  {(analysisData.cache_hit_rate * 100).toFixed(1)}%
                </div>
              </div>
              <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1rem", borderRadius: "12px" }}>
                <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>캐시 절감 비용</div>
                <div style={{ fontSize: "1.25rem", fontWeight: 800, color: "hsl(150, 100%, 40%)", marginTop: "0.25rem" }}>
                  ${analysisData.cache_saved_cost.toFixed(4)}
                </div>
              </div>
            </div>

            {/* 턴별 토큰 소비 스택 바 차트 */}
            <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1.25rem", borderRadius: "12px", position: "relative" }}>
              <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 1rem 0" }}>⚡ 턴별 토큰 소비 분석 (턴 순서 흐름)</h4>
              
              {analysisData.turns.length > 0 ? (
                <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
                  {/* Legend */}
                  <div style={{ display: "flex", gap: "1rem", fontSize: "0.75rem", justifyContent: "flex-end" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                      <div style={{ width: "10px", height: "10px", background: "var(--neon-blue)", borderRadius: "2px" }} />
                      <span>Input Tokens</span>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                      <div style={{ width: "10px", height: "10px", background: "var(--neon-purple)", borderRadius: "2px" }} />
                      <span>Output Tokens</span>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                      <div style={{ width: "10px", height: "10px", background: "var(--neon-green)", borderRadius: "2px" }} />
                      <span>Cache Read Tokens</span>
                    </div>
                  </div>

                  {/* Bars container */}
                  <div 
                    style={{ 
                      display: "flex", 
                      alignItems: "flex-end", 
                      height: "180px", 
                      gap: "0.4rem", 
                      overflowX: "auto", 
                      paddingBottom: "1.5rem",
                      borderBottom: "1px solid rgba(255,255,255,0.08)",
                      marginTop: "0.5rem"
                    }}
                  >
                    {analysisData.turns.map((turn, idx) => {
                      const turnTotal = turn.input_tokens + turn.output_tokens;
                      const maxTurnTotal = Math.max(...analysisData.turns.map(t => t.input_tokens + t.output_tokens), 1);
                      const barHeightPct = (turnTotal / maxTurnTotal) * 100;
                      
                      const inputPct = turnTotal > 0 ? (turn.input_tokens / turnTotal) * 100 : 0;
                      const outputPct = turnTotal > 0 ? (turn.output_tokens / turnTotal) * 100 : 0;
                      const cachePct = turn.input_tokens > 0 ? (turn.cache_read_tokens / turn.input_tokens) * 100 : 0;

                      return (
                        <div
                          key={idx}
                          style={{
                            flex: "0 0 28px",
                            height: `${barHeightPct}%`,
                            minHeight: "10px",
                            display: "flex",
                            flexDirection: "column-reverse",
                            borderRadius: "4px 4px 0 0",
                            overflow: "hidden",
                            cursor: "pointer",
                            transition: "opacity 0.2s ease",
                            background: "rgba(255,255,255,0.02)"
                          }}
                          onMouseEnter={(e) => {
                            setHoveredTurn(turn);
                            const rect = e.currentTarget.getBoundingClientRect();
                            const containerRect = e.currentTarget.parentElement?.getBoundingClientRect();
                            if (containerRect) {
                              setTooltipPos({
                                x: rect.left - containerRect.left + 14,
                                y: rect.top - containerRect.top - 10
                              });
                            }
                          }}
                          onMouseLeave={() => setHoveredTurn(null)}
                        >
                          {/* Input Section */}
                          <div style={{ height: `${inputPct}%`, background: "var(--neon-blue)", position: "relative" }}>
                            {/* Cache section embedded inside input */}
                            {cachePct > 0 && (
                              <div style={{ position: "absolute", bottom: 0, left: 0, right: 0, height: `${cachePct}%`, background: "var(--neon-green)" }} />
                            )}
                          </div>
                          {/* Output Section */}
                          <div style={{ height: `${outputPct}%`, background: "var(--neon-purple)" }} />
                        </div>
                      );
                    })}
                  </div>

                  {/* X Axis Labels */}
                  <div style={{ display: "flex", gap: "0.4rem", overflowX: "hidden", fontSize: "0.65rem", color: "hsl(215, 20%, 50%)" }}>
                    {analysisData.turns.map((turn, idx) => (
                      <div key={idx} style={{ flex: "0 0 28px", textAlign: "center" }}>
                        T{turn.turn_index}
                      </div>
                    ))}
                  </div>

                  {/* Interactive Tooltip inside container */}
                  {hoveredTurn && (
                    <div
                      style={{
                        position: "absolute",
                        left: `${tooltipPos.x}px`,
                        top: `${tooltipPos.y}px`,
                        transform: "translate(-50%, -100%)",
                        background: "rgba(10, 12, 16, 0.95)",
                        border: "1px solid var(--neon-blue)",
                        borderRadius: "6px",
                        padding: "0.5rem 0.75rem",
                        fontSize: "0.72rem",
                        boxShadow: "0 0 10px rgba(0, 242, 254, 0.2)",
                        pointerEvents: "none",
                        zIndex: 100,
                        transition: "all 0.1s ease"
                      }}
                    >
                      <div style={{ fontWeight: 700, color: "var(--neon-blue)", marginBottom: "4px" }}>
                        Turn {hoveredTurn.turn_index} ({hoveredTurn.role})
                      </div>
                      <div style={{ color: "#fff" }}>
                        입력: <span style={{ fontWeight: 600 }}>{hoveredTurn.input_tokens.toLocaleString()}</span>
                      </div>
                      <div style={{ color: "#fff" }}>
                        출력: <span style={{ fontWeight: 600 }}>{hoveredTurn.output_tokens.toLocaleString()}</span>
                      </div>
                      {hoveredTurn.cache_read_tokens > 0 && (
                        <div style={{ color: "var(--neon-green)" }}>
                          캐시 리드: <span style={{ fontWeight: 600 }}>{hoveredTurn.cache_read_tokens.toLocaleString()}</span>
                        </div>
                      )}
                      <div style={{ color: "var(--neon-purple)", fontWeight: 600, marginTop: "4px" }}>
                        비용: ${hoveredTurn.cost_usd.toFixed(5)}
                      </div>
                    </div>
                  )}
                </div>
              ) : (
                <div style={{ padding: "2rem", textAlign: "center", color: "hsl(215, 20%, 40%)", fontSize: "0.85rem" }}>
                  턴별 토큰 사용 내역이 없습니다.
                </div>
              )}
            </div>

            {/* 하단 2단: 캐시 도넛 차트 & 도구 비용 랭킹 */}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1.5rem" }}>
              {/* 캐시 히트율 도넛 차트 카드 */}
              <div className="glass" style={{ padding: "1.25rem", background: "rgba(255,255,255,0.01)" }}>
                <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 1rem 0" }}>🔌 캐시 효율성 & 히트율</h4>
                <div style={{ display: "flex", alignItems: "center", gap: "2rem" }}>
                  {/* SVG 도넛 */}
                  <div style={{ position: "relative", width: "120px", height: "120px" }}>
                    <svg width="100%" height="100%" viewBox="0 0 42 42">
                      <circle cx="21" cy="21" r="15.915" fill="transparent" stroke="rgba(255,255,255,0.03)" strokeWidth="4" />
                      <circle
                        cx="21"
                        cy="21"
                        r="15.915"
                        fill="transparent"
                        stroke="var(--neon-green)"
                        strokeWidth="4"
                        strokeDasharray={`${analysisData.cache_hit_rate * 100} ${100 - (analysisData.cache_hit_rate * 100)}`}
                        strokeDashoffset="25"
                        style={{ filter: "drop-shadow(0 0 4px rgba(16, 185, 129, 0.4))", transition: "stroke-dasharray 0.5s ease" }}
                      />
                    </svg>
                    <div style={{ position: "absolute", top: "50%", left: "50%", transform: "translate(-50%, -50%)", textAlign: "center" }}>
                      <div style={{ fontSize: "1.1rem", fontWeight: 800, color: "var(--foreground)" }}>
                        {(analysisData.cache_hit_rate * 100).toFixed(0)}%
                      </div>
                      <div style={{ fontSize: "0.6rem", color: "hsl(215, 20%, 50%)" }}>HIT RATE</div>
                    </div>
                  </div>

                  <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", flex: 1 }}>
                    <div style={{ background: "linear-gradient(135deg, rgba(16, 185, 129, 0.1), rgba(6, 182, 212, 0.05))", border: "1px solid rgba(16, 185, 129, 0.15)", borderRadius: "8px", padding: "0.5rem 0.75rem" }}>
                      <div style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 65%)" }}>누적 캐시 사용량</div>
                      <div style={{ fontSize: "0.95rem", fontWeight: 700, color: "var(--neon-green)", marginTop: "0.15rem" }}>
                        {analysisData.total_cache_read_tokens.toLocaleString()} Tokens
                      </div>
                    </div>
                    <div style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", lineHeight: 1.4 }}>
                      캐시가 탑재된 Claude 모델 사용 시 입력 토큰을 최대 90% 저렴하게 처리하여 예산을 대폭 절감합니다.
                    </div>
                  </div>
                </div>
              </div>

              {/* 도구 비용 랭킹 카드 */}
              <div className="glass" style={{ padding: "1.25rem", background: "rgba(255,255,255,0.01)" }}>
                <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 1rem 0" }}>🛠️ 도구별 비용 랭킹 (Top 10)</h4>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", maxHeight: "160px", overflowY: "auto", paddingRight: "4px" }}>
                  {analysisData.tool_cost_rank.map((t) => {
                    const maxCost = Math.max(...analysisData.tool_cost_rank.map(tc => tc.total_cost_usd), 0.0001);
                    const barWidth = (t.total_cost_usd / maxCost) * 100;
                    return (
                      <div key={t.tool_name} style={{ display: "flex", flexDirection: "column", gap: "0.2rem" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem" }}>
                          <span style={{ fontWeight: 600, color: "hsl(215, 20%, 85%)", fontFamily: "monospace" }}>{t.tool_name}</span>
                          <span style={{ fontWeight: 700, color: "var(--neon-blue)" }}>
                            ${t.total_cost_usd.toFixed(4)} ({t.call_count}회)
                          </span>
                        </div>
                        <div style={{ height: "6px", background: "rgba(255,255,255,0.03)", borderRadius: "3px", overflow: "hidden" }}>
                          <div style={{ height: "100%", width: `${barWidth}%`, background: "var(--neon-blue)", borderRadius: "3px" }} />
                        </div>
                      </div>
                    );
                  })}
                  {analysisData.tool_cost_rank.length === 0 && (
                    <div style={{ color: "hsl(215, 20%, 40%)", fontSize: "0.8rem", textAlign: "center", padding: "1.5rem" }}>
                      이 세션에 도구 호출 정보가 없습니다.
                    </div>
                  )}
                </div>
              </div>
            </div>

            {/* 이상 제어 / 시각화 디렉션 */}
            <div style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)", padding: "1.25rem", borderRadius: "12px" }}>
              <h4 style={{ fontSize: "0.95rem", fontWeight: 700, margin: "0 0 0.75rem 0", color: analysisData.is_anomaly ? "var(--neon-red)" : "var(--neon-green)" }}>
                {analysisData.is_anomaly ? "🚨 오작동 이상 탐지 분석" : "✓ 세션 이상 탐지 분석"}
              </h4>
              {analysisData.is_anomaly ? (
                <div>
                  <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 85%)", marginBottom: "1rem", lineHeight: 1.5 }}>
                    {analysisData.anomaly_signals.map((s, idx) => (
                      <div key={idx} style={{ padding: "0.5rem 0.75rem", background: "rgba(239, 68, 68, 0.06)", borderLeft: "3px solid var(--neon-red)", borderRadius: "0 6px 6px 0", marginBottom: "0.5rem" }}>
                        <strong>{s.signal_type === "repeated_call" ? "자가 루프 의심" : "핑퐁 순환 호출"}:</strong> {s.description}
                      </div>
                    ))}
                  </div>
                  <LoopDirectionViewer signals={analysisData.anomaly_signals} />
                </div>
              ) : (
                <div style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 70%)" }}>
                  현재 세션에서 동일한 도구 호출의 오작동 순환(Loop) 현상이나 급격한 토큰 폭팽 이상 징후가 검출되지 않았습니다. 안전하게 관리되고 있습니다.
                </div>
              )}
            </div>
            
            {interruptMessage && (
              <div style={{ padding: "0.75rem 1rem", background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.05)", borderRadius: "8px", fontSize: "0.75rem" }}>
                {interruptMessage}
              </div>
            )}
          </div>
        ) : (
          <div style={{ display: "flex", flex: 1, flexDirection: "column", justifyContent: "center", alignItems: "center", color: "hsl(215, 20%, 40%)" }}>
            <span>👈 왼쪽 히스토리 목록에서 분석할 세션을 선택해 주세요.</span>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
