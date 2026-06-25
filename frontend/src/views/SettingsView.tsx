import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PlanQuotaInfo, DetectedCredential, DetectedLogPath } from "../types";
import { formatTokens, formatResetTime } from "../utils/formatters";

interface SettingsViewProps {
  onSettingsSaved: () => Promise<void>;
  activeSection: string;
}

/**
 * 대시보드 내부의 설정(Settings) 및 연동(Integrations) 탭 뷰 컴포넌트
 * 
 * 로그 디렉토리 감시 경로 설정, 수동 API Key 관리 및 로컬 키체인/설정 파일 자동 크리덴셜 연동과 검증 등을 처리합니다.
 */
export function SettingsView({ onSettingsSaved, activeSection }: SettingsViewProps) {
  const [settings, setSettings] = useState({
    log_dir: "",
    claude_log_dir: "",
    codex_log_dir: "",
    antigravity_log_dir: "",
    token_limit: 50000000,
    token_limit_claude: 50000000,
    token_limit_codex: 50000000,
    token_limit_antigravity: 50000000,
    claude_plan: "pro",
    openai_plan: "tier1",
    token_display_mode: "tokens",
    refresh_interval: 3
  });
  const [keysStatus, setKeysStatus] = useState({ anthropic: false, openai: false });
  const [quotaInfo, setQuotaInfo] = useState<PlanQuotaInfo[]>([]);
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
  const [logPaths, setLogPaths] = useState<DetectedLogPath[]>([]);
  const [logPathScanLoading, setLogPathScanLoading] = useState(false);
  const [logPathDrafts, setLogPathDrafts] = useState<Record<string, string>>({});
  const [applyLoading, setApplyLoading] = useState<Record<number, boolean>>({});
  const [testLoading, setTestLoading] = useState<Record<number, boolean>>({});
  const [autoCredsTestResults, setAutoCredsTestResults] = useState<Record<number, { checked: boolean; valid: boolean | null; error?: string }>>({});

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

  const handleScanLogPaths = async () => {
    setLogPathScanLoading(true);
    try {
      const paths = await invoke<DetectedLogPath[]>("get_detected_log_paths");
      setLogPaths(paths);
      // 입력 초안을 현재 설정값으로 동기화
      const drafts: Record<string, string> = {};
      paths.forEach(p => { drafts[p.agent] = p.configured_path; });
      setLogPathDrafts(drafts);
    } catch (e) {
      console.error("세션 로그 경로 스캔 실패:", e);
    } finally {
      setLogPathScanLoading(false);
    }
  };

  // 에이전트별 로그 경로 저장 ("" 전달 시 기본 경로 자동감지로 복원)
  const handleSaveLogPath = async (agent: string, path: string) => {
    const fieldMap: Record<string, "claude_log_dir" | "codex_log_dir" | "antigravity_log_dir"> = {
      claude_code: "claude_log_dir",
      codex: "codex_log_dir",
      antigravity: "antigravity_log_dir",
    };
    const field = fieldMap[agent];
    if (!field) return;
    await handleSaveSettings({ [field]: path.trim() } as Partial<typeof settings>);
    await handleScanLogPaths();
  };

  const handleApplyCredential = async (cred: DetectedCredential, index: number) => {
    setApplyLoading(prev => ({ ...prev, [index]: true }));
    try {
      await invoke("auto_apply_credential", { 
        provider: cred.provider, 
        rawValue: cred.raw_value,
        raw_value: cred.raw_value 
      });
      alert(`${cred.provider === "anthropic" ? "Anthropic" : "OpenAI"} 인증 정보가 성공적으로 연동 및 저장되었습니다.`);
      await loadData();
      await onSettingsSaved(); // 대시보드 토큰 한도/게이지 리프레시
    } catch (e: any) {
      console.error("auto_apply_credential 실패:", e);
      alert(`자동 연동 실패: ${e.toString()}`);
    } finally {
      setApplyLoading(prev => ({ ...prev, [index]: false }));
    }
  };

  const handleTestCredential = async (provider: "anthropic" | "openai", index: number) => {
    const cred = localCreds[index];
    if (!cred) return;

    setTestLoading(prev => ({ ...prev, [index]: true }));
    setAutoCredsTestResults(prev => ({
      ...prev,
      [index]: { checked: true, valid: null } // 검사 중
    }));

    try {
      const isValid = await invoke<boolean>("validate_api_key_value", { 
        provider, 
        apiKey: cred.raw_value,
        api_key: cred.raw_value 
      });
      setAutoCredsTestResults(prev => ({
        ...prev,
        [index]: { checked: true, valid: isValid }
      }));
    } catch (e: any) {
      setAutoCredsTestResults(prev => ({
        ...prev,
        [index]: { checked: true, valid: false, error: e.toString() }
      }));
    } finally {
      setTestLoading(prev => ({ ...prev, [index]: false }));
    }
  };

  const loadData = async () => {
    try {
      const s = await invoke<{
        log_dir: string,
        claude_log_dir: string,
        codex_log_dir: string,
        antigravity_log_dir: string,
        token_limit: number,
        token_limit_claude: number,
        token_limit_codex: number,
        token_limit_antigravity: number,
        claude_plan: string,
        openai_plan: string,
        token_display_mode: string,
        refresh_interval: number
      }>("load_settings");

      setSettings({
        log_dir: s.log_dir,
        claude_log_dir: s.claude_log_dir || "",
        codex_log_dir: s.codex_log_dir || "",
        antigravity_log_dir: s.antigravity_log_dir || "",
        token_limit: s.token_limit,
        token_limit_claude: s.token_limit_claude,
        token_limit_codex: s.token_limit_codex,
        token_limit_antigravity: s.token_limit_antigravity || 50000000,
        claude_plan: s.claude_plan || "pro",
        openai_plan: s.openai_plan || "tier1",
        token_display_mode: s.token_display_mode || "tokens",
        refresh_interval: s.refresh_interval ?? 3
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
      // 세션 로그 경로 자동 감지 스캔 실행
      handleScanLogPaths();

      try {
        const quota = await invoke<PlanQuotaInfo[]>("get_subscription_quota");
        setQuotaInfo(quota);
      } catch (e) {
        console.error("실시간 구독 정보 로드 실패:", e);
      }
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
      await invoke("save_api_key", { 
        provider, 
        apiKey: key,
        api_key: key 
      });
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
        claudeLogDir: newSettings.claude_log_dir,
        codexLogDir: newSettings.codex_log_dir,
        antigravityLogDir: newSettings.antigravity_log_dir,
        tokenLimit: Number(newSettings.token_limit),
        tokenLimitClaude: Number(newSettings.token_limit_claude),
        tokenLimitCodex: Number(newSettings.token_limit_codex),
        tokenLimitAntigravity: Number(newSettings.token_limit_antigravity),
        claudePlan: newSettings.claude_plan,
        openaiPlan: newSettings.openai_plan,
        tokenDisplayMode: newSettings.token_display_mode,
        refreshInterval: Number(newSettings.refresh_interval)
      });
      alert("설정이 성공적으로 저장되었습니다.");
      loadData();
      await onSettingsSaved();
    } catch (e: any) {
      alert(`설정 저장 실패: ${e.toString()}`);
    }
  };

  // Provider별 자동 감지 크리덴셜 렌더링 헬퍼
  const renderProviderAutoCreds = (provider: "anthropic" | "openai") => {
    const filtered = localCreds.filter(c => c.provider === provider);
    if (filtered.length === 0) {
      return (
        <div style={{ padding: "0.75rem", fontSize: "0.75rem", color: "hsl(215, 20%, 45%)", background: "rgba(255,255,255,0.01)", borderRadius: "6px", border: "1px dashed rgba(255,255,255,0.05)" }}>
          {scanLoading ? "시스템 분석 중..." : `자동 감지된 로컬 ${provider === "anthropic" ? "Claude" : "OpenAI"} 인증 정보가 없습니다.`}
        </div>
      );
    }

    return (
      <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
        {filtered.map((cred, idx) => {
          const originalIdx = localCreds.findIndex(c => c.raw_value === cred.raw_value && c.provider === cred.provider);
          
          let tooltipInfo = "로컬 시스템 설정 또는 환경 변수에서 감지된 에이전트 인증 정보입니다.";
          if (cred.source === "Keychain") {
            if (cred.provider === "anthropic") {
              tooltipInfo = "Claude Code CLI가 macOS 키체인에 안전하게 저장하여 사용하는 OAuth Access Token 정보입니다.";
            } else {
              tooltipInfo = "OpenAI 에이전트 연동용으로 macOS 키체인에 안전하게 저장되어 있는 API Key 정보입니다.";
            }
          } else if (cred.description.includes("세션 키") || cred.description.includes("fetch-claude-usage")) {
            tooltipInfo = "로컬 세션 감지 스크립트를 통해 확인된 Claude.ai 웹 브라우저 로그인 세션(sk-ant-sid) 정보입니다.";
          } else if (cred.source === "ConfigFile") {
            tooltipInfo = "로컬 에이전트 설정 파일(.config/claude 등)에서 감지된 인증 토큰 정보입니다.";
          }

          return (
            <div key={idx} style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              background: "rgba(255, 255, 255, 0.02)",
              border: "1px solid rgba(255, 255, 255, 0.05)",
              padding: "0.6rem 0.85rem",
              borderRadius: "8px"
            }}>
              <div style={{ display: "flex", flexDirection: "column", gap: "0.2rem" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                  <span style={{ fontSize: "0.8rem", fontWeight: 600, color: "hsl(215, 20%, 85%)" }}>
                    {cred.description}
                  </span>
                  <div className="tooltip-container">
                    <span style={{ cursor: "help", fontSize: "0.75rem", opacity: 0.5 }}>ℹ️</span>
                    <div className="tooltip-text" style={{ bottom: "125%", left: "0", transform: "none", width: "270px" }}>
                      {tooltipInfo}
                    </div>
                  </div>
                </div>
                <span style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 45%)", fontFamily: "monospace" }}>
                  감지 토큰: {cred.value} (출처: {cred.source})
                </span>
                {autoCredsTestResults[originalIdx]?.checked && (
                  <div style={{ marginTop: "0.2rem", display: "flex", alignItems: "center" }}>
                    {autoCredsTestResults[originalIdx].valid === null ? (
                      <span style={{ fontSize: "0.7rem", color: "hsl(35, 100%, 65%)" }}>🔄 테스트 검증 중...</span>
                    ) : autoCredsTestResults[originalIdx].valid === true ? (
                      <span style={{ fontSize: "0.7rem", color: "hsl(150, 100%, 45%)" }}>
                        ✅ 테스트 성공 (유효한 크리덴셜)
                      </span>
                    ) : (
                      <span style={{ fontSize: "0.7rem", color: "hsl(0, 100%, 65%)" }}>
                        ❌ 테스트 실패: {autoCredsTestResults[originalIdx].error || "유효하지 않은 키"}
                      </span>
                    )}
                  </div>
                )}
              </div>
              <div style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
                <button 
                  onClick={() => handleApplyCredential(cred, originalIdx)}
                  disabled={applyLoading[originalIdx]}
                  className="btn btn-save"
                  style={{ padding: "0.25rem 0.6rem", fontSize: "0.7rem", background: "var(--neon-blue)", color: "#0a0c10", fontWeight: 700 }}
                >
                  {applyLoading[originalIdx] ? "연동 중..." : "바로 연동"}
                </button>
                <div className="tooltip-container" style={{ display: "inline-block" }}>
                  <button 
                    onClick={() => handleTestCredential(cred.provider as any, originalIdx)}
                    disabled={testLoading[originalIdx]}
                    className="btn"
                    style={{ 
                      padding: "0.25rem 0.6rem", 
                      fontSize: "0.7rem", 
                      background: "rgba(255,255,255,0.04)", 
                      border: "1px solid rgba(255,255,255,0.08)",
                      color: "hsl(215, 20%, 80%)",
                      borderRadius: "4px",
                      cursor: "pointer",
                      display: "flex",
                      alignItems: "center",
                      gap: "0.25rem"
                    }}
                  >
                    <span>{testLoading[originalIdx] ? "테스트 중..." : "연동 테스트"}</span>
                    <span style={{ fontSize: "0.72rem", opacity: 0.6 }}>ℹ️</span>
                  </button>
                  <div className="tooltip-text">
                    <b>연동 테스트 작동 원리</b>:<br/>
                    입력된 토큰을 백엔드에서 일시 수신하여 각사 API 검증 서버(Anthropic/OpenAI)로 1회성 초소형 쿼리(더미 API 요청)를 전송하고 인증 승인 상태를 실시간 진단합니다.
                  </div>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    );
  };

  return (
    <div className="settings-container" style={{ display: "flex", flexDirection: "column", gap: "2rem" }}>
      {activeSection === "settings-general" ? (
        <>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <h2 className="settings-title" style={{ margin: 0 }}>🖥️ 화면 및 일반 설정 (General Settings)</h2>
          </div>
          
          <div className="settings-card glass" style={{ padding: "1.5rem" }}>
            <h3 style={{ fontSize: "1.1rem", fontWeight: 700, margin: "0 0 1rem 0", color: "var(--neon-blue)", borderBottom: "1px solid rgba(255,255,255,0.08)", paddingBottom: "0.5rem" }}>
              ⚙️ 디렉토리 및 표시 방식 설정
            </h3>
            
            <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
              {/* 로그 감시 경로 */}
              <div className="form-group">
                <div className="form-group-header" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <label style={{ fontWeight: 600, fontSize: "0.85rem" }}>로컬 로그 디렉토리 감시 경로</label>
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
                <p style={{ margin: "0.25rem 0 0.5rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 55%)" }}>AI 에이전트들의 로그 파일을 실시간으로 추적/감시할 로컬 폴더 경로를 지정합니다.</p>
                <div className="input-group">
                  <input
                    type="text"
                    placeholder="예: /Users/username/logs"
                    value={settings.log_dir}
                    onChange={(e) => setSettings(prev => ({ ...prev, log_dir: e.target.value }))}
                    className="settings-input"
                    style={{ fontFamily: "monospace", fontSize: "0.8rem", flex: 1 }}
                  />
                  <button 
                    onClick={() => handleSaveSettings({ log_dir: settings.log_dir })} 
                    className="btn btn-save" 
                    disabled={!settings.log_dir.trim()}
                  >
                    저장
                  </button>
                </div>
              </div>

              {/* 화면 표시 설정 */}
              <div className="form-group" style={{ borderTop: "1px solid rgba(255,255,255,0.04)", paddingTop: "1rem" }}>
                <label style={{ fontWeight: 600, fontSize: "0.85rem", marginBottom: "0.3rem", display: "block" }}>토큰 잔여량 표시 방식</label>
                <p style={{ margin: "0 0 0.75rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 55%)" }}>대시보드 및 트레이 바에서 쿼터 잔여량을 표시할 단위를 선택합니다.</p>
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

              {/* 세션 자동 갱신 주기 설정 */}
              <div className="form-group" style={{ borderTop: "1px solid rgba(255,255,255,0.04)", paddingTop: "1rem" }}>
                <label style={{ fontWeight: 600, fontSize: "0.85rem", marginBottom: "0.3rem", display: "block" }}>세션 자동 갱신 주기</label>
                <p style={{ margin: "0 0 0.75rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 55%)" }}>대시보드·트레이의 세션 사용량을 선택한 주기마다 자동으로 새로고침합니다. (그 외에도 로그 변경 감지 시 즉시 갱신됩니다.)</p>
                <div style={{ display: "flex", gap: "2rem", marginTop: "0.25rem" }}>
                  {[1, 3, 5].map((min) => (
                    <label key={min} style={{ display: "flex", alignItems: "center", gap: "0.5rem", fontSize: "0.85rem", cursor: "pointer", color: "hsl(215, 20%, 85%)" }}>
                      <input
                        type="radio"
                        name="refresh_interval"
                        value={min}
                        checked={settings.refresh_interval === min}
                        onChange={() => handleSaveSettings({ refresh_interval: min })}
                        style={{ cursor: "pointer" }}
                      />
                      {min}분마다
                    </label>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </>
      ) : (
        <>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <h2 className="settings-title" style={{ margin: 0 }}>🔗 연동 및 구독 설정 (Integrations & Subscriptions)</h2>
            <button 
              onClick={handleScanCredentials} 
              disabled={scanLoading}
              className="btn"
              style={{ 
                padding: "0.4rem 1rem", 
                fontSize: "0.8rem", 
                background: "rgba(0, 242, 254, 0.1)", 
                border: "1px solid rgba(0, 242, 254, 0.3)",
                color: "var(--neon-blue)",
                fontWeight: 700
              }}
            >
              {scanLoading ? "🔄 감지 스캔 중..." : "⚡ 로컬 자격 증명 새로고침 스캔"}
            </button>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
            
            {/* 세션 로그 경로 자동 감지 */}
            <div className="settings-card glass" style={{ padding: "1.5rem", borderLeft: "3px solid hsl(35, 100%, 60%)" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.5rem" }}>
                <h4 style={{ margin: 0, fontSize: "1.05rem", color: "hsl(35, 100%, 65%)", fontWeight: 700 }}>📂 세션 로그 경로</h4>
                <button
                  onClick={handleScanLogPaths}
                  disabled={logPathScanLoading}
                  className="btn"
                  style={{ padding: "0.35rem 0.9rem", fontSize: "0.78rem", background: "rgba(255,170,0,0.1)", border: "1px solid rgba(255,170,0,0.3)", color: "hsl(35, 100%, 65%)", fontWeight: 700 }}
                >
                  {logPathScanLoading ? "🔄 스캔 중..." : "⚡ 경로 자동 감지 새로고침"}
                </button>
              </div>
              <p style={{ margin: "0 0 1rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", lineHeight: 1.4 }}>
                각 에이전트(Claude·Codex·Antigravity)의 세션 로그 폴더를 자동 감지하여 실시간 감시합니다. 로그가 비표준 위치에 있으면 직접 경로를 지정할 수 있습니다.
                <br />경로 변경은 즉시 동기화에 반영되며, 실시간 감시(watcher)는 앱 재시작 후 적용됩니다.
              </p>

              <div style={{ display: "flex", flexDirection: "column", gap: "0.85rem" }}>
                {logPaths.length === 0 ? (
                  <span style={{ fontSize: "0.8rem", color: "hsl(215, 20%, 55%)" }}>
                    {logPathScanLoading ? "경로 감지 중..." : "감지된 경로가 없습니다."}
                  </span>
                ) : logPaths.map((lp) => (
                  <div key={lp.agent} style={{ background: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.05)", padding: "0.85rem", borderRadius: "8px" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.5rem" }}>
                      <span style={{ fontSize: "0.85rem", fontWeight: 700, color: "#fff" }}>{lp.label}</span>
                      {lp.exists ? (
                        <span className="status-badge active"><span className="pulse-dot-green"></span>감지됨</span>
                      ) : (
                        <span className="status-badge none">경로 없음</span>
                      )}
                    </div>
                    <div style={{ fontSize: "0.72rem", color: "hsl(215, 20%, 55%)", marginBottom: "0.5rem", fontFamily: "monospace", wordBreak: "break-all" }}>
                      현재: {lp.active_path}{lp.configured_path ? "  (사용자 지정)" : "  (기본 자동감지)"}
                    </div>
                    <div className="input-group" style={{ display: "flex", gap: "0.5rem" }}>
                      <input
                        type="text"
                        placeholder={lp.default_path}
                        value={logPathDrafts[lp.agent] ?? ""}
                        onChange={(e) => setLogPathDrafts(prev => ({ ...prev, [lp.agent]: e.target.value }))}
                        className="settings-input"
                        style={{ flex: 1, fontSize: "0.75rem" }}
                      />
                      <button className="btn btn-save" onClick={() => handleSaveLogPath(lp.agent, logPathDrafts[lp.agent] ?? "")}>저장</button>
                      {lp.configured_path && (
                        <button className="btn btn-delete" onClick={() => handleSaveLogPath(lp.agent, "")}>기본값</button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Claude Code (Anthropic) */}
            <div className="settings-card glass" style={{ padding: "1.5rem", borderLeft: "3px solid var(--neon-blue)" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.5rem" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <h4 style={{ margin: 0, fontSize: "1.05rem", color: "var(--neon-blue)", fontWeight: 700 }}>Claude Code (Anthropic)</h4>
                  <span style={{ 
                    fontSize: "0.65rem", 
                    padding: "0.15rem 0.5rem", 
                    borderRadius: "9999px", 
                    background: "rgba(0, 242, 254, 0.08)", 
                    color: "var(--neon-blue)", 
                    fontWeight: 700,
                    border: "1px solid rgba(0, 242, 254, 0.2)",
                    fontFamily: "'Outfit', sans-serif"
                  }}>
                    {(quotaInfo.find(q => q.provider === "anthropic")?.plan_label || "Claude Pro").split(" (")[0]}
                  </span>
                </div>
                <span style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>Claude.ai Web Session & API 연동</span>
              </div>
              <p style={{ margin: "0 0 1rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", lineHeight: 1.4 }}>
                Claude Code 및 Claude.ai 웹 클라이언트의 토큰 실시간 쿼터 소진율을 갱신하기 위한 크리덴셜 연동입니다.
              </p>

              <div style={{ display: "flex", flexDirection: "column", gap: "1.25rem" }}>
                <div style={{ background: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.04)", padding: "0.85rem", borderRadius: "8px" }}>
                  <span style={{ fontSize: "0.8rem", fontWeight: 700, color: "hsl(35, 100%, 65%)", display: "block", marginBottom: "0.5rem" }}>⚡ Claude 자동 감지 크리덴셜</span>
                  {renderProviderAutoCreds("anthropic")}
                </div>

                <div style={{ borderTop: "1px dashed rgba(255,255,255,0.05)", paddingTop: "1rem" }}>
                  <div className="form-group">
                    <div className="form-group-header" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.3rem" }}>
                      <label style={{ fontSize: "0.8rem", fontWeight: 600 }}>Anthropic API Key / 웹 세션 토큰 수동 설정</label>
                      <div className="status-indicator">
                        {diagnoseLoading.anthropic ? (
                          <span className="status-badge checking">진단 중...</span>
                        ) : keysStatus.anthropic ? (
                          anthropicValid ? (
                            <span className="status-badge active">
                              <span className="pulse-dot-green"></span>
                              연결됨 (Active: {(quotaInfo.find(q => q.provider === "anthropic")?.plan_label || "Claude Pro").split(" (")[0]})
                            </span>
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
                        placeholder={keysStatus.anthropic ? "••••••••••••••••••••••••" : "API 키 (sk-ant-...) 또는 웹 세션 토큰 (sk-ant-sid02-...)"}
                        value={anthropicKey}
                        onChange={(e) => setAnthropicKey(e.target.value)}
                        className="settings-input"
                        style={{ flex: 1 }}
                      />
                      <button onClick={() => handleSaveKey("anthropic")} className="btn btn-save" disabled={!anthropicKey.trim()}>저장</button>
                      {keysStatus.anthropic && (
                        <button onClick={() => handleDeleteKey("anthropic")} className="btn btn-delete">삭제</button>
                      )}
                    </div>
                  </div>
                </div>

                {keysStatus.anthropic && anthropicValid && (
                  <div style={{ 
                    marginTop: "1rem", 
                    padding: "1rem", 
                    background: "rgba(0, 242, 254, 0.03)", 
                    border: "1px solid rgba(0, 242, 254, 0.15)", 
                    borderRadius: "8px",
                    fontSize: "0.8rem",
                    display: "flex",
                    flexDirection: "column",
                    gap: "0.5rem"
                  }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontWeight: 700 }}>
                      <span style={{ color: "var(--neon-blue)" }}>📋 연동된 구독 플랜</span>
                      <span style={{ color: "#fff" }}>{quotaInfo.find(q => q.provider === "anthropic")?.plan_label || "Claude Pro"}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 65%)" }}>
                      <span>기본 제공 한도 (5시간)</span>
                      <span style={{ fontFamily: "monospace" }}>
                        {quotaInfo.find(q => q.provider === "anthropic")?.plan_key === "api" 
                          ? "API Rate Limit" 
                          : `${formatTokens(quotaInfo.find(q => q.provider === "anthropic")?.quota_tokens || 44_000_000)} Tokens`}
                      </span>
                    </div>
                    {quotaInfo.find(q => q.provider === "anthropic")?.weekly_quota_tokens && (
                      <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 65%)" }}>
                        <span>주간 한도 (소진율)</span>
                        <span style={{ fontFamily: "monospace" }}>
                          {formatTokens(quotaInfo.find(q => q.provider === "anthropic")?.weekly_quota_tokens || 0)} Tokens 
                          {" "}({(quotaInfo.find(q => q.provider === "anthropic")?.weekly_usage_pct ?? 0).toFixed(1)}% 소진)
                        </span>
                      </div>
                    )}
                    {quotaInfo.find(q => q.provider === "anthropic")?.window_reset_at && (
                      <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 55%)", borderTop: "1px dashed rgba(255,255,255,0.05)", paddingTop: "0.4rem", marginTop: "0.2rem" }}>
                        <span>쿼터 롤링 리셋 시각</span>
                        <span style={{ fontFamily: "monospace" }}>
                          {formatResetTime(quotaInfo.find(q => q.provider === "anthropic")?.window_reset_at)}
                        </span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>

            {/* OpenAI Codex */}
            <div className="settings-card glass" style={{ padding: "1.5rem", borderLeft: "3px solid var(--neon-purple)" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.5rem" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <h4 style={{ margin: 0, fontSize: "1.05rem", color: "var(--neon-purple)", fontWeight: 700 }}>Codex (OpenAI)</h4>
                  <span style={{ 
                    fontSize: "0.65rem", 
                    padding: "0.15rem 0.5rem", 
                    borderRadius: "9999px", 
                    background: "rgba(139, 92, 246, 0.08)", 
                    color: "var(--neon-purple)", 
                    fontWeight: 700,
                    border: "1px solid rgba(139, 92, 246, 0.2)",
                    fontFamily: "'Outfit', sans-serif"
                  }}>
                    {(quotaInfo.find(q => q.provider === "openai")?.plan_label || "OpenAI Tier 1").split(" (")[0]}
                  </span>
                </div>
                <span style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>OpenAI API & Usage 연동</span>
              </div>
              <p style={{ margin: "0 0 1rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", lineHeight: 1.4 }}>
                OpenAI 에이전트(Codex 등) 연동을 통해 토큰 소비량 및 비용 정보를 동기화하고 잔여 한도를 추적합니다.
              </p>

              <div style={{ display: "flex", flexDirection: "column", gap: "1.25rem" }}>
                <div style={{ background: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.04)", padding: "0.85rem", borderRadius: "8px" }}>
                  <span style={{ fontSize: "0.8rem", fontWeight: 700, color: "var(--neon-purple)", display: "block", marginBottom: "0.5rem" }}>⚡ OpenAI 자동 감지 크리덴셜</span>
                  {renderProviderAutoCreds("openai")}
                </div>

                <div style={{ borderTop: "1px dashed rgba(255,255,255,0.05)", paddingTop: "1rem" }}>
                  <div className="form-group">
                    <div className="form-group-header" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.3rem" }}>
                      <label style={{ fontSize: "0.8rem", fontWeight: 600 }}>OpenAI API Key 수동 설정</label>
                      <div className="status-indicator">
                        {diagnoseLoading.openai ? (
                          <span className="status-badge checking">진단 중...</span>
                        ) : keysStatus.openai ? (
                          openaiValid ? (
                            <span className="status-badge active">
                              <span className="pulse-dot-green"></span>
                              연결됨 (Active: {(quotaInfo.find(q => q.provider === "openai")?.plan_label || "OpenAI Tier 1").split(" (")[0]})
                            </span>
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
                        placeholder={keysStatus.openai ? "••••••••••••••••••••••••" : "OpenAI API 키 수동 입력 (sk-...)"}
                        value={openaiKey}
                        onChange={(e) => setOpenAIKey(e.target.value)}
                        className="settings-input"
                        style={{ flex: 1 }}
                      />
                      <button onClick={() => handleSaveKey("openai")} className="btn btn-save" disabled={!openaiKey.trim()}>저장</button>
                      {keysStatus.openai && (
                        <button onClick={() => handleDeleteKey("openai")} className="btn btn-delete">삭제</button>
                      )}
                    </div>
                  </div>
                </div>

                {keysStatus.openai && openaiValid && (
                  <div style={{ 
                    marginTop: "1rem", 
                    padding: "1rem", 
                    background: "rgba(139, 92, 246, 0.03)", 
                    border: "1px solid rgba(139, 92, 246, 0.15)", 
                    borderRadius: "8px",
                    fontSize: "0.8rem",
                    display: "flex",
                    flexDirection: "column",
                    gap: "0.5rem"
                  }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontWeight: 700 }}>
                      <span style={{ color: "var(--neon-purple)" }}>📋 연동된 사용 Tier</span>
                      <span style={{ color: "#fff" }}>{quotaInfo.find(q => q.provider === "openai")?.plan_label || "OpenAI Tier 1"}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 65%)" }}>
                      <span>롤링 소비 한도 (5시간)</span>
                      <span style={{ fontFamily: "monospace" }}>
                        {formatTokens(quotaInfo.find(q => q.provider === "openai")?.quota_tokens || 100_000_000)} Tokens
                      </span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 65%)" }}>
                      <span>실시간 롤링 소진율</span>
                      <span style={{ fontFamily: "monospace" }}>
                        {formatTokens(quotaInfo.find(q => q.provider === "openai")?.remaining_tokens || 0)} Tokens 남음
                        {" "}({(quotaInfo.find(q => q.provider === "openai")?.usage_pct ?? 0).toFixed(1)}% 소진)
                      </span>
                    </div>
                    {quotaInfo.find(q => q.provider === "openai")?.window_reset_at && (
                      <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.75rem", color: "hsl(215, 20%, 55%)", borderTop: "1px dashed rgba(255,255,255,0.05)", paddingTop: "0.4rem", marginTop: "0.2rem" }}>
                        <span>롤링 쿼터 리셋 시각</span>
                        <span style={{ fontFamily: "monospace" }}>
                          {formatResetTime(quotaInfo.find(q => q.provider === "openai")?.window_reset_at)}
                        </span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>

            {/* Antigravity */}
            <div className="settings-card glass" style={{ padding: "1.5rem", borderLeft: "3px solid var(--neon-green)" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.5rem" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  <h4 style={{ margin: 0, fontSize: "1.05rem", color: "var(--neon-green)", fontWeight: 700 }}>Antigravity (Local)</h4>
                  <span style={{ 
                    fontSize: "0.65rem", 
                    padding: "0.15rem 0.5rem", 
                    borderRadius: "9999px", 
                    background: "rgba(16, 185, 129, 0.08)", 
                    color: "var(--neon-green)", 
                    fontWeight: 700,
                    border: "1px solid rgba(16, 185, 129, 0.2)",
                    fontFamily: "'Outfit', sans-serif"
                  }}>
                    로컬 한도
                  </span>
                </div>
                <span style={{ fontSize: "0.75rem", color: "hsl(215, 20%, 50%)" }}>로컬 전용 에이전트 연동</span>
              </div>
              <p style={{ margin: "0 0 1rem 0", fontSize: "0.75rem", color: "hsl(215, 20%, 60%)", lineHeight: 1.4 }}>
                로컬 환경에서 구동되는 Antigravity 에이전트의 로그 감시 상태 및 로컬 소비 한도를 추적합니다.
              </p>

              <div style={{ display: "flex", flexDirection: "column", gap: "1.25rem" }}>
                <div style={{ background: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.04)", padding: "0.85rem", borderRadius: "8px", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <div>
                    <span style={{ fontSize: "0.8rem", fontWeight: 700, color: "var(--neon-green)", display: "block", marginBottom: "0.15rem" }}>⚡ Antigravity 연동 상태</span>
                    <span style={{ fontSize: "0.7rem", color: "hsl(215, 20%, 50%)" }}>기본 OS globalStorage 파일시스템 자동 스캔 대기 중</span>
                  </div>
                  <span className="status-badge active" style={{ background: "rgba(16, 185, 129, 0.1)", border: "1px solid rgba(16, 185, 129, 0.2)", color: "hsl(150, 100%, 45%)" }}>
                    <span className="pulse-dot-green"></span>스캔 활성화
                  </span>
                </div>

                <div style={{ borderTop: "1px dashed rgba(255,255,255,0.05)", paddingTop: "1rem", display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1rem" }}>
                  <div className="form-group">
                    <label style={{ fontSize: "0.8rem", fontWeight: 600, marginBottom: "0.3rem", display: "block" }}>플랫폼 타입</label>
                    <input
                      type="text"
                      value="Local SQLite 감시 (VS Code User)"
                      disabled
                      className="settings-input"
                      style={{ width: "100%", opacity: 0.6, cursor: "not-allowed" }}
                    />
                  </div>
                  <div className="form-group">
                    <label style={{ fontSize: "0.8rem", fontWeight: 600, marginBottom: "0.3rem", display: "block" }}>일간(24h) 소비 제한 한도 (Tokens)</label>
                    <input
                      type="number"
                      value={settings.token_limit_antigravity}
                      onChange={(e) => setSettings(prev => ({ ...prev, token_limit_antigravity: Number(e.target.value) }))}
                      onBlur={() => handleSaveSettings({ token_limit_antigravity: settings.token_limit_antigravity })}
                      className="settings-input"
                      style={{ width: "100%" }}
                    />
                  </div>
                </div>
              </div>
            </div>

          </div>
        </>
      )}
    </div>
  );
}
export default SettingsView;
