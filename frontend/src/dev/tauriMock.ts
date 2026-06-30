/// <reference types="vite/client" />
/**
 * dev 전용 Tauri invoke 목 (import.meta.env.DEV 에서만 설치).
 *
 * 일반 브라우저(window.__TAURI_INTERNALS__ 부재)에서 실행될 때 types.ts 형태의
 * 픽스처를 반환하여 Puppeteer 등으로 UI 시각 검증을 가능하게 한다.
 * 실제 Tauri 런타임/프로덕션 빌드에서는 절대 설치되지 않는다(no-op).
 */
import type {
  AgentSummary,
  Session,
  LoopDetectionResult,
  PlanQuotaInfo,
  DailyTokenUsage,
  HourlyTokenUsage,
  SessionAnalysis,
  SessionDetails,
  DetectedCredential,
  DetectedLogPath,
} from "../types";

const ANOMALY_SESSION = "sess-claude-001";

const sessions: Session[] = [
  {
    session_id: ANOMALY_SESSION,
    session_name: "refactor-auth-module",
    agent_type: "claude_code",
    started_at: "2026-06-25T09:12:03",
    cwd: "/Users/dev/projects/web-core",
    model_id: "claude-opus-4-8",
    total_input_tokens: 184_200_000,
    total_output_tokens: 12_400_000,
  },
  {
    session_id: "sess-codex-114",
    session_name: "migrate-api-routes",
    agent_type: "codex",
    started_at: "2026-06-25T08:40:11",
    cwd: "/Users/dev/projects/api-gateway",
    model_id: "gpt-5-codex",
    total_input_tokens: 52_800_000,
    total_output_tokens: 4_100_000,
  },
  {
    session_id: "sess-claude-sub-7",
    session_name: "test-writer (sub)",
    agent_type: "claude_code",
    started_at: "2026-06-25T08:05:55",
    cwd: "/Users/dev/projects/web-core",
    model_id: "claude-sonnet-4-6",
    total_input_tokens: 9_300_000,
    total_output_tokens: 880_000,
    parent_session_id: ANOMALY_SESSION,
  },
  {
    session_id: "sess-anti-22",
    session_name: "local-doc-gen",
    agent_type: "antigravity",
    started_at: "2026-06-25T07:30:00",
    cwd: "/Users/dev/notes",
    model_id: "local-mixtral",
    total_input_tokens: 3_100_000,
    total_output_tokens: 410_000,
  },
];

const summaries: AgentSummary[] = [
  {
    agent_type: "claude_code",
    session_count: 2,
    total_input_tokens: 193_500_000,
    total_output_tokens: 13_280_000,
    total_cost_usd: 71.42,
  },
  {
    agent_type: "codex",
    session_count: 1,
    total_input_tokens: 52_800_000,
    total_output_tokens: 4_100_000,
    total_cost_usd: 14.07,
  },
  {
    agent_type: "antigravity",
    session_count: 1,
    total_input_tokens: 3_100_000,
    total_output_tokens: 410_000,
    total_cost_usd: 0,
  },
];

const anomalies: LoopDetectionResult[] = [
  {
    session_id: ANOMALY_SESSION,
    is_anomaly: true,
    signals: [
      {
        signal_type: "ping_pong",
        description: "Read↔Edit 도구가 6회 교대로 반복 호출되어 핑퐁 루프가 의심됩니다.",
        evidence: "tool_A=Read,tool_B=Edit,cycles=6",
      },
    ],
  },
  {
    session_id: "sess-codex-114",
    is_anomaly: true,
    signals: [
      {
        signal_type: "repeated_call",
        description: "동일 도구(Bash)가 8회 연속 반복 호출되었습니다.",
        evidence: "tool_name=Bash,count=8",
      },
    ],
  },
  {
    session_id: "sess-anti-22",
    is_anomaly: true,
    signals: [
      {
        signal_type: "repeated_failure",
        description: "도구 호출이 5회 연속 실패했습니다.",
        evidence: "tool_name=Write,failures=5",
      },
    ],
  },
];

const quotas: PlanQuotaInfo[] = [
  {
    provider: "anthropic",
    plan_key: "max20x",
    plan_label: "Claude Max 20x",
    quota_tokens: 44_000_000,
    used_tokens: 28_600_000,
    remaining_tokens: 15_400_000,
    usage_pct: 65,
    window_reset_at: "2026-06-25T13:00:00",
    window_hours: 5,
    weekly_quota_tokens: 880_000_000,
    weekly_used_tokens: 502_000_000,
    weekly_remaining_tokens: 378_000_000,
    weekly_usage_pct: 57,
    weekly_reset_at: "2026-06-30T00:00:00",
  },
  {
    provider: "openai",
    plan_key: "tier2",
    plan_label: "OpenAI Tier 2",
    quota_tokens: 100_000_000,
    used_tokens: 41_000_000,
    remaining_tokens: 59_000_000,
    usage_pct: 41,
    window_reset_at: "2026-06-25T12:30:00",
    window_hours: 5,
  },
  {
    provider: "antigravity",
    plan_key: "local",
    plan_label: "로컬 한도",
    quota_tokens: 50_000_000,
    used_tokens: 3_510_000,
    remaining_tokens: 46_490_000,
    usage_pct: 7,
    window_reset_at: null,
    window_hours: 24,
  },
];

