import { useState, useEffect, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DetectedCredential } from "../types";
import { useSubscriptionQuota } from "../hooks/queries/useQuotaQuery";
import { useSettings } from "../hooks/queries/useSettingsQuery";
import {
  useApiKeysStatus,
  useLocalCredentials,
  useDetectedLogPaths,
} from "../hooks/queries/useSettingsDiagnostics";
import {
  useSaveSettings,
  useSaveApiKey,
  useDeleteApiKey,
  useAutoApplyCredential,
} from "../hooks/mutations/useSettingsMutations";
import { formatTokens, formatResetTime } from "../utils/formatters";
import { cn } from "@/lib/utils";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Monitor,
  Plug,
  FolderOpen,
  RefreshCw,
  Info,
  Loader2,
  CheckCircle2,
  XCircle,
} from "lucide-react";

interface SettingsViewProps {
  activeSection: string;
}

/** 작은 상태 배지 */
type StatusKind = "checking" | "active" | "inactive" | "none";
function StatusBadge({ kind, children }: { kind: StatusKind; children: ReactNode }) {
  if (kind === "active") {
    return (
      <Badge className="gap-1 border-success/30 bg-success/10 text-success">
        <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-success" />
        {children}
      </Badge>
    );
  }
  if (kind === "inactive") {
    return (
      <Badge variant="destructive" className="gap-1">
        <span className="h-1.5 w-1.5 rounded-full bg-destructive-foreground/80" />
        {children}
      </Badge>
    );
  }
  if (kind === "checking") {
    return (
      <Badge variant="secondary" className="gap-1">
        <Loader2 className="h-3 w-3 animate-spin" />
        {children}
      </Badge>
    );
  }
  return (
    <Badge variant="outline" className="text-muted-foreground">
      {children}
    </Badge>
  );
}

/**
 * 대시보드 내부의 설정(Settings) 및 연동(Integrations) 탭 뷰 컴포넌트
 *
 * 로그 디렉토리 감시 경로 설정, 수동 API Key 관리 및 로컬 키체인/설정 파일 자동 크리덴셜 연동과 검증 등을 처리합니다.
 */
