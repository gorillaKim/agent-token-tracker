//! ATK MCP 서버 구조체 및 실행 루프
//!
//! rmcp 2.1의 `#[tool_router]` + `#[tool_handler]` 패턴으로 8개 MCP 도구를 정의합니다.
//! 모든 응답은 에이전트 친화적인 컴팩트 Markdown 형식으로 반환됩니다.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use rusqlite::Connection;

use crate::db;
use crate::mcp::types::{
    LoopSession, SessionMatch,
    fmt_token_summary, fmt_session_report, fmt_today_usage,
    fmt_loop_suspects_md, fmt_tool_usage, fmt_search_sessions,
    fmt_mcp_plugin_summary, fmt_mcp_plugin_tools,
    fmt_tool_trend, fmt_tool_offenders, fmt_tool_percentiles,
    fmt_malfunction_patterns, fmt_malfunction_detections,
    fmt_malfunction_detections_v2, fmt_malfunction_summary,
};

// ─────────────────────────────────────────────────────────────────────────────
// 도구 파라미터 구조체 정의
// ─────────────────────────────────────────────────────────────────────────────

/// `get_token_summary` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TokenSummaryParams {
    /// 조회 시작일 (예: "2026-07-01"). 미지정 시 전체 기간.
    pub since: Option<String>,
    /// 반환할 최대 행 수.
    pub limit: Option<i64>,
}

/// `get_session_report` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SessionReportParams {
    /// 특정 세션 ID 필터. 미지정 시 전체 세션.
    pub session_id: Option<String>,
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
    /// 정렬 기준: "cost"(비용), "tokens"(토큰). 기본값: 시작시각 내림차순.
    pub sort: Option<String>,
    /// 반환할 최대 행 수.
    pub limit: Option<i64>,
}

/// `get_loop_suspects` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LoopSuspectsParams {
    /// 에이전트 타입 필터 (예: "claude_code", "codex"). 미지정 시 전체.
    pub agent_type: Option<String>,
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
    /// 반환할 최대 행 수. 기본값: 20.
    pub limit: Option<i64>,
}

/// `get_tool_usage` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolUsageParams {
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
    /// 정렬 기준: "count"(호출 수), "loop"(루프 의심). 기본값: 호출 수.
    pub sort: Option<String>,
    /// 반환할 최대 행 수.
    pub limit: Option<i64>,
}

/// `search_sessions` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchSessionsParams {
    /// cwd 경로에서 검색할 문자열 (예: "gorillaProject", "my-service").
    pub cwd_contains: String,
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
    /// 반환할 최대 행 수. 기본값: 30.
    pub limit: Option<i64>,
}

/// `get_mcp_plugin_summary` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct McpPluginSummaryParams {
    /// 조회 시작일 (예: "2026-07-01"). 미지정 시 전체 기간.
    pub since: Option<String>,
    /// 반환할 최대 행 수.
    pub limit: Option<i64>,
}

/// `get_mcp_plugin_tools` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct McpPluginToolsParams {
    /// 조회할 MCP 서버명 (필수). 예: "engram", "atk", "playwright".
    pub mcp_server: String,
    /// 조회 시작일 (예: "2026-07-01"). 미지정 시 전체 기간.
    pub since: Option<String>,
    /// 정렬 기준: "count"(호출 수), "tokens"(토큰), "cost"(비용). 기본값: 호출 수.
    pub sort: Option<String>,
    /// 반환할 최대 행 수.
    pub limit: Option<i64>,
}

/// `get_tool_trend` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolTrendParams {
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
}

/// `get_tool_offenders` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolOffendersParams {
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
    /// 반환할 최대 행 수. 기본값: 10.
    pub limit: Option<i64>,
}

/// `get_tool_percentiles` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolPercentilesParams {
    /// 조회 시작일 (예: "2026-07-01").
    pub since: Option<String>,
}

