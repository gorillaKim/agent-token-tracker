//! Codex 어댑터 모듈

use crate::model::Session;
use super::LogAdapter;

pub struct CodexAdapter;

impl LogAdapter for CodexAdapter {
    fn parse_session(&self, path: &str) -> Result<Session, Box<dyn std::error::Error>> {
        // TODO: rollout-*.jsonl 파일 파싱 로직 구현 예정
        Err("아직 구현되지 않았습니다.".into())
    }
}
