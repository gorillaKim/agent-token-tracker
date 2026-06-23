//! 에이전트 로그 수집 어댑터 모듈

pub mod codex;
pub mod antigravity;
pub mod claude_code;

/// 단일 세션 로그 파싱 결과를 묶어 데이터베이스 적재를 돕는 구조체
#[derive(Debug, Clone)]
pub struct NormalizedSession {
    pub session: crate::model::Session,
    pub messages: Vec<crate::model::Message>,
    pub nodes: Vec<crate::model::Node>,
    pub tool_calls: Vec<crate::model::ToolCall>,
}

/// 어댑터들이 구현해야 하는 공통 트레이트
pub trait LogAdapter {
    /// 지정된 파일/경로에서 세션 데이터를 파싱하고 정규화된 묶음 데이터를 반환합니다.
    fn parse_session(&self, path: &str) -> Result<NormalizedSession, Box<dyn std::error::Error>>;
}
