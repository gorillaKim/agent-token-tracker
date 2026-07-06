//! MCP 응답 Markdown 포매터
//!
//! 에이전트가 읽기 쉬운 컴팩트한 Markdown 형식으로 응답을 생성합니다.
//! engram MCP 서버와 동일한 응답 철학을 따릅니다.

use crate::model::{AgentReport, SessionReport, ToolReport, McpServerReport, McpToolDetailReport};

// ─────────────────────────────────────────────────────────────────────────────
// 공통 유틸
// ─────────────────────────────────────────────────────────────────────────────

/// 큰 숫자를 읽기 쉬운 K/M 단위로 줄여서 표시합니다.
pub fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// 비용을 달러 단위로 포매팅합니다.
pub fn fmt_cost(usd: f64) -> String {
    if usd >= 1.0 {
        format!("${:.2}", usd)
    } else {
        format!("${:.4}", usd)
    }
}

/// 성공률을 퍼센트로 표시합니다.
pub fn fmt_rate(success: u64, total: u64) -> String {
    if total == 0 {
        "—".to_string()
    } else {
        format!("{:.0}%", success as f64 / total as f64 * 100.0)
    }
}

/// session_id를 앞 8자리로 단축합니다.
pub fn short_id(id: &str) -> &str {
    if id.len() > 8 { &id[..8] } else { id }
}

// ─────────────────────────────────────────────────────────────────────────────
// 도구별 Markdown 포매터
// ─────────────────────────────────────────────────────────────────────────────

/// `get_token_summary` 응답 포매터 — 에이전트별 토큰·비용 집계
pub fn fmt_token_summary(data: &[AgentReport], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## 토큰 요약\n데이터가 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## 에이전트별 토큰 요약{period}\n\n");
    out.push_str("| 에이전트 | 세션 | 입력 | 출력 | 비용 |\n");
    out.push_str("|---|---|---|---|---|\n");
    for r in data {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            r.agent_type,
            r.session_count,
            fmt_tokens(r.total_input_tokens),
            fmt_tokens(r.total_output_tokens),
            fmt_cost(r.total_cost_usd),
        ));
    }
    let total_in: u64 = data.iter().map(|r| r.total_input_tokens).sum();
    let total_out: u64 = data.iter().map(|r| r.total_output_tokens).sum();
    let total_cost: f64 = data.iter().map(|r| r.total_cost_usd).sum();
    out.push_str(&format!(
        "\n**합계** — 입력 {} / 출력 {} / 비용 {}",
        fmt_tokens(total_in), fmt_tokens(total_out), fmt_cost(total_cost)
    ));
    out
}

/// `get_session_report` 응답 포매터 — 세션별 토큰·비용 리포트
pub fn fmt_session_report(data: &[SessionReport], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## 세션 리포트\n데이터가 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## 세션 리포트{period}\n\n");
    out.push_str("| ID | 에이전트 | 모델 | 입력 | 출력 | 비용 | 시작 |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for r in data {
        let model = r.model_id.as_deref().unwrap_or("—");
        let started = &r.started_at[..10.min(r.started_at.len())];
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
            short_id(&r.session_id),
            r.agent_type,
            model,
            fmt_tokens(r.total_input_tokens),
            fmt_tokens(r.total_output_tokens),
            fmt_cost(r.total_cost_usd),
            started,
        ));
    }
    out
}

/// `get_today_usage` 응답 포매터 — 오늘 사용량 요약
pub fn fmt_today_usage(date: &str, sessions: &[SessionReport]) -> String {
    let total_in: u64 = sessions.iter().map(|s| s.total_input_tokens).sum();
    let total_out: u64 = sessions.iter().map(|s| s.total_output_tokens).sum();
    let total_cost: f64 = sessions.iter().map(|s| s.total_cost_usd).sum();

    let mut out = format!(
        "## 오늘 사용량 ({date})\n\n\
         - 세션: **{}개**\n\
         - 입력 토큰: **{}**\n\
         - 출력 토큰: **{}**\n\
         - 총 비용: **{}**\n",
        sessions.len(),
        fmt_tokens(total_in),
        fmt_tokens(total_out),
        fmt_cost(total_cost),
    );

    if !sessions.is_empty() {
        out.push_str("\n| ID | 에이전트 | 모델 | 입력 | 출력 | 비용 |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for s in sessions.iter().take(10) {
            let model = s.model_id.as_deref().unwrap_or("—");
            out.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} |\n",
                short_id(&s.session_id),
                s.agent_type,
                model,
                fmt_tokens(s.total_input_tokens),
                fmt_tokens(s.total_output_tokens),
                fmt_cost(s.total_cost_usd),
            ));
        }
        if sessions.len() > 10 {
            out.push_str(&format!("\n*... 외 {}개 세션*", sessions.len() - 10));
        }
    }
    out
}

