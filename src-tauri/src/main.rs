#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Serialize, Deserialize};
use rusqlite::Connection;
use agent_token_tracker::model::Session;
use agent_token_tracker::db;
use agent_token_tracker::detect::loops::{detect_session_anomalies, DetectorConfig, LoopDetectionResult};

// ────────────────────────────────────────────────────────────
// 프론트엔드 연동용 직렬화 구조체 정의
// ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSummary {
    pub agent_type: String,
    pub session_count: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

// ────────────────────────────────────────────────────────────
// 헬퍼: 데이터베이스 커넥션 획득
// ────────────────────────────────────────────────────────────
fn get_db_conn() -> Result<Connection, String> {
    // 로컬 작업 경로 내의 atk.db 커넥션 연결
    db::init_db("../atk.db").map_err(|e| format!("DB 연결 실패: {}", e))
}

// ════════════════════════════════════════════════════════════
// Tauri IPC Commands 구현
// ════════════════════════════════════════════════════════════

/// 1. 모든 세션 목록 획득
#[tauri::command]
fn get_active_sessions() -> Result<Vec<Session>, String> {
    let conn = get_db_conn()?;
    let sessions = db::get_all_sessions(&conn)
        .map_err(|e| format!("세션 로드 에러: {}", e))?;
    Ok(sessions)
}

/// 2. 에이전트별 토큰 및 비용 요약 집계
#[tauri::command]
fn get_agent_summaries() -> Result<Vec<AgentSummary>, String> {
    let conn = get_db_conn()?;
    let sessions = db::get_all_sessions(&conn)
        .map_err(|e| format!("세션 로드 에러: {}", e))?;

    let mut cc_sum = AgentSummary {
        agent_type: "claude_code".to_string(),
        session_count: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cost_usd: 0.0,
    };
    let mut cdx_sum = AgentSummary {
        agent_type: "codex".to_string(),
        session_count: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cost_usd: 0.0,
    };
    let mut agy_sum = AgentSummary {
        agent_type: "antigravity".to_string(),
        session_count: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cost_usd: 0.0,
    };

    for s in sessions {
        let msgs = db::get_messages_by_session(&conn, &s.session_id)
            .unwrap_or_default();
        let cost: f64 = msgs.iter().map(|m| m.cost_usd).sum();

        match s.agent_type.as_str() {
            "claude_code" => {
                cc_sum.session_count += 1;
                cc_sum.total_input_tokens += s.total_input_tokens;
                cc_sum.total_output_tokens += s.total_output_tokens;
                cc_sum.total_cost_usd += cost;
            }
            "codex" => {
                cdx_sum.session_count += 1;
                cdx_sum.total_input_tokens += s.total_input_tokens;
                cdx_sum.total_output_tokens += s.total_output_tokens;
                cdx_sum.total_cost_usd += cost;
            }
            "antigravity" => {
                agy_sum.session_count += 1;
                // Antigravity는 토큰 실측 불가능하므로 0 유지
                agy_sum.total_cost_usd += cost;
            }
            _ => {}
        }
    }

    Ok(vec![cc_sum, cdx_sum, agy_sum])
}

/// 3. 탐지된 모든 이상 징후 세션 리스트 반환
#[tauri::command]
fn get_loop_signals() -> Result<Vec<LoopDetectionResult>, String> {
    let conn = get_db_conn()?;
    let sessions = db::get_all_sessions(&conn)
        .map_err(|e| format!("세션 로드 에러: {}", e))?;

    let mut anomalies = Vec::new();
    let config = DetectorConfig::default();

    for s in sessions {
        let msgs = db::get_messages_by_session(&conn, &s.session_id)
            .unwrap_or_default();
        let tool_calls = db::get_tool_calls_by_session(&conn, &s.session_id)
            .unwrap_or_default();

        let detect_res = detect_session_anomalies(&s, &msgs, &tool_calls, &config);
        if detect_res.is_anomaly {
            anomalies.push(detect_res);
        }
    }

    Ok(anomalies)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_active_sessions,
            get_agent_summaries,
            get_loop_signals
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 구동 중 에러 발생");
}
