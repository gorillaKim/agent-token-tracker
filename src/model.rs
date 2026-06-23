//! 공통 데이터 모델 모듈
//!
//! 에이전트 활동, 세션, 토큰 사용량 등에 대한 정규화 모델 타입을 정의합니다.

/// 세션 정보를 나타내는 구조체
#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub agent_type: String, // "codex" | "claude_code" | "antigravity"
    pub agent_version: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub cwd: String,
    pub model_id: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub token_source: String, // "api" | "unavailable" | "parse_error" | "db_locked" | "permission_denied"
}