/// `get_tool_usage` 응답 포매터 — 도구 호출 통계
pub fn fmt_tool_usage(data: &[ToolReport], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## 도구 사용 통계\n데이터가 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## 도구 사용 통계{period}\n\n");
    out.push_str("| 도구 | 호출 | 성공률 | 루프의심 | 결과 토큰 (추정 합/평균) |\n");
    out.push_str("|---|---|---|---|---|\n");
    for r in data {
        let loop_flag = if r.loop_suspect_count > 0 { " ⚠️" } else { "" };
        out.push_str(&format!(
            "| `{}` | {} | {} | {}{} | {} / {} |\n",
            r.tool_name,
            r.call_count,
            fmt_rate(r.success_count, r.call_count),
            r.loop_suspect_count,
            loop_flag,
            fmt_tokens(r.total_result_est_tokens),
            fmt_tokens(r.avg_result_est_tokens as u64),
        ));
    }
    out
}

/// `get_loop_suspects` 응답 포매터 — 루프 의심 세션
pub fn fmt_loop_suspects_md(rows: &[LoopSession]) -> String {
    if rows.is_empty() {
        return "## ✅ 루프 의심 세션 없음\n정상 범위 내에서 동작 중입니다.".to_string();
    }
    let mut out = format!("## ⚠️ 루프 의심 세션 ({}개)\n\n", rows.len());
    out.push_str("| ID | 에이전트 | 루프 호출 수 | 시작 |\n");
    out.push_str("|---|---|---|---|\n");
    for r in rows {
        let started = &r.started_at[..10.min(r.started_at.len())];
        out.push_str(&format!(
            "| `{}` | `{}` | **{}** | {} |\n",
            short_id(&r.session_id),
            r.agent_type,
            r.loop_tool_count,
            started,
        ));
    }
    out
}

/// `search_sessions` 응답 포매터 — cwd 검색 결과
pub fn fmt_search_sessions(rows: &[SessionMatch], cwd_contains: &str) -> String {
    if rows.is_empty() {
        return format!("## 세션 검색: `{cwd_contains}`\n일치하는 세션이 없습니다.");
    }
    let mut out = format!("## 세션 검색: `{cwd_contains}` ({}개)\n\n", rows.len());
    out.push_str("| ID | 에이전트 | 모델 | 입력 | 출력 | 비용 | 시작 |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for r in rows {
        let model = r.model_id.as_deref().unwrap_or("—");
        let started = &r.started_at[..10.min(r.started_at.len())];
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
            short_id(&r.session_id),
            r.agent_type,
            model,
            fmt_tokens(r.total_input_tokens),
            fmt_tokens(r.total_output_tokens),
            fmt_cost(r.total_cost_usd),
            started,
        ));
    }
    out
}

/// `get_mcp_plugin_summary` 응답 포매터 — MCP 서버별 사용량 집계
pub fn fmt_mcp_plugin_summary(data: &[McpServerReport], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## MCP 플러그인 사용량\nMCP 도구 호출 기록이 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## MCP 플러그인 사용량{period}\n\n");
    out.push_str("| 서버 | 호출 | 성공률 | 루프⚠️ | 세션 | 결과토큰(추정 합) | 입력(세션귀속⚠️) | 출력(세션귀속⚠️) | 비용(세션귀속⚠️) |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for r in data {
        let loop_flag = if r.loop_suspect_count > 0 { format!("**{}** ⚠️", r.loop_suspect_count) } else { "0".to_string() };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.mcp_server,
            r.call_count,
            fmt_rate(r.success_count, r.call_count),
            loop_flag,
            r.distinct_sessions,
            fmt_tokens(r.total_result_est_tokens),
            fmt_tokens(r.session_total_input_tokens),
            fmt_tokens(r.session_total_output_tokens),
            fmt_cost(r.session_total_cost_usd),
        ));
    }
    out.push_str("\n> ℹ️ '결과토큰(추정)'은 해당 서버 도구들이 반환한 결과 크기로 산출한 단독 추정 비용의 기초입니다.\n> ℹ️ '세션귀속' 토큰/비용은 해당 서버를 호출한 세션 전체 기준 집계이며, 중복계상(overlap)될 수 있습니다.");
    out
}

