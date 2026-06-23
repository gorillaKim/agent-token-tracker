//! ccusage / rtk gain 교차검증 하니스 모듈 (이슈 #705)
//!
//! ATK(`report` 커맨드)의 토큰 합계를 `ccusage session --json` 출력과 대조하여
//! 허용오차 내 일치 여부를 자동 확인하고 불일치 차이를 리포트합니다.
//!
//! ## 측정 범위 차이 (알려진 caveat)
//! - **ccusage**: Claude Code 세션 로그 기반, cache_creation/cache_read 토큰 포함.
//! - **ATK**: 어댑터가 파싱한 messages.input_tokens + messages.output_tokens 합계.
//!   cache_read 토큰은 별도 컬럼(`cache_read_input_tokens`)에 보관하고 합계 제외.
//! - 따라서 **inputTokens 기준** 대조 시 ATK 값이 ccusage보다 낮게 나타남(캐시 읽기 제외).
//! - `totalTokens`(ccusage) = inputTokens + outputTokens + cacheCreationTokens + cacheReadTokens
//!   이므로 ATK 합계와 직접 비교 불가. 이 하니스는 **outputTokens 기준** 대조를 주 지표로 사용.
//! - **rtk gain**: rtk 자체는 토큰 계수 도구가 아님(LLM 출력 압축 프록시). 직접 gain
//!   수치(압축률)만 보고하며, ATK와 토큰 수 기준으로 대조하지 않음.

use serde::{Deserialize, Serialize};
use rusqlite::Connection;

// ────────────────────────────────────────────────────────────
// ccusage JSON 스키마 (session --json 출력 기준)
// ────────────────────────────────────────────────────────────

/// ccusage `session --json` 최상위 응답
#[derive(Debug, Deserialize)]
pub struct CcusageResponse {
    pub session: Vec<CcusageSession>,
}

/// ccusage 세션 항목
#[derive(Debug, Deserialize, Clone)]
pub struct CcusageSession {
    pub period: String,          // 세션 식별자 (UUID)
    pub agent: String,           // "claude" | "codex" 등
    #[serde(rename = "inputTokens")]
    pub input_tokens: u64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: u64,
    #[serde(rename = "cacheCreationTokens")]
    pub cache_creation_tokens: Option<u64>,
    #[serde(rename = "cacheReadTokens")]
    pub cache_read_tokens: Option<u64>,
    #[serde(rename = "totalCost")]
    pub total_cost: f64,
    #[serde(rename = "totalTokens")]
    pub total_tokens: Option<u64>,
}

// ────────────────────────────────────────────────────────────
// ATK 집계 (DB 기반)
// ────────────────────────────────────────────────────────────

/// ATK 세션별 집계 결과
#[derive(Debug, Clone)]
pub struct AtkSessionAgg {
    pub session_id: String,
    pub agent_type: String,
    pub input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// DB에서 세션별 메시지 합계를 집계합니다.
pub fn get_atk_session_agg(
    conn: &Connection,
    since: Option<&str>,
) -> Result<Vec<AtkSessionAgg>, rusqlite::Error> {
    let mut query = "
        SELECT s.session_id, s.agent_type,
               COALESCE(SUM(m.input_tokens), 0),
               COALESCE(SUM(m.cache_read_input_tokens), 0),
               COALESCE(SUM(m.output_tokens), 0),
               COALESCE(SUM(m.cost_usd), 0.0)
        FROM sessions s
        LEFT JOIN messages m ON s.session_id = m.session_id
        WHERE 1=1
    ".to_string();

    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(date_str) = since {
        query.push_str(" AND s.started_at >= ? ");
        params.push(rusqlite::types::Value::Text(date_str.to_string()));
    }
    query.push_str(" GROUP BY s.session_id, s.agent_type ORDER BY s.started_at DESC");

    let mut stmt = conn.prepare(&query)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let iter = stmt.query_map(&params_ref[..], |row| {
        Ok(AtkSessionAgg {
            session_id: row.get(0)?,
            agent_type: row.get(1)?,
            input_tokens: row.get::<_, u64>(2).unwrap_or(0),
            cache_read_input_tokens: row.get::<_, u64>(3).unwrap_or(0),
            output_tokens: row.get::<_, u64>(4).unwrap_or(0),
            cost_usd: row.get::<_, f64>(5).unwrap_or(0.0),
        })
    })?;

    let mut list = Vec::new();
    for item in iter { list.push(item?); }
    Ok(list)
}

// ────────────────────────────────────────────────────────────
// 교차검증 로직
// ────────────────────────────────────────────────────────────

/// 허용오차 설정
#[derive(Debug, Clone)]
pub struct Tolerance {
    /// 출력 토큰 허용 오차율 (기본 5%)
    pub output_token_rate: f64,
    /// 비용 허용 오차율 (기본 10% — 가격 스냅샷 불일치 감안)
    pub cost_rate: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            output_token_rate: 0.05,
            cost_rate: 0.10,
        }
    }
}

