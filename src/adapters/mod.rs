//! 에이전트 로그 수집 어댑터 모듈

pub mod codex;
pub mod antigravity;

/// 어댑터들이 구현해야 하는 공통 트레이트
pub trait LogAdapter {
    /// 지정된 파일/경로에서 세션 데이터를 파싱합니다.
    fn parse_session(&self, path: &str) -> Result<crate::model::Session, Box<dyn std::error::Error>>;
}
