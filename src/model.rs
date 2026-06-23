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
    pub is_loop_suspect: bool, // 루프 오작동 의심 플래그
    pub created_at: String,
}

impl ToolCall {
    pub fn new(
        session_id: String,
        tool_name: String,
        tool_input: Option<String>,
        input_hash: String,
        success: bool,
        is_loop_suspect: bool,
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
            created_at,
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
}

impl ToolReport {
    pub fn new(
        tool_name: String,
        call_count: u64,
        success_count: u64,
        loop_suspect_count: u64,
    ) -> Self {
        Self {
            tool_name,
            call_count,
            success_count,
            loop_suspect_count,
        }
    }
}

