//! agent-token-tracker 라이브러리 크레이트 진입점
//!
//! 통합 테스트(`tests/`)에서 `use agent_token_tracker::...`로 임포트 가능하도록
//! 공개 모듈을 재노출합니다.

pub mod model;
pub mod db;
pub mod pricing;
pub mod adapters;
pub mod detect;
pub mod cross_check;
// tui는 crossterm I/O가 필요하므로 테스트 환경에서 조건부로만 노출
#[cfg(not(test))]
pub mod tui;
