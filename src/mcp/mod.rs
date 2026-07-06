//! MCP(Model Context Protocol) 서버 모듈
//!
//! ATK 데이터베이스에서 토큰 사용량 데이터를 조회할 수 있는
//! MCP 도구들을 노출하는 stdio 기반 서버입니다.
//!
//! 모든 도구 핸들러는 `server.rs`에 통합되어 있습니다.

pub mod server;
pub mod types;