const dailyTokenUsage: DailyTokenUsage[] = Array.from({ length: 14 }, (_, i) => {
  const day = String(12 + i).padStart(2, "0");
  const claude = 8_000_000 + ((i * 3137) % 11) * 1_500_000;
  const codex = 2_000_000 + ((i * 911) % 7) * 900_000;
  const anti = ((i * 53) % 5) * 300_000;
  return {
    date: `2026-06-${day}`,
    claude_tokens: claude,
    codex_tokens: codex,
    antigravity_tokens: anti,
    total_tokens: claude + codex + anti,
  };
});

const hourlyTokenUsage: HourlyTokenUsage[] = Array.from({ length: 24 }, (_, h) => {
  const claude = ((h * 7) % 9) * 900_000;
  const codex = ((h * 5) % 6) * 600_000;
  const anti = ((h * 3) % 4) * 150_000;
  return {
    hour: String(h),
    claude_tokens: claude,
    codex_tokens: codex,
    antigravity_tokens: anti,
    total_tokens: claude + codex + anti,
  };
});

const analysis: SessionAnalysis = {
  session_id: ANOMALY_SESSION,
  session_name: "refactor-auth-module",
  agent_type: "claude_code",
  model_id: "claude-opus-4-8",
  started_at: "2026-06-25T09:12:03",
  total_input_tokens: 184_200_000,
  total_output_tokens: 12_400_000,
  total_cache_read_tokens: 142_000_000,
  total_cost_usd: 71.42,
  cache_hit_rate: 0.77,
  cache_saved_cost: 58.3,
  is_anomaly: true,
  turns: Array.from({ length: 14 }, (_, i) => ({
    turn_index: i + 1,
    role: i % 2 === 0 ? "user" : "assistant",
    input_tokens: 6_000_000 + ((i * 211) % 9) * 1_200_000,
    output_tokens: 400_000 + ((i * 97) % 5) * 320_000,
    cache_read_tokens: 4_000_000 + ((i * 151) % 8) * 900_000,
    cost_usd: 2.1 + ((i * 13) % 7) * 0.6,
    created_at: "2026-06-25T09:12:03",
  })),
  tool_cost_rank: [
    { tool_name: "Read", call_count: 142, success_count: 140, estimated_tokens: 38_000_000, total_cost_usd: 18.4 },
    { tool_name: "Edit", call_count: 96, success_count: 94, estimated_tokens: 21_000_000, total_cost_usd: 11.2 },
    { tool_name: "mcp__serena__find_symbol", call_count: 54, success_count: 51, estimated_tokens: 9_400_000, total_cost_usd: 5.1 },
    { tool_name: "Bash", call_count: 31, success_count: 28, estimated_tokens: 4_200_000, total_cost_usd: 2.3 },
    { tool_name: "Grep", call_count: 22, success_count: 22, estimated_tokens: 1_800_000, total_cost_usd: 0.9 },
  ],
  anomaly_signals: anomalies[0].signals,
};

const sessionDetails: SessionDetails = {
  messages: [],
  tool_calls: Array.from({ length: 8 }, (_, i) => ({
    session_id: ANOMALY_SESSION,
    tool_name: i % 2 === 0 ? "Read" : "Edit",
    input_hash: `h${i}`,
    success: i !== 5,
    is_mcp: i % 3 === 0,
    cost_usd: 0.4 + (i % 3) * 0.2,
    created_at: "2026-06-25T09:12:03",
    tool_input: `{"file_path":"/Users/dev/projects/web-core/src/auth/handler-${i}.ts"}`,
  })),
};

const detectedCredentials: DetectedCredential[] = [
  {
    provider: "anthropic",
    token_type: "oauth",
    value: "sk-ant-oat01-…q9Zk",
    raw_value: "sk-ant-oat01-mock",
    source: "Keychain",
    description: "Claude Code OAuth Access Token",
  },
];