/// `get_mcp_plugin_tools` 응답 포매터 — 특정 MCP 서버의 도구별 상세
pub fn fmt_mcp_plugin_tools(data: &[McpToolDetailReport], mcp_server: &str, since: Option<&str>) -> String {
    if data.is_empty() {
        return format!("## `{mcp_server}` 도구 사용량\n호출 기록이 없습니다.");
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## `{mcp_server}` 도구별 사용량{period}\n\n");
    out.push_str("| 도구 | 호출 | 성공률 | 루프⚠️ | 세션 | 결과토큰(추정 합/평균) | 입력(세션귀속⚠️) | 출력(세션귀속⚠️) | 비용(세션귀속⚠️) |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for r in data {
        let loop_flag = if r.loop_suspect_count > 0 { format!("**{}** ⚠️", r.loop_suspect_count) } else { "0".to_string() };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} / {} | {} | {} | {} |\n",
            r.mcp_tool,
            r.call_count,
            fmt_rate(r.success_count, r.call_count),
            loop_flag,
            r.distinct_sessions,
            fmt_tokens(r.total_result_est_tokens),
            fmt_tokens(r.avg_result_est_tokens as u64),
            fmt_tokens(r.session_total_input_tokens),
            fmt_tokens(r.session_total_output_tokens),
            fmt_cost(r.session_total_cost_usd),
        ));
    }
    out.push_str("\n> ℹ️ '결과토큰(추정)'은 해당 도구가 반환한 결과 크기로 산출한 단독 추정 비용의 기초입니다.\n> ℹ️ '세션귀속' 토큰/비용은 해당 도구를 호출한 세션 전체 기준 집계이며, 중복계상(overlap)될 수 있습니다.");
    out
}

/// `get_tool_trend` 응답 포매터 — 날짜별 도구 평균 결과 토큰 추세
pub fn fmt_tool_trend(data: &[crate::db::ToolTrendRow], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## 도구 결과 토큰 시계열 추세\n데이터가 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## 도구 결과 토큰 시계열 추세{period}\n\n");
    out.push_str("| 날짜 | 도구명 | 평균 결과 토큰 | 호출 횟수 |\n");
    out.push_str("|---|---|---|---|\n");
    for r in data {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            r.date_bucket,
            r.tool_name,
            fmt_tokens(r.avg_result_est_tokens as u64),
            r.call_count,
        ));
    }
    out
}

/// `get_tool_offenders` 응답 포매터 — 결과가 가장 큰 도구 호출 Top-N
pub fn fmt_tool_offenders(data: &[crate::db::ToolOffenderRow], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## 도구 결과 오펜더 랭킹 (Top-N)\n데이터가 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## 도구 결과 오펜더 랭킹{period}\n\n");
    out.push_str("| 세션 ID | 도구명 | 일시 | 결과 글자수 | 결과 추정 토큰 |\n");
    out.push_str("|---|---|---|---|---|\n");
    for r in data {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} |\n",
            short_id(&r.session_id),
            r.tool_name,
            r.created_at,
            r.result_char_count,
            fmt_tokens(r.result_est_tokens as u64),
        ));
    }
    out
}

/// `get_tool_percentiles` 응답 포매터 — 백분위 분포
pub fn fmt_tool_percentiles(data: &[crate::db::ToolPercentileRow], since: Option<&str>) -> String {
    if data.is_empty() {
        return "## 도구 결과 백분위 분포\n데이터가 없습니다.".to_string();
    }
    let period = since.map(|s| format!(" *(since {s})*")).unwrap_or_default();
    let mut out = format!("## 도구 결과 백분위 분포{period}\n\n");
    out.push_str("| 도구명 | 호출수 | p50 토큰 | p90 토큰 | Max 토큰 |\n");
    out.push_str("|---|---|---|---|---|\n");
    for r in data {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            r.tool_name,
            r.call_count,
            fmt_tokens(r.p50_tokens as u64),
            fmt_tokens(r.p90_tokens as u64),
            fmt_tokens(r.max_tokens as u64),
        ));
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// server.rs와 공유하는 인라인 구조체 (serde용)
// ─────────────────────────────────────────────────────────────────────────────

/// 루프 의심 세션 행 (get_loop_suspects 내부 사용)
#[derive(serde::Serialize)]
pub struct LoopSession {
    pub session_id: String,
    pub agent_type: String,
    pub started_at: String,
    pub loop_tool_count: i64,
}

/// cwd 검색 결과 행 (search_sessions 내부 사용)
#[derive(serde::Serialize)]
pub struct SessionMatch {
    pub session_id: String,
    pub agent_type: String,
    pub cwd: String,
    pub model_id: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub started_at: String,
    pub total_cost_usd: f64,
}

