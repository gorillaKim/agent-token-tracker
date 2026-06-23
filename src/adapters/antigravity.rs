//! Antigravity 어댑터 모듈

use crate::model::Session;
use super::LogAdapter;

pub struct AntigravityAdapter;

impl LogAdapter for AntigravityAdapter {
    fn parse_session(&self, _path: &str) -> Result<Session, Box<dyn std::error::Error>> {
        // TODO: state.vscdb protobuf 디코딩 로직 구현 예정
        Err("아직 구현되지 않았습니다.".into())
    }
}