const logPaths: DetectedLogPath[] = [
  {
    agent: "claude_code",
    label: "Claude Code",
    default_path: "~/.claude/projects",
    configured_path: "",
    active_path: "/Users/dev/.claude/projects",
    exists: true,
  },
  {
    agent: "codex",
    label: "Codex (OpenAI)",
    default_path: "~/.codex/sessions",
    configured_path: "",
    active_path: "/Users/dev/.codex/sessions",
    exists: true,
  },
  {
    agent: "antigravity",
    label: "Antigravity (Local)",
    default_path: "~/Library/.../state.vscdb",
    configured_path: "",
    active_path: "/Users/dev/Library/.../state.vscdb",
    exists: false,
  },
];

const settings = {
  log_dir: "/Users/dev/logs",
  claude_log_dir: "",
  codex_log_dir: "",
  antigravity_log_dir: "",
  token_limit: 50_000_000,
  token_limit_claude: 50_000_000,
  token_limit_codex: 50_000_000,
  token_limit_antigravity: 50_000_000,
  claude_plan: "max20x",
  openai_plan: "tier2",
  token_display_mode: "tokens",
  refresh_interval: 3,
};

const handlers: Record<string, (args?: any) => unknown> = {
  // days 인자에 따라 슬라이스 — 1d/3d/7d 필터가 dev에서도 눈에 보이게.
  get_active_sessions: (args?: any) => {
    const days = args?.days as number | undefined;
    if (days === 1) return sessions.slice(0, 2);
    if (days === 3) return sessions.slice(0, 3);
    return sessions;
  },
  get_agent_summaries: () => summaries,
  get_loop_signals: (args?: any) => {
    const days = args?.days as number | undefined;
    if (days === 1) return anomalies.slice(0, 1);
    if (days === 3) return anomalies.slice(0, 2);
    return anomalies;
  },
  get_daily_token_usage: () => dailyTokenUsage,
  get_hourly_token_usage: () => hourlyTokenUsage,
  get_mcp_usage_trend: (args?: any) => {
    const days = (args?.days as number | undefined) ?? 7;
    if (days === 1) {
      return Array.from({ length: 24 }, (_, i) => {
        const hourStr = String(i).padStart(2, "0");
        return {
          label: hourStr,
          engram_calls: Math.max(0, Math.floor(Math.sin(i / 3) * 10 + 12)),
          doxus_calls: Math.max(0, Math.floor(Math.cos(i / 4) * 5 + 6)),
          playwright_calls: i % 4 === 0 ? 3 : 0,
          other_calls: i % 6 === 0 ? 1 : 0,
        };
      });
    } else {
      return Array.from({ length: days }, (_, i) => {
        const d = new Date();
        d.setDate(d.getDate() - (days - 1 - i));
        const dateStr = d.toISOString().split("T")[0];
        return {
          label: dateStr,
          engram_calls: Math.max(0, Math.floor(Math.sin(i + 1) * 20 + 35)),
          doxus_calls: Math.max(0, Math.floor(Math.cos(i + 2) * 10 + 15)),
          playwright_calls: Math.max(0, Math.floor(Math.sin(i * 1.5) * 5 + 8)),
          other_calls: i % 3 === 0 ? 3 : 1,
        };
      });
    }
  },
  get_subscription_quota: () => quotas,
  get_session_analysis: () => analysis,
  get_session_details: () => sessionDetails,
  get_local_credentials: () => detectedCredentials,
  get_detected_log_paths: () => logPaths,
  load_settings: () => settings,
  get_api_keys_status: () => ({ anthropic: true, openai: false }),
  validate_stored_api_key: () => true,
  validate_local_path: () => true,
  validate_api_key_value: () => true,
  save_settings: () => null,
  save_api_key: () => null,
  delete_api_key: () => null,
  auto_apply_credential: () => null,
  interrupt_agent: () => "인터럽트 신호가 전송되었습니다. (mock)",
  focus_main_window: () => null,
  sync_local_sessions: () => ({ files_total: 4, sessions_inserted: 0, sessions_skipped: 4, sessions_failed: 0 }),
  force_sync_local_sessions: () => ({ files_total: 4, sessions_inserted: 4, sessions_skipped: 0, sessions_failed: 0 }),
};

function mockInvoke(cmd: string, args?: unknown): unknown {
  if (cmd.startsWith("plugin:")) return 0; // event listen/unlisten 등
  const handler = handlers[cmd];
  if (handler) return handler(args);
  console.warn(`[tauriMock] 미정의 커맨드: ${cmd} → [] 반환`);
  return [];
}

let cbId = 1;

export function mountTauriMockIfDev(): void {
  if (!import.meta.env.DEV) return;
  if (typeof window === "undefined") return;
  if ((window as any).__TAURI_INTERNALS__) return; // 실제 Tauri 런타임이면 건드리지 않음

  (window as any).__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args?: unknown) => mockInvoke(cmd, args),
    transformCallback: (_cb?: (...a: unknown[]) => void) => cbId++,
  };
  console.info("[tauriMock] dev invoke 목 설치됨 (브라우저 시각 검증용)");
}
