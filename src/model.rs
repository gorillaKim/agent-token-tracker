//! 공통 데이터 모델 모듈
//!
//! 에이전트 활동, 세션, 토큰 사용량 등에 대한 정규화 모델 타입을 정의합니다.
//! 주석과 구조체 설명은 한국어 규칙을 준수하여 작성되었습니다.

use serde::{Deserialize, Serialize};

/// 에이전트 세션 정보를 나타내는 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub session_id: String,
    pub agent_type: String, // 예: "codex" | "claude_code" | "antigravity"
    pub agent_version: Option<String>,
    pub started_at: String, // ISO8601 형식
    pub ended_at: Option<String>, // ISO8601 형식
    pub cwd: String, // 실행 절대경로
    pub model_id: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_input_tokens: u64,
    pub token_source: String, // "api" | "unavailable" | "parse_error" | "db_locked" | "permission_denied"
    pub session_name: Option<String>,
    pub parent_session_id: Option<String>,
}

impl Session {
    pub fn new(
        session_id: String,
        agent_type: String,
        agent_version: Option<String>,
        started_at: String,
        ended_at: Option<String>,
        cwd: String,
        model_id: Option<String>,
        total_input_tokens: u64,
        total_output_tokens: u64,
        total_cache_creation_input_tokens: u64,
        token_source: String,
        session_name: Option<String>,
        parent_session_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            agent_type,
            agent_version,
            started_at,
            ended_at,
            cwd,
            model_id,
            total_input_tokens,
            total_output_tokens,
            total_cache_creation_input_tokens,
            token_source,
            session_name,
            parent_session_id,
        }
    }
}

/// 세션 내부의 개별 메시지/턴 정보를 나타내는 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: Option<i64>,
    pub session_id: String,
    pub turn_index: u64,
    pub role: String, // "user" | "agent"
    pub input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub created_at: String,
    pub content: Option<String>,
}

impl Message {
    pub fn new(
        session_id: String,
        turn_index: u64,
        role: String,
        input_tokens: u64,
        cache_read_input_tokens: u64,
        cache_creation_input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        created_at: String,
        content: Option<String>,
    ) -> Self {
        Self {
            id: None,
            session_id,
            turn_index,
            role,
            input_tokens,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            output_tokens,
            cost_usd,
            created_at,
            content,
        }
    }
}

/// 에이전트 행동 블록 노드를 나타내는 구조체 (스파이크 및 파일 변경 관련)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: Option<i64>,
    pub session_id: String,
    pub node_type: String, // "tool_call" | "text" | "patch"
    pub success: bool,
    pub created_at: String,
}

impl Node {
    pub fn new(session_id: String, node_type: String, success: bool, created_at: String) -> Self {
        Self {
            id: None,
            session_id,
            node_type,
            success,
            created_at,
        }
    }
}

/// 도구(MCP 도구 및 내장 도구) 호출 상세 기록 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: Option<i64>,
    pub session_id: String,
    pub tool_name: String,
    pub tool_input: Option<String>, // JSON 직렬화된 문자열
    pub input_hash: String,
    pub success: bool,
    pub is_loop_suspect: bool,       // 루프 오작동 의심 플래그
    pub is_mcp: bool,                // MCP 도구 여부 플래그
    pub mcp_server: Option<String>,  // MCP 서버명 (예: "engram")
    pub mcp_tool: Option<String>,    // MCP 원천 도구명 (예: "epic_create")
    pub created_at: String,
    pub result_char_count: Option<i64>,
    pub result_est_tokens: Option<i64>,
    pub tool_use_id: Option<String>,
}

impl ToolCall {
    pub fn new(
        session_id: String,
        tool_name: String,
        tool_input: Option<String>,
        input_hash: String,
        success: bool,
        is_loop_suspect: bool,
        is_mcp: bool,
        mcp_server: Option<String>,
        mcp_tool: Option<String>,
        created_at: String,
    ) -> Self {
        Self {
            id: None,
            session_id,
            tool_name,
            tool_input,
            input_hash,
            success,
            is_loop_suspect,
            is_mcp,
            mcp_server,
            mcp_tool,
            created_at,
            result_char_count: None,
            result_est_tokens: None,
            tool_use_id: None,
        }
    }
}

/// 모델별 토큰 단가 정보 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pricing {
    pub model_id: String,
    pub provider: String,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cached_input_cost_per_million: f64,
    pub updated_at: String,
}

impl Pricing {
    pub fn new(
        model_id: String,
        provider: String,
        input_cost_per_million: f64,
        output_cost_per_million: f64,
        cached_input_cost_per_million: f64,
        updated_at: String,
    ) -> Self {
        Self {
            model_id,
            provider,
            input_cost_per_million,
            output_cost_per_million,
            cached_input_cost_per_million,
            updated_at,
        }
    }
}