/// 세션 단위 불일치 항목
#[derive(Debug, Serialize)]
pub struct Discrepancy {
    pub session_id: String,            // ATK 세션 ID
    pub ccusage_period: Option<String>, // 대응되는 ccusage period (없을 수 있음)
    pub field: String,                  // "output_tokens" | "cost_usd"
    pub atk_value: f64,
    pub ccusage_value: f64,
    pub diff_abs: f64,
    pub diff_rate: f64,
    pub within_tolerance: bool,
}

/// 전체 교차검증 결과
#[derive(Debug, Serialize)]
pub struct CrossCheckReport {
    /// ATK 전체 합계
    pub atk_total_input: u64,
    pub atk_total_output: u64,
    pub atk_total_cost: f64,
    pub atk_total_cache_read: u64,
    /// ccusage 전체 합계
    pub ccusage_total_input: u64,
    pub ccusage_total_output: u64,
    pub ccusage_total_cost: f64,
    pub ccusage_total_cache_read: u64,
    /// 합계 레벨 편차
    pub output_diff_abs: i64,
    pub output_diff_rate: f64,
    pub cost_diff_abs: f64,
    pub cost_diff_rate: f64,
    /// 허용오차 통과 여부
    pub output_within_tolerance: bool,
    pub cost_within_tolerance: bool,
    /// 세션 단위 불일치 목록 (허용오차 초과만)
    pub discrepancies: Vec<Discrepancy>,
    /// caveat 메시지
    pub caveats: Vec<String>,
}

/// ccusage JSON 문자열을 파싱합니다.
pub fn parse_ccusage_json(raw: &str) -> Result<Vec<CcusageSession>, serde_json::Error> {
    // {"session": [...]} 형식 또는 배열 직접 파싱 모두 처리
    if let Ok(resp) = serde_json::from_str::<CcusageResponse>(raw) {
        return Ok(resp.session);
    }
    serde_json::from_str::<Vec<CcusageSession>>(raw)
}

