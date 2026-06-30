/**
 * 에이전트 토큰 트래커 프론트엔드 공통 타입 정의 파일
 * 
 * 사용자의 한국어 문서화 선호에 맞춰 주석 및 문서화가 한국어로 작성되었습니다.
 */

export interface Session {
  session_id: string;
  session_name?: string;
  agent_type: string;
  agent_version?: string;
  started_at: string;
  ended_at?: string;
  cwd: string;
  model_id?: string;
  total_input_tokens: number;
  total_output_tokens: number;
  parent_session_id?: string;
}

export interface AgentSummary {
  agent_type: string;
  session_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

export interface LoopSignal {
  signal_type: string; // "repeated_call" | "repeated_failure" | "token_inflation" | "ping_pong"
  description: string;
  evidence: string;
}

export interface LoopDetectionResult {
  session_id: string;
  is_anomaly: boolean;
  signals: LoopSignal[];
}

export interface DailyTokenUsage {
  date: string;
  total_tokens: number;
  claude_tokens: number;
  codex_tokens: number;
  antigravity_tokens: number;
}

export interface HourlyTokenUsage {
  hour: string;
  total_tokens: number;
  claude_tokens: number;
  codex_tokens: number;
  antigravity_tokens: number;
}

/**
 * 캘린더 뷰용 일별 사용량 상세
 *
 * 백엔드 `get_daily_usage_in_range` 커맨드(src-tauri)가 반환하는 구조체와 1:1 매칭됩니다.
 * 토큰은 세션 시작일, 비용은 메시지 생성일 기준으로 사용자 PC 로컬 타임존 일자별 집계됩니다.
 */
export interface DailyUsageDetail {
  date: string; // "YYYY-MM-DD" (사용자 PC 로컬 타임존)
  total_tokens: number;
  claude_tokens: number;
  codex_tokens: number;
  antigravity_tokens: number;
  total_cost: number;
  claude_cost: number;
  codex_cost: number;
  antigravity_cost: number;
}

/**
 * 비용 랭킹 항목 (캘린더 일별 상세 모달용)
 *
 * tool_calls 에는 직접 비용이 없어, 세션 총비용을 도구 호출 수로 균등 배분한 **추정 비용**입니다.
 */
export interface CostRankItem {
  name: string;
  call_count: number;
  total_cost: number;
  total_tokens: number;
}

/** 특정 일자의 플러그인별·도구별 비용 랭킹 (백엔드 `get_day_cost_breakdown` 반환) */
export interface DayCostBreakdown {
  date: string;
  plugins: CostRankItem[];
  tools: CostRankItem[];
}

export interface ModelTokenUsage {
  model_id: string;
  total_tokens: number;
}

export interface PluginTokenUsage {
  plugin_name: string;
  total_tokens: number;
}

export interface SkillTokenUsage {
  skill_name: string;
  total_tokens: number;
}

export interface TokenUsageBreakdown {
  models: ModelTokenUsage[];
  plugins: PluginTokenUsage[];
  skills: SkillTokenUsage[];
}

export interface ToolCall {
  id?: number;
  session_id: string;
  tool_name: string;
  input_hash: string;
  success: boolean;
  cost_usd: number;
  created_at: string;
  tool_input: string;
}

export interface SessionDetails {
  messages: any[];
  tool_calls: ToolCall[];
}

/** 백엔드 load_settings 반환 구조 */
export interface SettingsDto {
  log_dir: string;
  claude_log_dir: string;
  codex_log_dir: string;
  antigravity_log_dir: string;
  token_limit: number;
  token_limit_claude: number;
  token_limit_codex: number;
  token_limit_antigravity: number;
  claude_plan: string;
  openai_plan: string;
  token_display_mode: string;
  refresh_interval: number;
}

/** sync_local_sessions / force_sync_local_sessions 반환 */
export interface SyncResult {
  files_total: number;
  sessions_inserted: number;
  sessions_skipped: number;
  sessions_failed: number;
}

export interface PlanQuotaInfo {
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

export interface DetectedCredential {
  provider: string;
  token_type: string;
  value: string;
  raw_value: string;
  source: string;
  description: string;
}

export interface DetectedLogPath {
  agent: string;            // "claude_code" | "codex" | "antigravity"
  label: string;            // 표시용 이름
  default_path: string;     // OS 기본 경로(자동 감지)
  configured_path: string;  // 사용자가 지정한 경로("" = 기본 경로 사용 중)
  active_path: string;      // 실제 사용 중인 경로
  exists: boolean;          // active_path 존재 여부
}

export interface TurnTokenUsage {
  turn_index: number;
  role: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cost_usd: number;
  created_at: string;
}

export interface ToolCostRank {
  tool_name: string;
  call_count: number;
  success_count: number;
  estimated_tokens: number;
  total_cost_usd: number;
}

export interface SessionAnalysis {
  session_id: string;
  session_name?: string;
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