/// 세션별 리포트 요약 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionReport {
    pub session_id: String,
    pub agent_type: String,
    pub model_id: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub started_at: String,
}

impl SessionReport {
    pub fn new(
        session_id: String,
        agent_type: String,
        model_id: Option<String>,
        total_input_tokens: u64,
        total_output_tokens: u64,
        total_cost_usd: f64,
        started_at: String,
    ) -> Self {
        Self {
            session_id,
            agent_type,
            model_id,
            total_input_tokens,
            total_output_tokens,
            total_cost_usd,
            started_at,
        }
    }
}

/// 에이전트별 리포트 요약 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReport {
    pub agent_type: String,
    pub session_count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

impl AgentReport {
    pub fn new(
        agent_type: String,
        session_count: u64,
        total_input_tokens: u64,
        total_output_tokens: u64,
        total_cost_usd: f64,
    ) -> Self {
        Self {
            agent_type,
            session_count,
            total_input_tokens,
            total_output_tokens,
            total_cost_usd,
        }
    }
}

/// 도구별 리포트 요약 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolReport {
    pub tool_name: String,
    pub call_count: u64,
    pub success_count: u64,
    pub loop_suspect_count: u64,
    pub total_result_char_count: u64,
    pub total_result_est_tokens: u64,
    pub avg_result_est_tokens: f64,
}

impl ToolReport {
    pub fn new(
        tool_name: String,
        call_count: u64,
        success_count: u64,
        loop_suspect_count: u64,
        total_result_char_count: u64,
        total_result_est_tokens: u64,
        avg_result_est_tokens: f64,
    ) -> Self {
        Self {
            tool_name,
            call_count,
            success_count,
            loop_suspect_count,
            total_result_char_count,
            total_result_est_tokens,
            avg_result_est_tokens,
        }
    }
}

/// MCP 서버(플러그인)별 사용량 집계 리포트
///
/// 토큰 수치는 '세션 기여도 방식'으로 집계됩니다.
/// 즉, 해당 MCP 서버를 1회 이상 호출한 세션들의 총 토큰 합계이며,
/// 세션 내 다른 작업의 토큰도 포함됩니다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerReport {
    pub mcp_server: String,
    pub call_count: u64,
    pub success_count: u64,
    pub loop_suspect_count: u64,
    pub distinct_sessions: u64,
    pub session_total_input_tokens: u64,
    pub session_total_output_tokens: u64,
    pub session_total_cost_usd: f64,
    pub total_result_char_count: u64,
    pub total_result_est_tokens: u64,
}

impl McpServerReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mcp_server: String,
        call_count: u64,
        success_count: u64,
        loop_suspect_count: u64,
        distinct_sessions: u64,
        session_total_input_tokens: u64,
        session_total_output_tokens: u64,
        session_total_cost_usd: f64,
        total_result_char_count: u64,
        total_result_est_tokens: u64,
    ) -> Self {
        Self {
            mcp_server,
            call_count,
            success_count,
            loop_suspect_count,
            distinct_sessions,
            session_total_input_tokens,
            session_total_output_tokens,
            session_total_cost_usd,
            total_result_char_count,
            total_result_est_tokens,
        }
    }
}

/// MCP 서버 내 개별 도구별 상세 사용량 리포트
///
/// 토큰 수치는 '세션 기여도 방식'으로 집계됩니다.
/// `note` 필드에 귀속 방식에 대한 설명이 포함됩니다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolDetailReport {
    pub mcp_server: String,
    pub mcp_tool: String,
    pub call_count: u64,
    pub success_count: u64,
    pub loop_suspect_count: u64,
    pub distinct_sessions: u64,
    pub session_total_input_tokens: u64,
    pub session_total_output_tokens: u64,
    pub session_total_cost_usd: f64,
    pub total_result_char_count: u64,
    pub total_result_est_tokens: u64,
    pub avg_result_est_tokens: f64,
    /// 토큰 귀속 방식 설명 (에이전트에게 수치의 의미를 명확히 전달)
    pub note: String,
}

impl McpToolDetailReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mcp_server: String,
        mcp_tool: String,
        call_count: u64,
        success_count: u64,
        loop_suspect_count: u64,
        distinct_sessions: u64,
        session_total_input_tokens: u64,
        session_total_output_tokens: u64,
        session_total_cost_usd: f64,
        total_result_char_count: u64,
        total_result_est_tokens: u64,
        avg_result_est_tokens: f64,
    ) -> Self {
        Self {
            mcp_server,
            mcp_tool,
            call_count,
            success_count,
            loop_suspect_count,
            distinct_sessions,
            session_total_input_tokens,
            session_total_output_tokens,
            session_total_cost_usd,
            total_result_char_count,
            total_result_est_tokens,
            avg_result_est_tokens,
            note: "세션귀속 방식: '입력/출력/비용 (세션귀속)' 수치는 해당 MCP 도구를 1회 이상 사용한 세션 전체의 토큰 및 비용 총합이며, 세션 내 다른 작업의 토큰이 포함되어 중복계상(overlap)될 수 있습니다. 반면 '결과토큰(추정)'은 도구의 tool_result 페이로드 크기 기반으로 도구 단독 비용을 추정한 값(estimate)입니다.".to_string(),
        }
    }
}