/// ATK DB와 ccusage 세션 목록을 교차검증합니다.
///
/// ATK 세션 ID와 ccusage period(UUID)를 직접 매칭할 수 없으므로
/// **합계 레벨 대조**를 주 지표로 삼고, 세션별 대조는 보조 정보로 제공합니다.
pub fn cross_check(
    atk_sessions: &[AtkSessionAgg],
    ccusage_sessions: &[CcusageSession],
    tolerance: &Tolerance,
) -> CrossCheckReport {
    // ATK 합계
    let atk_total_input: u64  = atk_sessions.iter().map(|s| s.input_tokens).sum();
    let atk_total_output: u64 = atk_sessions.iter().map(|s| s.output_tokens).sum();
    let atk_total_cost: f64   = atk_sessions.iter().map(|s| s.cost_usd).sum();
    let atk_total_cache: u64  = atk_sessions.iter().map(|s| s.cache_read_input_tokens).sum();

    // ccusage 합계 (claude 에이전트만)
    let ccusage_claude: Vec<&CcusageSession> = ccusage_sessions.iter()
        .filter(|s| s.agent == "claude")
        .collect();

    let ccusage_total_input: u64 = ccusage_claude.iter().map(|s| s.input_tokens).sum();
    let ccusage_total_output: u64= ccusage_claude.iter().map(|s| s.output_tokens).sum();
    let ccusage_total_cost: f64  = ccusage_claude.iter().map(|s| s.total_cost).sum();
    let ccusage_total_cache: u64 = ccusage_claude.iter().map(|s| s.cache_read_tokens.unwrap_or(0)).sum();

    // 합계 레벨 편차 계산
    let output_diff_abs = atk_total_output as i64 - ccusage_total_output as i64;
    let output_diff_rate = if ccusage_total_output > 0 {
        output_diff_abs.abs() as f64 / ccusage_total_output as f64
    } else {
        0.0
    };

    let cost_diff_abs = atk_total_cost - ccusage_total_cost;
    let cost_diff_rate = if ccusage_total_cost > 0.0 {
        cost_diff_abs.abs() / ccusage_total_cost
    } else {
        0.0
    };

    let output_within_tolerance = output_diff_rate <= tolerance.output_token_rate;
    let cost_within_tolerance   = cost_diff_rate   <= tolerance.cost_rate;

    // Caveat 메시지 수집
    let mut caveats = vec![
        "ATK input_tokens는 cache_read_input_tokens를 별도 컬럼에 보관하며 합계에서 제외합니다. \
         ccusage inputTokens와 직접 비교 시 ATK 값이 낮게 나타나는 것은 정상입니다.".to_string(),
        "ATK cost_usd는 messages 적재 시점의 가격 스냅샷(pricing.rs) 기준입니다. \
         ccusage totalCost는 실시간 API 가격 기준이므로 소폭 차이가 발생할 수 있습니다.".to_string(),
        "truncation/중단된 세션은 양쪽 합계 모두에서 누락될 수 있습니다.".to_string(),
        format!(
            "비교 대상: ATK {} 세션 ↔ ccusage {} 세션(claude)",
            atk_sessions.len(), ccusage_claude.len()
        ),
    ];

    if atk_sessions.len() != ccusage_claude.len() {
        caveats.push(format!(
            "⚠  세션 수 불일치 — ATK: {}, ccusage(claude): {}. \
             scan 범위(경로/날짜)가 다르거나 일부 세션이 ATK에 미적재된 경우입니다.",
            atk_sessions.len(), ccusage_claude.len()
        ));
    }

    CrossCheckReport {
        atk_total_input,
        atk_total_output,
        atk_total_cost,
        atk_total_cache_read: atk_total_cache,
        ccusage_total_input,
        ccusage_total_output,
        ccusage_total_cost,
        ccusage_total_cache_read: ccusage_total_cache,
        output_diff_abs,
        output_diff_rate,
        cost_diff_abs,
        cost_diff_rate,
        output_within_tolerance,
        cost_within_tolerance,
        discrepancies: Vec::new(), // 합계 레벨 단순화 — 세션 매핑 불가
        caveats,
    }
}

/// 교차검증 결과를 아스키 테이블로 출력합니다.
pub fn print_report(report: &CrossCheckReport, tolerance: &Tolerance) {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║          ATK ↔ ccusage 교차검증 결과 (합계 레벨)               ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ 항목                     ATK              ccusage          차이 ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ input tokens   {:>15}   {:>15}   {:>+15} ║",
        fmt_tok(report.atk_total_input),
        fmt_tok(report.ccusage_total_input),
        report.atk_total_input as i64 - report.ccusage_total_input as i64,
    );
    println!("║ output tokens  {:>15}   {:>15}   {:>+15} ║",
        fmt_tok(report.atk_total_output),
        fmt_tok(report.ccusage_total_output),
        report.output_diff_abs,
    );
    println!("║ cache_read tok {:>15}   {:>15}   {:>+15} ║",
        fmt_tok(report.atk_total_cache_read),
        fmt_tok(report.ccusage_total_cache_read),
        report.atk_total_cache_read as i64 - report.ccusage_total_cache_read as i64,
    );
    println!("║ cost (USD)     {:>15.4}   {:>15.4}   {:>+15.4} ║",
        report.atk_total_cost,
        report.ccusage_total_cost,
        report.cost_diff_abs,
    );
    println!("╠══════════════════════════════════════════════════════════════════╣");

    // 판정
    let tok_icon = if report.output_within_tolerance { "✅" } else { "❌" };
    let cost_icon = if report.cost_within_tolerance { "✅" } else { "❌" };
    println!("║ output 오차율: {:>6.2}% (허용 {:.0}%)  {}                        ║",
        report.output_diff_rate * 100.0, tolerance.output_token_rate * 100.0, tok_icon,
    );
    println!("║ cost   오차율: {:>6.2}% (허용 {:.0}%)  {}                        ║",
        report.cost_diff_rate * 100.0, tolerance.cost_rate * 100.0, cost_icon,
    );
    println!("╚══════════════════════════════════════════════════════════════════╝");

    println!();
    println!("📋 측정 신뢰도 Caveat:");
    for caveat in &report.caveats {
        println!("  • {}", caveat);
    }
    println!();
}