/// `register_malfunction_pattern` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RegisterMalfunctionPatternParams {
    /// 오작동 패턴의 고유 이름 (예: "지연 및 비정상 종료")
    pub pattern_name: String,
    /// 패턴 설명
    pub description: Option<String>,
    /// MalfunctionRule 구조체를 직렬화한 JSON 문자열
    pub rules_json: String,
}

/// `analyze_session_malfunctions` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeSessionMalfunctionsParams {
    /// 분석 대상 세션 ID
    pub session_id: String,
}

/// `get_session_malfunctions` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSessionMalfunctionsParams {
    /// 조회 대상 세션 ID
    pub session_id: String,
}

/// `delete_malfunction_pattern` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteMalfunctionPatternParams {
    /// 삭제할 오작동 패턴의 ID (i64)
    pub id: i64,
}

/// `get_malfunction_detections` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetMalfunctionDetectionsParams {
    /// 조회 시작일 (예: "2026-07-01"). 미지정 시 전체 기간.
    pub since: Option<String>,
    /// 특정 오작동 패턴명 필터.
    pub pattern_name: Option<String>,
    /// 특정 에이전트 타입 필터.
    pub agent_type: Option<String>,
}

/// `scan_and_detect_recent` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScanAndDetectRecentParams {
    /// 일괄 감지 기준 시작일 (필수, 예: "2026-07-01").
    pub since: String,
}

/// `validate_malfunction_pattern` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValidateMalfunctionPatternParams {
    /// 검증 및 FP 분석 대상 규칙 JSON 문자열.
    pub rules_json: String,
    /// 테스트해볼 최근 세션 수 (기본값: 30).
    pub limit: Option<i64>,
}

/// `ingest_logs` 파라미터
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IngestLogsParams {
    /// 이미 적재 완료된 파일도 강제 덮어쓰기할지 여부.
    pub force: Option<bool>,
}

// ─────────────────────────────────────────────────────────────────────────────
// ATK MCP 서버 핸들러
// ─────────────────────────────────────────────────────────────────────────────

/// ATK MCP 서버 핸들러
///
/// DB 커넥션을 `Arc<Mutex<Connection>>`으로 공유하여 비동기 환경에서 안전하게 접근합니다.
#[derive(Clone)]
pub struct AtkMcpServer {
    conn: Arc<Mutex<Connection>>,
}

#[tool_router]
impl AtkMcpServer {
    // ─────────────────────────────────────────────────────────────────
    // 기본 토큰 조회 도구 (6개)
    // ─────────────────────────────────────────────────────────────────

