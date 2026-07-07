//! MCP 응답 Markdown 포매터
//!
//! 에이전트가 읽기 쉬운 컴팩트한 Markdown 형식으로 응답을 생성합니다.
//! engram MCP 서버와 동일한 응답 철학을 따릅니다.

use crate::model::{AgentReport, SessionReport, ToolReport, McpServerReport, McpToolDetailReport, MalfunctionPattern, MalfunctionReport};

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

/// `get_malfunction_patterns` 응답 포매터 — 전체 오작동 패턴 목록
pub fn fmt_malfunction_patterns(data: &[MalfunctionPattern]) -> String {
    if data.is_empty() {
        return "## 🔍 오작동 감지 패턴\n등록된 오작동 감지 패턴이 없습니다.".to_string();
    }
    let mut out = "## 🔍 등록된 오작동 감지 패턴 목록\n\n".to_string();
    out.push_str("| ID | 패턴명 | 설명 | 규칙 요약 | 등록 시간 |\n");
    out.push_str("|---|---|---|---|---|\n");
    for p in data {
        let desc = p.description.as_deref().unwrap_or("—");
        let rules_summary = if p.rules_json.len() > 50 {
            format!("{}...", &p.rules_json[..47])
        } else {
            p.rules_json.clone()
        };
        out.push_str(&format!(
            "| {} | **{}** | {} | `{}` | {} |\n",
            p.id,
            p.pattern_name,
            desc,
            rules_summary,
            p.created_at,
        ));
    }
    out
}

/// `get_session_malfunctions` 및 `analyze_session_malfunctions` 응답 포매터 — 세션의 오작동 감지 이력
/// `get_session_malfunctions` 및 `analyze_session_malfunctions` 응답 포매터 — 세션의 오작동 감지 이력
pub fn fmt_malfunction_detections(session_id: &str, data: &[MalfunctionReport]) -> String {
    if data.is_empty() {
        return format!("## ✅ 세션 `{}` 오작동 감지 결과\n감지된 오작동 패턴이 없습니다.", short_id(session_id));
    }
    let mut out = format!("## ⚠️ 세션 `{}` 오작동 감지 이력 ({}건)\n\n", short_id(session_id), data.len());
    out.push_str("| ID | 패턴명 | 설명 | 상태 | 상세 증거 (Evidence) | 감지 시각 |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for r in data {
        let desc = r.description.as_deref().unwrap_or("—");
        let status = if r.is_false_positive { "해제됨 (FP) 🟢" } else { "감지됨 🔴" };
        out.push_str(&format!(
            "| {} | **{}** | {} | {} | {} | {} |\n",
            r.id,
            r.pattern_name,
            desc,
            status,
            r.evidence,
            r.detected_at,
        ));
    }
    out
}

/// `get_malfunction_detections` 응답 포매터 — 다차원 필터링 감지 이력 목록
pub fn fmt_malfunction_detections_v2(data: &[MalfunctionReport]) -> String {
    if data.is_empty() {
        return "## 🔍 오작동 감지 이력\n조건에 매칭되는 오작동 감지 이력이 없습니다.".to_string();
    }
    let mut out = format!("## ⚠️ 오작동 감지 이력 목록 (총 {}건)\n\n", data.len());
    out.push_str("| ID | 세션 ID | 패턴명 | 상태 | 상세 증거 (Evidence) | 감지 시각 |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for r in data {
        let status = if r.is_false_positive { "해제됨 (FP) 🟢" } else { "감지됨 🔴" };
        out.push_str(&format!(
            "| {} | `{}` | **{}** | {} | {} | {} |\n",
            r.id,
            short_id(&r.session_id),
            r.pattern_name,
            status,
            r.evidence,
            r.detected_at,
        ));
    }
    out
}

/// `get_malfunction_summary` 응답 포매터 — 패턴별 집계 리포트
pub fn fmt_malfunction_summary(data: &[crate::db::MalfunctionSummary]) -> String {
    if data.is_empty() {
        return "## 📊 오작동 패턴별 요약\n감지된 통계 데이터가 없습니다.".to_string();
    }
    let mut out = "## 📊 오작동 패턴별 집계 요약\n\n".to_string();
    out.push_str("| 패턴 ID | 패턴명 | 설명 | 매칭 세션 수 | 누적 감지 건수 | 해제 건수 (FP) | 최초 감지 | 최근 감지 | 최근 7일 추세 |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for s in data {
        let desc = s.description.as_deref().unwrap_or("—");
        let first = s.first_detected.as_deref().unwrap_or("—");
        let last = s.last_detected.as_deref().unwrap_or("—");
        out.push_str(&format!(
            "| {} | **{}** | {} | {} | {} | {} | {} | {} | `{}` |\n",
            s.pattern_id,
            s.pattern_name,
            desc,
            s.matching_sessions,
            s.detection_count,
            s.false_positive_count,
            first,
            last,
            s.recent_trend,
        ));
    }
    out
}