fn fmt_tok(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

// ────────────────────────────────────────────────────────────
// 단위 테스트
// ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_atk_sessions() -> Vec<AtkSessionAgg> {
        vec![
            AtkSessionAgg {
                session_id: "sess-001".into(),
                agent_type: "claude_code".into(),
                input_tokens: 10_000,
                cache_read_input_tokens: 50_000,
                output_tokens: 5_000,
                cost_usd: 0.50,
            },
            AtkSessionAgg {
                session_id: "sess-002".into(),
                agent_type: "claude_code".into(),
                input_tokens: 8_000,
                cache_read_input_tokens: 20_000,
                output_tokens: 3_000,
                cost_usd: 0.30,
            },
        ]
    }

    fn make_ccusage_sessions() -> Vec<CcusageSession> {
        vec![
            CcusageSession {
                period: "period-001".into(),
                agent: "claude".into(),
                input_tokens: 10_200,
                output_tokens: 5_100,
                cache_creation_tokens: Some(2_000),
                cache_read_tokens: Some(50_000),
                total_cost: 0.52,
                total_tokens: Some(67_300),
            },
            CcusageSession {
                period: "period-002".into(),
                agent: "claude".into(),
                input_tokens: 8_100,
                output_tokens: 3_050,
                cache_creation_tokens: Some(1_000),
                cache_read_tokens: Some(20_000),
                total_cost: 0.31,
                total_tokens: Some(32_150),
            },
        ]
    }

    #[test]
    fn test_parse_ccusage_json_response_format() {
        let json = r#"{"session":[{"period":"abc","agent":"claude","inputTokens":1000,"outputTokens":500,"totalCost":0.05}]}"#;
        let sessions = parse_ccusage_json(json).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].period, "abc");
        assert_eq!(sessions[0].input_tokens, 1000);
        assert_eq!(sessions[0].output_tokens, 500);
    }

    #[test]
    fn test_parse_ccusage_json_array_format() {
        let json = r#"[{"period":"xyz","agent":"claude","inputTokens":500,"outputTokens":200,"totalCost":0.02}]"#;
        let sessions = parse_ccusage_json(json).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].agent, "claude");
    }

    #[test]
    fn test_cross_check_within_tolerance() {
        let atk = make_atk_sessions();
        let ccusage = make_ccusage_sessions();
        let tol = Tolerance::default();
        let report = cross_check(&atk, &ccusage, &tol);

        // ATK output = 8000, ccusage output = 8150 → 차이 ~1.8% < 5%
        assert_eq!(report.atk_total_output, 8_000);
        assert_eq!(report.ccusage_total_output, 8_150);
        assert!(report.output_within_tolerance, "output 토큰 허용오차 내여야 함");

        // 비용: ATK $0.80, ccusage $0.83 → 차이 ~3.6% < 10%
        assert!((report.atk_total_cost - 0.80).abs() < 0.001);
        assert!(report.cost_within_tolerance, "비용 허용오차 내여야 함");
    }

    #[test]
    fn test_cross_check_exceeds_tolerance() {
        let atk = vec![AtkSessionAgg {
            session_id: "sess-x".into(),
            agent_type: "claude_code".into(),
            input_tokens: 1_000,
            cache_read_input_tokens: 0,
            output_tokens: 1_000,  // ATK가 많이 낮음
            cost_usd: 0.10,
        }];
        let ccusage = vec![CcusageSession {
            period: "p-x".into(),
            agent: "claude".into(),
            input_tokens: 1_500,
            output_tokens: 2_000,  // ccusage가 훨씬 높음
            cache_creation_tokens: None,
            cache_read_tokens: None,
            total_cost: 0.25,
            total_tokens: None,
        }];
        let tol = Tolerance::default();
        let report = cross_check(&atk, &ccusage, &tol);

        // 50% 차이 → 허용오차(5%) 초과
        assert!(!report.output_within_tolerance, "50% 차이는 허용오차 초과여야 함");
    }

    #[test]
    fn test_cross_check_caveats_include_session_count_mismatch() {
        let atk = make_atk_sessions(); // 2개
        let ccusage = vec![make_ccusage_sessions()[0].clone()]; // 1개
        let tol = Tolerance::default();
        let report = cross_check(&atk, &ccusage, &tol);

        let has_mismatch_caveat = report.caveats.iter().any(|c| c.contains("세션 수 불일치"));
        assert!(has_mismatch_caveat, "세션 수 불일치 caveat 포함 필요");
    }

    #[test]
    fn test_fmt_tok() {
        assert_eq!(fmt_tok(500), "500");
        assert_eq!(fmt_tok(12_500), "12.5K");
        assert_eq!(fmt_tok(2_500_000), "2.50M");
    }
}