    /// 에이전트별·모델별 토큰 및 비용 사용량 집계를 조회합니다.
    #[tool(description = "에이전트별(codex, claude_code, antigravity 등) 토큰 사용량 및 비용을 집계합니다. since(시작일), limit(최대 행 수)를 선택적으로 지정할 수 있습니다.")]
    async fn get_token_summary(&self, Parameters(p): Parameters<TokenSummaryParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_agent_report(&conn, p.since.as_deref(), None, p.limit.map(|l| l as usize)) {
            Ok(result) => fmt_token_summary(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 세션 단위 토큰/비용 리포트를 조회합니다.
    #[tool(description = "세션 단위로 토큰 사용량, 비용, 에이전트 타입, 시작 시각을 조회합니다. session_id로 특정 세션을 지정하거나 since·sort·limit으로 필터링할 수 있습니다.")]
    async fn get_session_report(&self, Parameters(p): Parameters<SessionReportParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_session_report(
            &conn,
            p.session_id.as_deref(),
            p.since.as_deref(),
            p.sort.as_deref(),
            p.limit.map(|l| l as usize),
        ) {
            Ok(result) => fmt_session_report(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 오늘(UTC 기준) 토큰 사용량을 빠르게 요약합니다. 파라미터 불필요.
    #[tool(description = "오늘 시작된 세션들의 토큰 사용량과 비용을 빠르게 집계합니다.")]
    async fn get_today_usage(&self) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        let today = chrono_today_utc();
        match db::get_session_report(&conn, None, Some(&today), Some("cost"), Some(50)) {
            Ok(result) => fmt_today_usage(&today, &result),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 루프·오작동 의심 세션 목록을 조회합니다.
    #[tool(description = "무한 루프, 반복 실패 등 오작동 패턴이 감지된 세션 목록을 반환합니다. agent_type으로 특정 에이전트만 필터링 가능합니다.")]
    async fn get_loop_suspects(&self, Parameters(p): Parameters<LoopSuspectsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        let limit_n = p.limit.unwrap_or(20) as usize;

        let mut query = "
            SELECT DISTINCT s.session_id, s.agent_type, s.started_at,
                   COUNT(tc.id) AS loop_tool_count
            FROM sessions s
            JOIN tool_calls tc ON tc.session_id = s.session_id
            WHERE tc.is_loop_suspect = 1
        ".to_string();
        let mut pv: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(ref at) = p.agent_type {
            query.push_str(" AND s.agent_type = ? ");
            pv.push(rusqlite::types::Value::Text(at.clone()));
        }
        if let Some(ref d) = p.since {
            query.push_str(" AND s.started_at >= ? ");
            pv.push(rusqlite::types::Value::Text(d.clone()));
        }
        query.push_str(" GROUP BY s.session_id ORDER BY loop_tool_count DESC LIMIT ? ");
        pv.push(rusqlite::types::Value::Integer(limit_n as i64));

        let mut stmt = match conn.prepare(&query) {
            Ok(s) => s,
            Err(e) => return format!("❌ 쿼리 준비 실패: {e}"),
        };
        let pr: Vec<&dyn rusqlite::ToSql> = pv.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = match stmt.query_map(&pr[..], |row| {
            Ok(LoopSession {
                session_id: row.get(0)?,
                agent_type: row.get(1)?,
                started_at: row.get(2)?,
                loop_tool_count: row.get(3)?,
            })
        }) {
            Ok(r) => r,
            Err(e) => return format!("❌ 쿼리 실패: {e}"),
        };

        let mut list = Vec::new();
        for r in rows {
            match r {
                Ok(item) => list.push(item),
                Err(e) => return format!("❌ 행 파싱 실패: {e}"),
            }
        }
        fmt_loop_suspects_md(&list)
    }

    /// 도구 호출 빈도 및 루프 의심 통계를 조회합니다.
    #[tool(description = "에이전트가 사용한 도구별 호출 횟수, 성공 횟수, 루프 의심 횟수를 집계합니다. sort='count' 또는 'loop'로 정렬 가능합니다.")]
    async fn get_tool_usage(&self, Parameters(p): Parameters<ToolUsageParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_tool_report(&conn, p.since.as_deref(), p.sort.as_deref(), p.limit.map(|l| l as usize)) {
            Ok(result) => fmt_tool_usage(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 프로젝트 디렉토리 경로(cwd)를 기반으로 세션을 검색합니다.
    #[tool(description = "작업 디렉토리 경로(cwd)에 특정 문자열이 포함된 세션을 검색합니다. 예: cwd_contains='gorillaProject'로 특정 프로젝트 세션만 조회 가능합니다.")]
    async fn search_sessions(&self, Parameters(p): Parameters<SearchSessionsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        let limit_n = p.limit.unwrap_or(30) as usize;
        let pattern = format!("%{}%", p.cwd_contains);

        let mut query = "
            SELECT s.session_id, s.agent_type, s.cwd, s.model_id,
                   s.total_input_tokens, s.total_output_tokens, s.started_at,
                   COALESCE(mc.session_cost, 0.0) as total_cost_usd
            FROM sessions s
            LEFT JOIN (
                SELECT session_id, SUM(cost_usd) as session_cost FROM messages GROUP BY session_id
            ) mc ON s.session_id = mc.session_id
            WHERE s.cwd LIKE ?
        ".to_string();
        let mut pv: Vec<rusqlite::types::Value> = Vec::new();
        pv.push(rusqlite::types::Value::Text(pattern));

        if let Some(ref d) = p.since {
            query.push_str(" AND s.started_at >= ? ");
            pv.push(rusqlite::types::Value::Text(d.clone()));
        }
        query.push_str(" ORDER BY s.started_at DESC LIMIT ? ");
        pv.push(rusqlite::types::Value::Integer(limit_n as i64));

        let mut stmt = match conn.prepare(&query) {
            Ok(s) => s,
            Err(e) => return format!("❌ 쿼리 준비 실패: {e}"),
        };
        let pr: Vec<&dyn rusqlite::ToSql> = pv.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = match stmt.query_map(&pr[..], |row| {
            Ok(SessionMatch {
                session_id: row.get(0)?,
                agent_type: row.get(1)?,
                cwd: row.get(2)?,
                model_id: row.get(3)?,
                total_input_tokens: row.get::<_, i64>(4)? as u64,
                total_output_tokens: row.get::<_, i64>(5)? as u64,
                started_at: row.get(6)?,
                total_cost_usd: row.get(7)?,
            })
        }) {
            Ok(r) => r,
            Err(e) => return format!("❌ 쿼리 실패: {e}"),
        };

        let mut list = Vec::new();
        for r in rows {
            match r {
                Ok(item) => list.push(item),
                Err(e) => return format!("❌ 행 파싱 실패: {e}"),
            }
        }
        fmt_search_sessions(&list, &p.cwd_contains)
    }

    // ─────────────────────────────────────────────────────────────────
    // MCP 플러그인 전용 도구 (2개)
    // ─────────────────────────────────────────────────────────────────

    /// 사용 중인 MCP 플러그인(서버)별 사용량 전체를 집계합니다.
    #[tool(description = "에이전트가 호출한 MCP 플러그인(서버)별로 호출 횟수, 성공률, 루프 의심, 연관 세션 토큰/비용을 집계합니다. 어떤 MCP 서버를 가장 많이/비싸게 쓰는지 한눈에 파악할 수 있습니다.")]
    async fn get_mcp_plugin_summary(&self, Parameters(p): Parameters<McpPluginSummaryParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_mcp_server_report(&conn, p.since.as_deref(), p.limit.map(|l| l as usize)) {
            Ok(result) => fmt_mcp_plugin_summary(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 특정 MCP 플러그인(서버) 내 도구별 상세 사용량과 토큰 비용을 조회합니다.
    #[tool(description = "특정 MCP 서버(예: 'engram', 'atk') 내에서 각 도구별 호출 횟수, 성공률, 세션 토큰/비용을 조회합니다. sort='cost'로 비용 기준 정렬 가능합니다.")]
    async fn get_mcp_plugin_tools(&self, Parameters(p): Parameters<McpPluginToolsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_mcp_tool_report_by_server(
            &conn,
            &p.mcp_server,
            p.since.as_deref(),
            p.sort.as_deref(),
            p.limit.map(|l| l as usize),
        ) {
            Ok(result) => fmt_mcp_plugin_tools(&result, &p.mcp_server, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 도구별 결과 토큰 시계열 추세를 조회합니다.
    #[tool(description = "각 도구별 날짜별 평균 결과 토큰 크기 추세를 조회합니다. since(시작일)를 지정할 수 있습니다.")]
    async fn get_tool_trend(&self, Parameters(p): Parameters<ToolTrendParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_tool_trend(&conn, p.since.as_deref()) {
            Ok(result) => fmt_tool_trend(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 결과 데이터(페이로드) 크기가 가장 큰 Top-N 도구 호출을 조회합니다.
    #[tool(description = "가장 큰 응답 결과를 반환한 도구 호출의 상세 정보와 크기를 조회합니다. since(시작일), limit(기본 10)를 지정할 수 있습니다.")]
    async fn get_tool_offenders(&self, Parameters(p): Parameters<ToolOffendersParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        let limit_n = p.limit.unwrap_or(10) as usize;
        match db::get_tool_offenders(&conn, p.since.as_deref(), limit_n) {
            Ok(result) => fmt_tool_offenders(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 도구별 결과 토큰의 백분위 분포(p50, p90, Max)를 조회합니다.
    #[tool(description = "각 도구별 반환 결과 크기의 p50, p90, Max 분포를 조회합니다. since(시작일)를 지정할 수 있습니다.")]
    async fn get_tool_percentiles(&self, Parameters(p): Parameters<ToolPercentilesParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_tool_percentiles(&conn, p.since.as_deref()) {
            Ok(result) => fmt_tool_percentiles(&result, p.since.as_deref()),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 새로운 오작동 패턴을 등록합니다.
    #[tool(description = "새로운 오작동 패턴을 등록합니다. pattern_name(이름), description(설명), rules_json(규칙 JSON 문자열)이 필요합니다.")]
    async fn register_malfunction_pattern(&self, Parameters(p): Parameters<RegisterMalfunctionPatternParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::insert_malfunction_pattern(&conn, &p.pattern_name, p.description.as_deref(), &p.rules_json) {
            Ok(id) => format!("✅ 오작동 패턴 등록 완료 (ID: {})", id),
            Err(e) => format!("❌ 등록 실패: {e}"),
        }
    }

    /// 등록된 오작동 패턴 목록을 조회합니다.
    #[tool(description = "등록된 모든 오작동 패턴 목록을 조회합니다.")]
    async fn get_malfunction_patterns(&self) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_malfunction_patterns(&conn) {
            Ok(result) => fmt_malfunction_patterns(&result),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 특정 세션을 실시간으로 분석하여 감지된 오작동 내역을 DB에 기록하고, 그 결과를 반환합니다.
    #[tool(description = "특정 세션에 대해 오작동 감지 엔진을 가동합니다. session_id(세션 ID)가 필요합니다.")]
    async fn analyze_session_malfunctions(&self, Parameters(p): Parameters<AnalyzeSessionMalfunctionsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        
        // 1. 분석 및 오작동 검출 매칭 가동
        if let Err(e) = crate::detect::malfunctions::analyze_and_detect_malfunctions(&conn, &p.session_id) {
            return format!("❌ 분석 중 오류 발생: {e}");
        }

        // 2. 결과 리포트 조회
        match db::get_session_malfunction_reports(&conn, &p.session_id) {
            Ok(result) => fmt_malfunction_detections(&p.session_id, &result),
            Err(e) => format!("❌ 분석 결과 조회 실패: {e}"),
        }
    }

    /// 특정 세션에서 이미 감지된 오작동 이력 목록을 조회합니다.
    #[tool(description = "특정 세션의 기존 오작동 감지 이력을 조회합니다. session_id(세션 ID)가 필요합니다.")]
    async fn get_session_malfunctions(&self, Parameters(p): Parameters<GetSessionMalfunctionsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_session_malfunction_reports(&conn, &p.session_id) {
            Ok(result) => fmt_malfunction_detections(&p.session_id, &result),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 오작동 패턴을 삭제합니다.
    #[tool(description = "특정 오작동 패턴을 삭제합니다. id(패턴 ID)가 필요합니다.")]
    async fn delete_malfunction_pattern(&self, Parameters(p): Parameters<DeleteMalfunctionPatternParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::delete_malfunction_pattern(&conn, p.id) {
            Ok(_) => format!("✅ 오작동 패턴 삭제 완료 (ID: {})", p.id),
            Err(e) => format!("❌ 삭제 실패: {e}"),
        }
    }

    /// 조건에 매칭되는 오작동 감지 상세 이력 목록을 집계 조회합니다.
    #[tool(description = "조건(since, pattern_name, agent_type)을 지정하여 매칭되는 오작동 감지 상세 이력 목록을 집계 조회합니다.")]
    async fn get_malfunction_detections(&self, Parameters(p): Parameters<GetMalfunctionDetectionsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_malfunction_detections(
            &conn,
            p.since.as_deref(),
            p.pattern_name.as_deref(),
            p.agent_type.as_deref(),
        ) {
            Ok(result) => fmt_malfunction_detections_v2(&result),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 등록된 오작동 패턴별 매칭률, 누적 건수, 최초/최근 발생 시간, 최근 7일 추세를 요약 조회합니다.
    #[tool(description = "등록된 모든 오작동 패턴별 매칭률, 누적 감지 횟수, 최초/최근 감지 시각 및 최근 7일 일별 발생 추세를 요약 조회합니다.")]
    async fn get_malfunction_summary(&self) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::get_malfunction_summary(&conn) {
            Ok(result) => fmt_malfunction_summary(&result),
            Err(e) => format!("❌ 조회 실패: {e}"),
        }
    }

    /// 특정 시점 이후의 모든 세션에 대해 오작동 감지 엔진을 구동하고 결과를 DB에 반영합니다. (멱등 실행)
    #[tool(description = "since(시작일) 이후 시작된 세션들을 대상으로 오작동 분석을 일괄 수행하고, 그 결과를 DB에 멱등하게 적재합니다.")]
    async fn scan_and_detect_recent(&self, Parameters(p): Parameters<ScanAndDetectRecentParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        match db::scan_and_detect_recent(&conn, &p.since) {
            Ok(count) => format!("✅ 일괄 감지 완료 (분석 수행된 세션: {}개)", count),
            Err(e) => format!("❌ 일괄 감지 실패: {e}"),
        }
    }

    /// 오작동 패턴 규칙 JSON이 유효한지 검증하고, 최근 세션을 기반으로 False Positive(오탐) 가능성을 테스트합니다.
    #[tool(description = "규칙 JSON 문자열이 유효한지 파싱을 테스트하고, 최근 N개 세션을 대상으로 임시 평가를 돌려 False Positive(오탐) 확률을 백분율로 제공합니다.")]
    async fn validate_malfunction_pattern(&self, Parameters(p): Parameters<ValidateMalfunctionPatternParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        let limit_n = p.limit.unwrap_or(30) as usize;
        match crate::detect::malfunctions::validate_malfunction_pattern(&conn, &p.rules_json, limit_n) {
            Ok((valid, msg, _ratio, _is_fp, matched_samples)) => {
                let mut out = format!("### 규칙 검증 결과 요약\n\n{}\n\n", msg);
                if valid && !matched_samples.is_empty() {
                    out.push_str("#### 🔍 매칭 샘플 세션 및 감지 근거\n");
                    out.push_str("| 세션 ID | 감지 근거 (Evidence) |\n");
                    out.push_str("|---|---|\n");
                    for (sid, ev) in matched_samples.iter().take(10) {
                        let short_sid = if sid.len() > 8 { &sid[..8] } else { sid };
                        out.push_str(&format!("| `{}` | {} |\n", short_sid, ev));
                    }
                    if matched_samples.len() > 10 {
                        out.push_str(&format!("\n*(그 외 {}개의 세션이 더 매칭되었습니다)*\n", matched_samples.len() - 10));
                    }
                }
                out
            }
            Err(e) => format!("❌ 검증 처리 오류: {e}"),
        }
    }

    /// 로컬 에이전트 로그 디렉토리를 자동 감지하여 신규 로그들을 즉시 DB에 수집(Ingest)합니다.
    #[tool(description = "로컬의 Claude Code, Codex, Antigravity 로그 파일들을 자동 탐색하여 신규 세션 정보를 데이터베이스에 즉시 수집/동기화합니다.")]
    async fn ingest_logs(&self, Parameters(p): Parameters<IngestLogsParams>) -> String {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return "❌ DB 락 취득 실패".to_string(),
        };
        let paths = crate::adapters::ingest::detect_default_log_paths();
        if paths.is_empty() {
            return "⚠️ 감지된 에이전트 로그 경로가 없습니다. 로그가 정상 경로에 존재하는지 확인해주세요.".to_string();
        }
        
        let force_val = p.force.unwrap_or(false);
        match crate::adapters::ingest::ingest_logs(&conn, &paths, None, force_val) {
            Ok(res) => {
                let mut out = "### 📥 로그 수집(Ingest) 요약 보고\n\n".to_string();
                out.push_str(&format!("* 🔍 **스캔 경로**: 수집된 감지 디렉토리/파일 {}개\n", paths.len()));
                for path in &paths {
                    out.push_str(&format!("  - `{}`\n", path.display()));
                }
                out.push_str("\n| 항목 | 건수 |\n");
                out.push_str("|---|---|\n");
                out.push_str(&format!("| 총 발견된 파일 | {}개 |\n", res.files_total));
                out.push_str(&format!("| 스캔 진행한 세션 | {}개 |\n", res.sessions_scanned));
                out.push_str(&format!("| **새로 추가된 세션** | **{}개** |\n", res.sessions_inserted));
                out.push_str(&format!("| 중복 스킵된 세션 | {}개 |\n", res.sessions_skipped));
                out.push_str(&format!("| 실패한 세션 | {}개 |\n", res.sessions_failed));
                out
            }
            Err(e) => format!("❌ 로그 수집 실패: {e}"),
        }
    }
}

#[tool_handler]
impl ServerHandler for AtkMcpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        use rmcp::model::{ServerCapabilities, ServerInfo, Implementation};
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("atk", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "ATK(Agent Token Tracker) MCP 서버입니다. 응답은 Markdown 형식입니다.\n\
             - 오늘 현황: get_today_usage (파라미터 없음)\n\
             - MCP 서버 전체 집계: get_mcp_plugin_summary\n\
             - 특정 서버 도구별 비용: get_mcp_plugin_tools {mcp_server: 'engram'}\n\
             - 루프 의심 세션: get_loop_suspects\n\
             - 프로젝트별 세션: search_sessions {cwd_contains: 'my-project'}\n\
             - 도구별 결과 토큰 추세: get_tool_trend {since: '2026-07-01'}\n\
             - 가장 큰 도구 응답 Top-N: get_tool_offenders {limit: 10}\n\
             - 도구 결과 크기 백분위 분포: get_tool_percentiles\n\
             - 오작동 패턴 목록: get_malfunction_patterns\n\
             - 세션 오작동 분석: analyze_session_malfunctions {session_id: '...'}\n\
             - 세션 오작동 이력: get_session_malfunctions {session_id: '...'}\n\
             - 오작동 감지 이력 조회: get_malfunction_detections {since: '2026-07-01'}\n\
             - 오작동 패턴별 요약 통계: get_malfunction_summary\n\
             - since 이후 일괄 멱등 분석: scan_and_detect_recent {since: '2026-07-01'}\n\
             - 규칙 JSON 검증 및 FP 추정: validate_malfunction_pattern {rules_json: '...'}\n\
             - 로그 자동 감지 및 Ingest: ingest_logs {force: false}"
        )
    }
}

impl AtkMcpServer {
    /// 새 ATK MCP 서버 인스턴스를 생성합니다.
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }
}

/// ATK MCP 서버를 stdio 트랜스포트로 실행합니다.
///
/// stdin에서 JSON-RPC 요청을 읽고, stdout으로 Markdown 응답을 씁니다.
pub async fn run(db_path: String) -> Result<(), Box<dyn std::error::Error>> {
    let conn = crate::db::init_db(&db_path)
        .map_err(|e| format!("DB 초기화 실패: {e}"))?;
    eprintln!("[ATK MCP] 서버 시작. DB: {db_path}");
    let server = AtkMcpServer::new(conn);
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}

/// 오늘 날짜를 UTC 기준 "YYYY-MM-DD" 형식으로 반환합니다. (외부 크레이트 미사용)
fn chrono_today_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    days_to_date(secs / 86400)
}

fn days_to_date(days: u64) -> String {
    let mut d = days as i64;
    let mut y = 1970i64;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let diy = if leap { 366 } else { 365 };
        if d < diy { break; }
        d -= diy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = [31i64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1i64;
    for &dim in &months {
        if d < dim { break; }
        d -= dim;
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m, d + 1)
}