/// `get_session_detail` 응답 포매터 — 세션의 입체적 상세 이력
pub fn fmt_session_detail(ctx: &crate::detect::malfunctions::SessionMalfunctionContext) -> String {
    let mut out = String::new();
    
    // 1. 세션 기본 정보
    let ended = ctx.session.ended_at.as_deref().unwrap_or("진행 중 (또는 비정상 종료 ⚠️)");
    let unexpected = if ctx.session.ended_at.is_none() { "Yes ⚠️" } else { "No" };
    let model = ctx.session.model_id.as_deref().unwrap_or("—");
    
    out.push_str(&format!(
        "## 🔍 세션 상세 분석: `{}`\n\n",
        ctx.session.session_id
    ));
    
    out.push_str("### 📋 메타 데이터\n");
    out.push_str(&format!("* **에이전트**: `{}`\n", ctx.session.agent_type));
    out.push_str(&format!("* **모델 ID**: `{}`\n", model));
    out.push_str(&format!("* **작업 디렉토리**: `{}`\n", ctx.session.cwd));
    out.push_str(&format!("* **시작 일시**: `{}`\n", ctx.session.started_at));
    out.push_str(&format!("* **종료 일시**: `{}`\n", ended));
    out.push_str(&format!("* **예상치 못한 종료 (Unexpected Exit)**: `{}`\n\n", unexpected));

    // 2. 누적 집계 (Totals)
    let total_cost = ctx.messages.iter().map(|m| m.cost_usd).sum::<f64>();
    let turn_count = ctx.messages.len();
    out.push_str("### 📊 누적 통계\n");
    out.push_str(&format!("* **입력 토큰**: `{}`\n", fmt_tokens(ctx.session.total_input_tokens)));
    out.push_str(&format!("* **출력 토큰**: `{}`\n", fmt_tokens(ctx.session.total_output_tokens)));
    out.push_str(&format!("* **총 추정 비용**: `{}`\n", fmt_cost(total_cost)));
    out.push_str(&format!("* **총 도구 호출 수**: `{}`\n", ctx.tool_calls.len()));
    out.push_str(&format!("* **총 대화 턴 수**: `{}`\n\n", turn_count));

    // 3. 도구 호출 빈도표 (tool_frequency)
    use std::collections::HashMap;
    let mut tool_freq: HashMap<String, u64> = HashMap::new();
    for tc in &ctx.tool_calls {
        *tool_freq.entry(tc.tool_name.clone()).or_insert(0) += 1;
    }
    let mut sorted_freq: Vec<(String, u64)> = tool_freq.into_iter().collect();
    sorted_freq.sort_by(|a, b| b.1.cmp(&a.1));

    out.push_str("### 🛠️ 도구 호출 빈도 (상위 N)\n");
    if sorted_freq.is_empty() {
        out.push_str("호출된 도구가 없습니다.\n\n");
    } else {
        out.push_str("| 도구명 | 호출 수 |\n");
        out.push_str("|---|---|\n");
        for (name, count) in sorted_freq.iter().take(10) {
            out.push_str(&format!("| `{}` | {} |\n", name, count));
        }
        out.push_str("\n");
    }

    // 4. MCP 서버 빈도표 (mcp_server_frequency)
    out.push_str("### 🔌 MCP 서버 호출 빈도\n");
    if ctx.mcp_server_counts.is_empty() {
        out.push_str("호출된 MCP 서버가 없습니다.\n\n");
    } else {
        let mut sorted_mcp: Vec<(String, usize)> = ctx.mcp_server_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        sorted_mcp.sort_by(|a, b| b.1.cmp(&a.1));
        out.push_str("| MCP 서버 | 호출 수 |\n");
        out.push_str("|---|---|\n");
        for (server, count) in sorted_mcp {
            out.push_str(&format!("| `{}` | {} |\n", server, count));
        }
        out.push_str("\n");
    }

    // 5. 엔진 권위의 루프 시그널 정보 (loop_signals)
    let mut max_tool_fail_cnt = 0;
    let mut max_tool_fail_name = "—".to_string();
    for (tname, cnt) in &ctx.tool_consecutive_failures_map {
        if *cnt > max_tool_fail_cnt {
            max_tool_fail_cnt = *cnt;
            max_tool_fail_name = tname.clone();
        }
    }
    let fail_detail = if max_tool_fail_cnt > 0 {
        format!("{}회 연속 실패 (도구: `{}`)", max_tool_fail_cnt, max_tool_fail_name)
    } else {
        "0회".to_string()
    };

    out.push_str("### 🚨 오작동 및 루프 시그널 (ATK 권위값)\n");
    out.push_str(&format!("* **동일 도구 & 인풋 최대 연속 반복 (`dynamic_repeated_calls`)**: `{}회`\n", ctx.max_repeated_calls));
    out.push_str(&format!("* **최대 핑퐁 사이클 반복 (`dynamic_ping_pong`)**: `{}회`\n", ctx.max_ping_pong_cycles));
    out.push_str(&format!("* **최대 순환 루프 반복 (`dynamic_cyclic_loop`)**: `{}회`\n", ctx.max_cyclic_loop_cycles));
    out.push_str(&format!("* **최대 도구 연속 실패 (`max_consecutive_tool_failures`)**: `{}`\n", fail_detail));
    out.push_str(&format!("* **한 턴 내 최대 도구 호출 횟수 (`max_tool_calls_per_turn`)**: `{}회`\n\n", ctx.max_tool_calls_per_turn));

    // 6. 결과 페이로드 오펜더 (top_result_offenders)
    let mut offenders: Vec<&crate::model::ToolCall> = ctx.tool_calls.iter()
        .filter(|tc| tc.result_est_tokens.is_some())
        .collect();
    offenders.sort_by(|a, b| b.result_est_tokens.unwrap_or(0).cmp(&a.result_est_tokens.unwrap_or(0)));

    out.push_str("### 📦 결과 페이로드 오펜더 (Top 5)\n");
    if offenders.is_empty() {
        out.push_str("반환된 결과 페이로드 데이터가 없습니다.\n");
    } else {
        out.push_str("| 도구명 | 일시 | 결과 글자 수 | 추정 토큰 수 |\n");
        out.push_str("|---|---|---|---|\n");
        for tc in offenders.iter().take(5) {
            let chars = tc.result_char_count.unwrap_or(0);
            let tokens = tc.result_est_tokens.unwrap_or(0);
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                tc.tool_name,
                tc.created_at,
                fmt_tokens(chars as u64),
                fmt_tokens(tokens as u64)
            ));
        }
    }
    
    out
}


