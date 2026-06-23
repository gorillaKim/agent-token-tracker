//! 공통 데이터 모델 모듈
//!
//! 에이전트 활동, 세션, 토큰 사용량 등에 대한 정규화 모델 타입을 정의합니다.
//! 주석과 구조체 설명은 한국어 규칙을 준수하여 작성되었습니다.

use serde::{Deserialize, Serialize};

/// 에이전트 세션 정보를 나타내는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// 세션 내부의 개별 메시지/턴 정보를 나타내는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// 에이전트 행동 블록 노드를 나타내는 구조체 (스파이크 및 파일 변경 관련)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Option<i64>,
    pub session_id: String,
    pub node_type: String, // "tool_call" | "text" | "patch"
    pub success: bool,
    pub created_at: String,
}

/// 도구(MCP 도구 및 내장 도구) 호출 상세 기록 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 모델별 토큰 단가 정보 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    pub model_id: String,
    pub provider: String,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cached_input_cost_per_million: f64,
    pub updated_at: String,
}