/// 오작동 규칙 매칭을 위한 Enum 정의
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MalfunctionRule {
    /// 대상 에이전트 종류 필터링 (예: ["claude_code", "antigravity"])
    TargetAgentTypes { agent_types: Vec<String> },

    /// 대상 AI 모델 ID 필터링 (예: ["claude-3-5-sonnet", "gpt-4o"])
    TargetModelIds { model_ids: Vec<String> },

    /// 대상 AI 제공사/회사 필터링 (예: ["anthropic", "openai", "google"])
    TargetProviders { providers: Vec<String> },

    /// 예상치 못한 종료 여부 (bool)
    UnexpectedExit { value: bool },

    /// 답변 지연 시간 임계치 (초 단위)
    MaxResponseDelaySec { value: u64 },

    /// 특정 도구/스킬의 연속 실패 임계치
    ConsecutiveToolFailures {
        tool_name: Option<String>,
        count_threshold: usize,
    },

    /// 특정 플러그인 또는 서버의 발동 횟수 임계치
    PluginTriggerLimit {
        mcp_server: String,
        mcp_tool: Option<String>,
        count_threshold: usize,
    },
    
    /// 정규식(Regex) 기반 유연한 에러 및 로그 패턴 감지
    ErrorMessagePatterns { 
        patterns: Vec<String>,
        is_regex: bool,
    },

    /// (동적 핑퐁) 임의의 두 도구 간의 왕복 핑퐁 루프 횟수 임계치 (A <-> B)
    DynamicPingPong { cycles_threshold: usize },

    /// (동적 다중 순환) 3개 이상의 도구가 순환하는 루프 횟수 임계치 (A -> B -> C -> A)
    DynamicCyclicLoop { 
        window_size: usize,
        cycles_threshold: usize,
    },

    /// (동적 반복) 임의의 동일 도구 연속 반복 호출 횟수 임계치
    DynamicRepeatedCalls { count_threshold: usize },

    /// (토큰 효율성) 누적 입력 토큰 증가율 대비 유의미한 코드 생성(출력)율이 지나치게 낮음
    TokenInefficiency { ratio_threshold: f64 },

    /// (비용 임계치) 단일 세션에서 소모한 누적 비용 임계치
    MaxSessionCostUsd { limit_usd: f64 },

    /// (턴 복잡도) 한 턴 내부에서 연속적으로 호출된 도구의 수 임계치
    MaxToolCallsPerTurn { count_threshold: usize },

    /// (세션 진전 지연) 전체 대화 세션의 누적 턴 수
    MaxTurnCount { count_threshold: usize },

    /// (사용자 인터랙션 차단) 사용자가 도구 실행(승인)을 거절/취소한 횟수 임계치
    UserInterruptionLimit { count_threshold: usize },

    /// (자식 세션 연쇄 분석) 자식 세션(Subagent 세션) 중 오작동으로 식별된 세션의 개수
    SubagentAnomalyLimit { count_threshold: usize },

    /// 시계열 흐름 매칭 (정의된 단계적 규칙이 순서대로 발생했는지 판정)
    Sequence { steps: Vec<MalfunctionRule> },

    /// Logical AND 연산자 (하위 모든 조건 만족 시 참)
    And { conditions: Vec<MalfunctionRule> },

    /// Logical OR 연산자 (하위 조건 중 하나라도 만족 시 참)
    Or { conditions: Vec<MalfunctionRule> },

    /// Logical NOT 연산자 (하위 조건의 부정)
    Not { condition: Box<MalfunctionRule> },
}

/// 오작동 패턴 데이터 모델
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MalfunctionPattern {
    pub id: i64,
    pub pattern_name: String,
    pub description: Option<String>,
    pub rules_json: String,
    pub created_at: String,
}

/// 오작동 탐지 이력 데이터 모델
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MalfunctionDetection {
    pub id: i64,
    pub session_id: String,
    pub pattern_id: i64,
    pub evidence: String,
    pub detected_at: String,
    pub is_false_positive: bool,
}

/// 오작동 감지 보고서 데이터 모델 (패턴명 조인 결과)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MalfunctionReport {
    pub id: i64,
    pub session_id: String,
    pub pattern_name: String,
    pub description: Option<String>,
    pub evidence: String,
    pub detected_at: String,
    pub is_false_positive: bool,
}