export function SettingsView({ activeSection }: SettingsViewProps) {
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
    refresh_interval: 3,
  });
  // 읽기 데이터는 React Query (quota 는 대시보드와 캐시 공유)
  const settingsQ = useSettings();
  const keysStatusQ = useApiKeysStatus();
  const quotaQ = useSubscriptionQuota();
  const credsQ = useLocalCredentials();
  const logPathsQ = useDetectedLogPaths();

  const keysStatus = {
    anthropic: keysStatusQ.data?.anthropic ?? false,
    openai: keysStatusQ.data?.openai ?? false,
  };
  const quotaInfo = quotaQ.data ?? [];
  const localCreds = credsQ.data ?? [];
  const logPaths = logPathsQ.data ?? [];
  const scanLoading = credsQ.isFetching;
  const logPathScanLoading = logPathsQ.isFetching;

  // 저장/연동 mutation
  const saveSettingsMutation = useSaveSettings();
  const saveApiKeyMutation = useSaveApiKey();
  const deleteApiKeyMutation = useDeleteApiKey();
  const autoApplyMutation = useAutoApplyCredential();

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

  const [logPathDrafts, setLogPathDrafts] = useState<Record<string, string>>({});
  const [applyLoading, setApplyLoading] = useState<Record<number, boolean>>({});
  const [testLoading, setTestLoading] = useState<Record<number, boolean>>({});
  const [autoCredsTestResults, setAutoCredsTestResults] = useState<
    Record<number, { checked: boolean; valid: boolean | null; error?: string }>
  >({});

  // 로컬 자격 증명 재스캔 (쿼리 refetch — 로딩은 credsQ.isFetching)
  const handleScanCredentials = () => {
    credsQ.refetch();
  };

  // 세션 로그 경로 재감지 (입력 초안은 logPathsQ.data 시드 effect 가 동기화)
  const handleScanLogPaths = () => {
    logPathsQ.refetch();
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
    setApplyLoading((prev) => ({ ...prev, [index]: true }));
    try {
      // 토스트/캐시 무효화는 mutation onSuccess/onError 에서 처리
      await autoApplyMutation.mutateAsync({ provider: cred.provider, rawValue: cred.raw_value });
    } catch {
      /* onError 토스트 처리 */
    } finally {
      setApplyLoading((prev) => ({ ...prev, [index]: false }));
    }
  };

  const handleTestCredential = async (provider: "anthropic" | "openai", index: number) => {
    const cred = localCreds[index];
    if (!cred) return;

    setTestLoading((prev) => ({ ...prev, [index]: true }));
    setAutoCredsTestResults((prev) => ({
      ...prev,
      [index]: { checked: true, valid: null }, // 검사 중
    }));

    try {
      const isValid = await invoke<boolean>("validate_api_key_value", {
        provider,
        apiKey: cred.raw_value,
        api_key: cred.raw_value,
      });
      setAutoCredsTestResults((prev) => ({
        ...prev,
        [index]: { checked: true, valid: isValid },
      }));
    } catch (e: any) {
      setAutoCredsTestResults((prev) => ({
        ...prev,
        [index]: { checked: true, valid: false, error: e.toString() },
      }));
    } finally {
      setTestLoading((prev) => ({ ...prev, [index]: false }));
    }
  };

  // 설정 폼 draft 를 서버 설정으로 시드 (최초 로드 + 저장 후 무효화→재조회 시 재시드)
  useEffect(() => {
    const s = settingsQ.data;
    if (!s) return;
    setSettings({
      log_dir: s.log_dir ?? "",
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
      refresh_interval: s.refresh_interval ?? 3,
    });
  }, [settingsQ.data]);

  // 로그 경로 입력 초안을 감지 결과로 시드
  useEffect(() => {
    const paths = logPathsQ.data;
    if (!paths) return;
    const drafts: Record<string, string> = {};
    paths.forEach((p) => {
      drafts[p.agent] = p.configured_path;
    });
    setLogPathDrafts(drafts);
  }, [logPathsQ.data]);

  // 저장된 키/경로 유효성 자동 진단 (로드 시 + 상태 변경 시)
  useEffect(() => {
    if (keysStatusQ.data?.anthropic) diagnoseKey("anthropic");
    if (keysStatusQ.data?.openai) diagnoseKey("openai");
  }, [keysStatusQ.data]);

  useEffect(() => {
    if (settingsQ.data?.log_dir) diagnosePath(settingsQ.data.log_dir);
  }, [settingsQ.data?.log_dir]);

  const diagnoseKey = async (provider: "anthropic" | "openai") => {
    setDiagnoseLoading((prev) => ({ ...prev, [provider]: true }));
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
      setDiagnoseLoading((prev) => ({ ...prev, [provider]: false }));
    }
  };

  const diagnosePath = async (path: string) => {
    setDiagnoseLoading((prev) => ({ ...prev, path: true }));
    try {
      const isValid = await invoke<boolean>("validate_local_path", { path });
      setPathValid(isValid);
    } catch (e) {
      console.error("로컬 경로 진단 실패:", e);
      setPathValid(false);
    } finally {
      setDiagnoseLoading((prev) => ({ ...prev, path: false }));
    }
  };

  const handleSaveKey = async (provider: "anthropic" | "openai") => {
    const key = provider === "anthropic" ? anthropicKey : openaiKey;
    if (!key.trim()) return;
    try {
      await saveApiKeyMutation.mutateAsync({ provider, key });
      if (provider === "anthropic") setAnthropicKey("");
      else setOpenAIKey("");
    } catch {
      /* onError 토스트 처리 */
    }
  };

  const handleDeleteKey = async (provider: "anthropic" | "openai") => {
    try {
      await deleteApiKeyMutation.mutateAsync({ provider });
      if (provider === "anthropic") setAnthropicValid(null);
      else setOpenAIValid(null);
    } catch {
      /* onError 토스트 처리 */
    }
  };

  const handleSaveSettings = async (updates: Partial<typeof settings>) => {
    const newSettings = { ...settings, ...updates };
    setSettings(newSettings); // 토글/입력 즉시 반영 (저장 후 재조회로 재시드)
    try {
      await saveSettingsMutation.mutateAsync({
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
        refreshInterval: Number(newSettings.refresh_interval),
      });
    } catch {
      /* onError 토스트 처리 */
    }
  };

  // Provider별 자동 감지 크리덴셜 렌더링 헬퍼
  const renderProviderAutoCreds = (provider: "anthropic" | "openai") => {
    const filtered = localCreds.filter((c) => c.provider === provider);
    if (filtered.length === 0) {
      return (
        <div className="rounded-md border border-dashed border-border bg-muted/20 p-3 text-xs text-muted-foreground">
          {scanLoading
            ? "시스템 분석 중..."
            : `자동 감지된 로컬 ${provider === "anthropic" ? "Claude" : "OpenAI"} 인증 정보가 없습니다.`}
        </div>
      );
    }

    return (
      <div className="flex flex-col gap-2">
        {filtered.map((cred, idx) => {
          const originalIdx = localCreds.findIndex(
            (c) => c.raw_value === cred.raw_value && c.provider === cred.provider
          );

          let tooltipInfo = "로컬 시스템 설정 또는 환경 변수에서 감지된 에이전트 인증 정보입니다.";
          if (cred.source === "Keychain") {
            if (cred.provider === "anthropic") {
              tooltipInfo =
                "Claude Code CLI가 macOS 키체인에 안전하게 저장하여 사용하는 OAuth Access Token 정보입니다.";
            } else {
              tooltipInfo =
                "OpenAI 에이전트 연동용으로 macOS 키체인에 안전하게 저장되어 있는 API Key 정보입니다.";
            }
          } else if (cred.description.includes("세션 키") || cred.description.includes("fetch-claude-usage")) {
            tooltipInfo =
              "로컬 세션 감지 스크립트를 통해 확인된 Claude.ai 웹 브라우저 로그인 세션(sk-ant-sid) 정보입니다.";
          } else if (cred.source === "ConfigFile") {
            tooltipInfo = "로컬 에이전트 설정 파일(.config/claude 등)에서 감지된 인증 토큰 정보입니다.";
          }

          const testResult = autoCredsTestResults[originalIdx];

          return (
            <div
              key={idx}
              className="flex items-center justify-between gap-3 rounded-lg border border-border bg-card p-3"
            >
              <div className="flex min-w-0 flex-col gap-0.5">
                <div className="flex items-center gap-1.5">
                  <span className="text-sm font-medium">{cred.description}</span>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="cursor-help text-muted-foreground">
                        <Info className="h-3.5 w-3.5" />
                      </span>
                    </TooltipTrigger>
                    <TooltipContent className="max-w-[270px]">{tooltipInfo}</TooltipContent>
                  </Tooltip>
                </div>
                <span className="truncate font-mono text-[11px] text-muted-foreground">
                  감지 토큰: {cred.value} (출처: {cred.source})
                </span>
                {testResult?.checked && (
                  <div className="mt-0.5 text-[11px]">
                    {testResult.valid === null ? (
                      <span className="text-warning">테스트 검증 중...</span>
                    ) : testResult.valid === true ? (
                      <span className="flex items-center gap-1 text-success">
                        <CheckCircle2 className="h-3 w-3" /> 테스트 성공 (유효한 크리덴셜)
                      </span>
                    ) : (
                      <span className="flex items-center gap-1 text-destructive">
                        <XCircle className="h-3 w-3" /> 테스트 실패: {testResult.error || "유효하지 않은 키"}
                      </span>
                    )}
                  </div>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                <Button
                  size="sm"
                  onClick={() => handleApplyCredential(cred, originalIdx)}
                  disabled={applyLoading[originalIdx]}
                >
                  {applyLoading[originalIdx] ? "연동 중..." : "바로 연동"}
                </Button>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleTestCredential(cred.provider as any, originalIdx)}
                      disabled={testLoading[originalIdx]}
                    >
                      {testLoading[originalIdx] ? "테스트 중..." : "연동 테스트"}
                      <Info className="h-3 w-3 opacity-60" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent className="max-w-[300px]">
                    입력된 토큰을 백엔드에서 일시 수신하여 각사 API 검증 서버(Anthropic/OpenAI)로 1회성 초소형
                    쿼리(더미 API 요청)를 전송하고 인증 승인 상태를 실시간 진단합니다.
                  </TooltipContent>
                </Tooltip>
              </div>
            </div>
          );
        })}
      </div>
    );
  };

  return (
    <TooltipProvider delayDuration={200}>
      <div className="mx-auto flex max-w-4xl flex-col gap-8">
        {activeSection === "settings-general" ? (
          <>
            <h2 className="flex items-center gap-2 text-2xl font-semibold tracking-tight">
              <Monitor className="h-6 w-6 text-muted-foreground" />
              화면 및 일반 설정
            </h2>

            <Card className="gap-0 p-6">
              <h3 className="mb-4 border-b border-border pb-2 text-base font-semibold">
                디렉토리 및 표시 방식 설정
              </h3>

              <div className="flex flex-col gap-6">
                {/* 로그 감시 경로 */}
                <div className="flex flex-col gap-2">
                  <div className="flex items-center justify-between">
                    <Label className="text-sm font-semibold">로컬 로그 디렉토리 감시 경로</Label>
                    {diagnoseLoading.path ? (
                      <StatusBadge kind="checking">검사 중...</StatusBadge>
                    ) : settings.log_dir ? (
                      pathValid ? (
                        <StatusBadge kind="active">경로 유효함</StatusBadge>
                      ) : (
                        <StatusBadge kind="inactive">경로 오류 (존재하지 않음)</StatusBadge>
                      )
                    ) : (
                      <StatusBadge kind="none">설정되지 않음</StatusBadge>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    AI 에이전트들의 로그 파일을 실시간으로 추적/감시할 로컬 폴더 경로를 지정합니다.
                  </p>
                  <div className="flex gap-2">
                    <Input
                      type="text"
                      placeholder="예: /Users/username/logs"
                      value={settings.log_dir}
                      onChange={(e) => setSettings((prev) => ({ ...prev, log_dir: e.target.value }))}
                      className="font-mono text-sm"
                    />
                    <Button
                      onClick={() => handleSaveSettings({ log_dir: settings.log_dir })}
                      disabled={!settings.log_dir.trim()}
                    >
                      저장
                    </Button>
                  </div>
                </div>

                {/* 화면 표시 설정 */}
                <div className="flex flex-col gap-2 border-t border-border pt-4">
                  <Label className="text-sm font-semibold">토큰 잔여량 표시 방식</Label>
                  <p className="text-xs text-muted-foreground">
                    대시보드 및 트레이 바에서 쿼터 잔여량을 표시할 단위를 선택합니다.
                  </p>
                  <div className="mt-1 inline-flex w-fit items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
                    {[
                      { v: "tokens", label: "토큰 단위" },
                      { v: "percentage", label: "백분율 (%)" },
                    ].map((opt) => (
                      <button
                        key={opt.v}
                        onClick={() => handleSaveSettings({ token_display_mode: opt.v })}
                        className={cn(
                          "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                          settings.token_display_mode === opt.v
                            ? "bg-background text-foreground shadow-sm"
                            : "text-muted-foreground hover:text-foreground"
                        )}
                      >
                        {opt.label}
                      </button>
                    ))}
                  </div>
                </div>

                {/* 세션 자동 갱신 주기 설정 */}
                <div className="flex flex-col gap-2 border-t border-border pt-4">
                  <Label className="text-sm font-semibold">세션 자동 갱신 주기</Label>
                  <p className="text-xs text-muted-foreground">
                    대시보드·트레이의 세션 사용량을 선택한 주기마다 자동으로 새로고침합니다. (그 외에도 로그 변경
                    감지 시 즉시 갱신됩니다.)
                  </p>
                  <div className="mt-1 inline-flex w-fit items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
                    {[1, 3, 5].map((min) => (
                      <button
                        key={min}
                        onClick={() => handleSaveSettings({ refresh_interval: min })}
                        className={cn(
                          "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                          settings.refresh_interval === min
                            ? "bg-background text-foreground shadow-sm"
                            : "text-muted-foreground hover:text-foreground"
                        )}
                      >
                        {min}분마다
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            </Card>
          </>
        ) : (
          <>
            <div className="flex items-center justify-between gap-4">
              <h2 className="flex items-center gap-2 text-2xl font-semibold tracking-tight">
                <Plug className="h-6 w-6 text-muted-foreground" />
                연동 및 구독 설정
              </h2>
              <Button variant="outline" size="sm" onClick={handleScanCredentials} disabled={scanLoading}>
                {scanLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
                {scanLoading ? "감지 스캔 중..." : "로컬 자격 증명 새로고침"}
              </Button>
            </div>

            <div className="flex flex-col gap-6">
              {/* 세션 로그 경로 자동 감지 */}
              <Card className="gap-0 border-l-2 border-l-warning p-6">
                <div className="mb-2 flex items-center justify-between">
                  <h4 className="flex items-center gap-1.5 text-base font-semibold text-warning">
                    <FolderOpen className="h-4 w-4" /> 세션 로그 경로
                  </h4>
                  <Button variant="outline" size="sm" onClick={handleScanLogPaths} disabled={logPathScanLoading}>
                    {logPathScanLoading ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <RefreshCw className="h-4 w-4" />
                    )}
                    {logPathScanLoading ? "스캔 중..." : "경로 자동 감지"}
                  </Button>
                </div>
                <p className="mb-4 text-xs leading-relaxed text-muted-foreground">
                  각 에이전트(Claude·Codex·Antigravity)의 세션 로그 폴더를 자동 감지하여 실시간 감시합니다. 로그가
                  비표준 위치에 있으면 직접 경로를 지정할 수 있습니다.
                  <br />
                  경로 변경은 즉시 동기화에 반영되며, 실시간 감시(watcher)는 앱 재시작 후 적용됩니다.
                </p>

                <div className="flex flex-col gap-3">
                  {logPaths.length === 0 ? (
                    <span className="text-sm text-muted-foreground">
                      {logPathScanLoading ? "경로 감지 중..." : "감지된 경로가 없습니다."}
                    </span>
                  ) : (
                    logPaths.map((lp) => (
                      <div key={lp.agent} className="rounded-lg border border-border bg-muted/20 p-3">
                        <div className="mb-2 flex items-center justify-between">
                          <span className="text-sm font-semibold">{lp.label}</span>
                          {lp.exists ? (
                            <StatusBadge kind="active">감지됨</StatusBadge>
                          ) : (
                            <StatusBadge kind="none">경로 없음</StatusBadge>
                          )}
                        </div>
                        <div className="mb-2 break-all font-mono text-[11px] text-muted-foreground">
                          현재: {lp.active_path}
                          {lp.configured_path ? "  (사용자 지정)" : "  (기본 자동감지)"}
                        </div>
                        <div className="flex gap-2">
                          <Input
                            type="text"
                            placeholder={lp.default_path}
                            value={logPathDrafts[lp.agent] ?? ""}
                            onChange={(e) =>
                              setLogPathDrafts((prev) => ({ ...prev, [lp.agent]: e.target.value }))
                            }
                            className="text-xs"
                          />
                          <Button size="sm" onClick={() => handleSaveLogPath(lp.agent, logPathDrafts[lp.agent] ?? "")}>
                            저장
                          </Button>
                          {lp.configured_path && (
                            <Button size="sm" variant="outline" onClick={() => handleSaveLogPath(lp.agent, "")}>
                              기본값
                            </Button>
                          )}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </Card>

              {/* Claude Code (Anthropic) */}
              <Card className="gap-0 border-l-2 border-l-agent-claude p-6">
                <div className="mb-2 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <h4 className="text-base font-semibold text-agent-claude">Claude Code (Anthropic)</h4>
                    <Badge className="border-agent-claude/30 bg-agent-claude/10 text-agent-claude">
                      {(quotaInfo.find((q) => q.provider === "anthropic")?.plan_label || "Claude Pro").split(" (")[0]}
                    </Badge>
                  </div>
                  <span className="text-xs text-muted-foreground">Claude.ai Web Session &amp; API 연동</span>
                </div>
                <p className="mb-4 text-xs leading-relaxed text-muted-foreground">
                  Claude Code 및 Claude.ai 웹 클라이언트의 토큰 실시간 쿼터 소진율을 갱신하기 위한 크리덴셜
                  연동입니다.
                </p>

                <div className="flex flex-col gap-5">
                  <div className="rounded-lg border border-border bg-muted/20 p-3">
                    <span className="mb-2 block text-sm font-semibold text-agent-claude">
                      Claude 자동 감지 크리덴셜
                    </span>
                    {renderProviderAutoCreds("anthropic")}
                  </div>

                  <div className="border-t border-dashed border-border pt-4">
                    <div className="mb-1.5 flex items-center justify-between">
                      <Label className="text-sm font-semibold">
                        Anthropic API Key / 웹 세션 토큰 수동 설정
                      </Label>
                      {diagnoseLoading.anthropic ? (
                        <StatusBadge kind="checking">진단 중...</StatusBadge>
                      ) : keysStatus.anthropic ? (
                        anthropicValid ? (
                          <StatusBadge kind="active">
                            연결됨 (
                            {(quotaInfo.find((q) => q.provider === "anthropic")?.plan_label || "Claude Pro").split(
                              " ("
                            )[0]}
                            )
                          </StatusBadge>
                        ) : (
                          <StatusBadge kind="inactive">인증 실패 (Invalid)</StatusBadge>
                        )
                      ) : (
                        <StatusBadge kind="none">설정되지 않음</StatusBadge>
                      )}
                    </div>
                    <div className="flex gap-2">
                      <Input
                        type="password"
                        placeholder={
                          keysStatus.anthropic
                            ? "••••••••••••••••••••••••"
                            : "API 키 (sk-ant-...) 또는 웹 세션 토큰 (sk-ant-sid02-...)"
                        }
                        value={anthropicKey}
                        onChange={(e) => setAnthropicKey(e.target.value)}
                      />
                      <Button onClick={() => handleSaveKey("anthropic")} disabled={!anthropicKey.trim()}>
                        저장
                      </Button>
                      {keysStatus.anthropic && (
                        <Button variant="destructive" onClick={() => handleDeleteKey("anthropic")}>
                          삭제
                        </Button>
                      )}
                    </div>
                  </div>

                  {keysStatus.anthropic && anthropicValid && (
                    <div className="flex flex-col gap-2 rounded-lg border border-agent-claude/15 bg-agent-claude/5 p-4 text-sm">
                      <div className="flex justify-between font-semibold">
                        <span className="text-agent-claude">연동된 구독 플랜</span>
                        <span>{quotaInfo.find((q) => q.provider === "anthropic")?.plan_label || "Claude Pro"}</span>
                      </div>
                      <div className="flex justify-between text-xs text-muted-foreground">
                        <span>기본 제공 한도 (5시간)</span>
                        <span className="font-mono">
                          {quotaInfo.find((q) => q.provider === "anthropic")?.plan_key === "api"
                            ? "API Rate Limit"
                            : `${formatTokens(quotaInfo.find((q) => q.provider === "anthropic")?.quota_tokens || 44_000_000)} Tokens`}
                        </span>
                      </div>
                      {quotaInfo.find((q) => q.provider === "anthropic")?.weekly_quota_tokens && (
                        <div className="flex justify-between text-xs text-muted-foreground">
                          <span>주간 한도 (소진율)</span>
                          <span className="font-mono">
                            {formatTokens(quotaInfo.find((q) => q.provider === "anthropic")?.weekly_quota_tokens || 0)}{" "}
                            Tokens (
                            {(quotaInfo.find((q) => q.provider === "anthropic")?.weekly_usage_pct ?? 0).toFixed(1)}%
                            소진)
                          </span>
                        </div>
                      )}
                      {quotaInfo.find((q) => q.provider === "anthropic")?.window_reset_at && (
                        <div className="flex justify-between border-t border-dashed border-border pt-2 text-xs text-muted-foreground">
                          <span>쿼터 롤링 리셋 시각</span>
                          <span className="font-mono">
                            {formatResetTime(quotaInfo.find((q) => q.provider === "anthropic")?.window_reset_at)}
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </Card>

              {/* OpenAI Codex */}
              <Card className="gap-0 border-l-2 border-l-agent-codex p-6">
                <div className="mb-2 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <h4 className="text-base font-semibold text-agent-codex">Codex (OpenAI)</h4>
                    <Badge className="border-agent-codex/30 bg-agent-codex/10 text-agent-codex">
                      {(quotaInfo.find((q) => q.provider === "openai")?.plan_label || "OpenAI Tier 1").split(" (")[0]}
                    </Badge>
                  </div>
                  <span className="text-xs text-muted-foreground">OpenAI API &amp; Usage 연동</span>
                </div>
                <p className="mb-4 text-xs leading-relaxed text-muted-foreground">
                  OpenAI 에이전트(Codex 등) 연동을 통해 토큰 소비량 및 비용 정보를 동기화하고 잔여 한도를
                  추적합니다.
                </p>

                <div className="flex flex-col gap-5">
                  <div className="rounded-lg border border-border bg-muted/20 p-3">
                    <span className="mb-2 block text-sm font-semibold text-agent-codex">
                      OpenAI 자동 감지 크리덴셜
                    </span>
                    {renderProviderAutoCreds("openai")}
                  </div>

                  <div className="border-t border-dashed border-border pt-4">
                    <div className="mb-1.5 flex items-center justify-between">
                      <Label className="text-sm font-semibold">OpenAI API Key 수동 설정</Label>
                      {diagnoseLoading.openai ? (
                        <StatusBadge kind="checking">진단 중...</StatusBadge>
                      ) : keysStatus.openai ? (
                        openaiValid ? (
                          <StatusBadge kind="active">
                            연결됨 (
                            {(quotaInfo.find((q) => q.provider === "openai")?.plan_label || "OpenAI Tier 1").split(
                              " ("
                            )[0]}
                            )
                          </StatusBadge>
                        ) : (
                          <StatusBadge kind="inactive">인증 실패 (Invalid)</StatusBadge>
                        )
                      ) : (
                        <StatusBadge kind="none">설정되지 않음</StatusBadge>
                      )}
                    </div>
                    <div className="flex gap-2">
                      <Input
                        type="password"
                        placeholder={
                          keysStatus.openai ? "••••••••••••••••••••••••" : "Admin API 키 입력 (sk-admin-...)"
                        }
                        value={openaiKey}
                        onChange={(e) => setOpenAIKey(e.target.value)}
                      />
                      <Button onClick={() => handleSaveKey("openai")} disabled={!openaiKey.trim()}>
                        저장
                      </Button>
                      {keysStatus.openai && (
                        <Button variant="destructive" onClick={() => handleDeleteKey("openai")}>
                          삭제
                        </Button>
                      )}
                    </div>
                    <p className="mt-1.5 text-xs text-muted-foreground">
                      실시간 사용량 조회에는 <span className="font-medium">Admin API 키(sk-admin-…)</span>가
                      필요합니다. platform.openai.com → Organization → Admin keys 에서 발급하세요. 일반 키나
                      미설정 시 로컬 DB 집계로 표시됩니다.
                    </p>
                  </div>

                  {keysStatus.openai && openaiValid && (
                    <div className="flex flex-col gap-2 rounded-lg border border-agent-codex/15 bg-agent-codex/5 p-4 text-sm">
                      <div className="flex justify-between font-semibold">
                        <span className="text-agent-codex">연동된 사용 Tier</span>
                        <span>{quotaInfo.find((q) => q.provider === "openai")?.plan_label || "OpenAI Tier 1"}</span>
                      </div>
                      <div className="flex justify-between text-xs text-muted-foreground">
                        <span>롤링 소비 한도 (5시간)</span>
                        <span className="font-mono">
                          {formatTokens(quotaInfo.find((q) => q.provider === "openai")?.quota_tokens || 100_000_000)}{" "}
                          Tokens
                        </span>
                      </div>
                      <div className="flex justify-between text-xs text-muted-foreground">
                        <span>실시간 롤링 소진율</span>
                        <span className="font-mono">
                          {formatTokens(quotaInfo.find((q) => q.provider === "openai")?.remaining_tokens || 0)} Tokens
                          남음 ({(quotaInfo.find((q) => q.provider === "openai")?.usage_pct ?? 0).toFixed(1)}% 소진)
                        </span>
                      </div>
                      {quotaInfo.find((q) => q.provider === "openai")?.window_reset_at && (
                        <div className="flex justify-between border-t border-dashed border-border pt-2 text-xs text-muted-foreground">
                          <span>롤링 쿼터 리셋 시각</span>
                          <span className="font-mono">
                            {formatResetTime(quotaInfo.find((q) => q.provider === "openai")?.window_reset_at)}
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </Card>

              {/* Antigravity */}
              <Card className="gap-0 border-l-2 border-l-agent-antigravity p-6">
                <div className="mb-2 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <h4 className="text-base font-semibold text-agent-antigravity">Antigravity (Local)</h4>
                    <Badge className="border-agent-antigravity/30 bg-agent-antigravity/10 text-agent-antigravity">
                      로컬 한도
                    </Badge>
                  </div>
                  <span className="text-xs text-muted-foreground">로컬 전용 에이전트 연동</span>
                </div>
                <p className="mb-4 text-xs leading-relaxed text-muted-foreground">
                  로컬 환경에서 구동되는 Antigravity 에이전트의 로그 감시 상태 및 로컬 소비 한도를 추적합니다.
                </p>

                <div className="flex flex-col gap-5">
                  <div className="flex items-center justify-between rounded-lg border border-border bg-muted/20 p-3">
                    <div>
                      <span className="mb-0.5 block text-sm font-semibold text-agent-antigravity">
                        Antigravity 연동 상태
                      </span>
                      <span className="text-xs text-muted-foreground">
                        기본 OS globalStorage 파일시스템 자동 스캔 대기 중
                      </span>
                    </div>
                    <StatusBadge kind="active">스캔 활성화</StatusBadge>
                  </div>

                  <div className="grid grid-cols-1 gap-4 border-t border-dashed border-border pt-4 md:grid-cols-2">
                    <div className="flex flex-col gap-1.5">
                      <Label className="text-sm font-semibold">플랫폼 타입</Label>
                      <Input type="text" value="Local SQLite 감시 (VS Code User)" disabled />
                    </div>
                    <div className="flex flex-col gap-1.5">
                      <Label className="text-sm font-semibold">일간(24h) 소비 제한 한도 (Tokens)</Label>
                      <Input
                        type="number"
                        value={settings.token_limit_antigravity}
                        onChange={(e) =>
                          setSettings((prev) => ({ ...prev, token_limit_antigravity: Number(e.target.value) }))
                        }
                        onBlur={() =>
                          handleSaveSettings({ token_limit_antigravity: settings.token_limit_antigravity })
                        }
                      />
                    </div>
                  </div>
                </div>
              </Card>
            </div>
          </>
        )}
      </div>
    </TooltipProvider>
  );
}
export default SettingsView;
