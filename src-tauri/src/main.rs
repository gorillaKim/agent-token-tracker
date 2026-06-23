#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Serialize, Deserialize};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use notify::{Watcher, RecursiveMode};
use keyring::Entry;

use agent_token_tracker::model::Session;
use agent_token_tracker::db;
use agent_token_tracker::pricing;
use agent_token_tracker::detect::loops::{detect_session_anomalies, DetectorConfig, LoopDetectionResult};
use agent_token_tracker::adapters::{
    LogAdapter,
    claude_code::ClaudeCodeAdapter,
    codex::CodexAdapter,
    antigravity::AntigravityAdapter,
};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyCost {
    pub date: String,
    pub total_cost: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionDetails {
    pub messages: Vec<agent_token_tracker::model::Message>,
    pub tool_calls: Vec<agent_token_tracker::model::ToolCall>,
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

/// 4. 최근 14일간의 일별 비용 집계
#[tauri::command]
fn get_daily_costs() -> Result<Vec<DailyCost>, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "WITH RECURSIVE dates(date) AS (
            SELECT date('now', '-13 day')
            UNION ALL
            SELECT date(date, '+1 day') FROM dates WHERE date < date('now')
         )
         SELECT 
            d.date, 
            COALESCE(SUM(m.cost_usd), 0.0) as total_cost
         FROM dates d
         LEFT JOIN messages m ON date(m.created_at) = d.date
         GROUP BY d.date
         ORDER BY d.date ASC;"
    ).map_err(|e| format!("SQL 쿼리 준비 에러: {}", e))?;

    let rows = stmt.query_map([], |row| {
        Ok(DailyCost {
            date: row.get(0)?,
            total_cost: row.get(1)?,
        })
    }).map_err(|e| format!("쿼리 실행 에러: {}", e))?;

    let mut daily_costs = Vec::new();
    for r in rows {
        daily_costs.push(r.map_err(|e| format!("데이터 매핑 에러: {}", e))?);
    }

    Ok(daily_costs)
}

// ────────────────────────────────────────────────────────────
// 파일 실시간 감시 (notify) 백엔드 로직 구현
// ────────────────────────────────────────────────────────────

fn process_watch_file(
    file_path: &Path,
    pricing_cache: &HashMap<String, agent_token_tracker::model::Pricing>,
    db_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = file_path.to_str().unwrap_or("");
    let is_vscdb = path_str.contains("state.vscdb");
    let has_vscdb_param = path_str.contains("state.vscdb?session_id=");

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "jsonl" && !is_vscdb && !has_vscdb_param {
        return Ok(());
    }

    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let is_codex = file_name.starts_with("rollout-");
    let is_antigravity = is_vscdb || has_vscdb_param;

    let parsed_res = if is_antigravity {
        let adapter = AntigravityAdapter;
        adapter.parse_session(path_str)
    } else if is_codex {
        let adapter = CodexAdapter;
        adapter.parse_session(path_str)
    } else {
        let adapter = ClaudeCodeAdapter;
        adapter.parse_session(path_str)
    };

    let mut parsed_session = parsed_res?;

    // 비용 계산
    let model_id_opt = parsed_session.session.model_id.as_deref().unwrap_or("unknown");
    let pricing_info = parsed_session.session.model_id.as_ref()
        .and_then(|m_id| pricing_cache.get(m_id));

    for msg in &mut parsed_session.messages {
        if msg.role == "assistant" {
            msg.cost_usd = pricing::calculate_cost_usd(
                pricing_info,
                msg.input_tokens,
                msg.cache_read_input_tokens,
                msg.output_tokens,
            );
        }
    }

    // DB 갱신 (기존 세션 CASCADE 삭제 후 재생성)
    let conn = Connection::open(db_path)?;
    let _ = conn.pragma_update(None, "foreign_keys", "ON");
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "busy_timeout", &5000);

    db::delete_session(&conn, &parsed_session.session.session_id)?;
    db::insert_session(&conn, &parsed_session.session)?;
    for msg in &parsed_session.messages {
        db::insert_message(&conn, msg)?;
    }
    for node in &parsed_session.nodes {
        db::insert_node(&conn, node)?;
    }
    for tc in &parsed_session.tool_calls {
        db::insert_tool_call(&conn, tc)?;
    }

    Ok(())
}

fn start_watch_loop(app_handle: AppHandle) -> Result<(), String> {
    let db_path = "../atk.db";
    let watch_path = "../tests/fixtures";

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    }).map_err(|e| format!("파일 감시자 생성 실패: {}", e))?;

    let target_dir = Path::new(watch_path);
    if !target_dir.exists() {
        return Err(format!("감시 경로가 존재하지 않습니다: {}", watch_path));
    }

    watcher.watch(target_dir, RecursiveMode::Recursive)
        .map_err(|e| format!("파일 감시 시작 실패: {}", e))?;

    println!("[Watch] Tauri 백그라운드 파일 감시 시작: {}", watch_path);

    let mut last_event_time = Instant::now();
    let mut pending_files = HashSet::new();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                for p in event.paths {
                    if p.is_file() {
                        pending_files.insert(p);
                    }
                }
                last_event_time = Instant::now();
            }
            Err(e) => {
                let err_str = format!("{:?}", e);
                if err_str.contains("Timeout") {
                    if !pending_files.is_empty() && last_event_time.elapsed() >= Duration::from_millis(500) {
                        println!("[Watch] 감시 대상 파일 수정 감지, 증분 갱신 및 UI 업데이트 중...");
                        
                        let conn = match Connection::open(db_path) {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("DB 연결 실패: {}", e);
                                pending_files.clear();
                                continue;
                            }
                        };

                        let pricing_map = match db::get_all_pricings(&conn) {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("Pricing 데이터 조회 실패: {}", e);
                                pending_files.clear();
                                continue;
                            }
                        };

                        let mut updated_any = false;
                        for file in pending_files.drain() {
                            let path_str = file.to_str().unwrap_or("");
                            let is_vscdb = path_str.contains("state.vscdb");
                            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
                            
                            if ext != "jsonl" && !is_vscdb {
                                continue;
                            }

                            if is_vscdb {
                                if let Ok(ids) = agent_token_tracker::adapters::antigravity::get_vscdb_session_ids(path_str) {
                                    for id in ids {
                                        let virtual_path_str = format!("{}?session_id={}", path_str, id);
                                        let virtual_path = PathBuf::from(virtual_path_str);
                                        if let Err(e) = process_watch_file(&virtual_path, &pricing_map, db_path) {
                                            eprintln!("vscdb 파일 적재 중 에러: {}", e);
                                        } else {
                                            updated_any = true;
                                        }
                                    }
                                }
                            } else {
                                if let Err(e) = process_watch_file(&file, &pricing_map, db_path) {
                                    eprintln!("JSONL 파일 적재 중 에러: {}", e);
                                } else {
                                    updated_any = true;
                                }
                            }
                        }

                        if updated_any {
                            println!("[Watch] 증분 갱신 완료. 프론트엔드로 db-updated 이벤트 전송!");
                            update_tray_status(&app_handle);
                            if let Err(e) = app_handle.emit("db-updated", ()) {
                                eprintln!("Tauri 이벤트 방출 실패: {}", e);
                            }
                        }
                    }
                } else if err_str.contains("Disconnected") {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// 5. 세션 상세 정보 획득 (메시지 및 도구 호출 목록)
#[tauri::command]
fn get_session_details(session_id: String) -> Result<SessionDetails, String> {
    let conn = get_db_conn()?;
    let messages = db::get_messages_by_session(&conn, &session_id)
        .map_err(|e| format!("메시지 조회 실패: {}", e))?;
    let tool_calls = db::get_tool_calls_by_session(&conn, &session_id)
        .map_err(|e| format!("도구 호출 조회 실패: {}", e))?;
    Ok(SessionDetails { messages, tool_calls })
}

/// 6. 실행 중인 오작동 에이전트 프로세스 강제 중단
#[tauri::command]
fn interrupt_agent(agent_type: String, _cwd: String) -> Result<String, String> {
    use std::process::Command as StdCommand;

    let proc_pattern = match agent_type.as_str() {
        "claude_code" => "claude-code",
        "codex" => "codex",
        "antigravity" => "antigravity",
        _ => return Err("알 수 없는 에이전트 타입입니다.".to_string()),
    };

    println!("[Interrupt] 에이전트 프로세스 종료 시도: {}", proc_pattern);

    // pgrep -f 패턴
    let output = StdCommand::new("pgrep")
        .args(["-f", proc_pattern])
        .output()
        .map_err(|e| format!("pgrep 실행 실패: {}", e))?;

    let pids_str = String::from_utf8_lossy(&output.stdout);
    let pids: Vec<&str> = pids_str.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

    if pids.is_empty() {
        return Ok("실행 중인 관련 에이전트 프로세스가 없습니다.".to_string());
    }

    let mut killed_count = 0;
    for pid in pids {
        let kill_res = StdCommand::new("kill")
            .args(["-9", pid])
            .status();
        if let Ok(status) = kill_res {
            if status.success() {
                killed_count += 1;
            }
        }
    }

    Ok(format!("{}개의 에이전트 프로세스(PID)를 강제 종료(Interrupt)했습니다.", killed_count))
}

fn get_today_cost_and_health(conn: &Connection) -> Result<(f64, bool), String> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM messages WHERE date(created_at) = date('now');"
    ).map_err(|e| e.to_string())?;
    
    let today_cost: f64 = stmt.query_row([], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let sessions = db::get_all_sessions(conn)
        .map_err(|e| format!("세션 로드 에러: {}", e))?;

    let config = DetectorConfig::default();
    let mut is_healthy = true;

    for s in sessions {
        let msgs = db::get_messages_by_session(conn, &s.session_id)
            .unwrap_or_default();
        let tool_calls = db::get_tool_calls_by_session(conn, &s.session_id)
            .unwrap_or_default();

        let detect_res = detect_session_anomalies(&s, &msgs, &tool_calls, &config);
        if detect_res.is_anomaly {
            is_healthy = false;
            break;
        }
    }

    Ok((today_cost, is_healthy))
}

fn update_tray_status(app_handle: &AppHandle) {
    let tray = match app_handle.tray_by_id("main-tray") {
        Some(t) => t,
        None => {
            eprintln!("[Tray] 트레이 아이콘을 찾을 수 없습니다.");
            return;
        }
    };

    let (cost, is_healthy) = match get_db_conn().and_then(|conn| get_today_cost_and_health(&conn)) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[Tray] 데이터베이스 조회 실패: {}", e);
            (0.0, true)
        }
    };

    let title = format!("${:.2}", cost);
    if let Err(e) = tray.set_title(Some(title)) {
        eprintln!("[Tray] 타이틀 설정 실패: {}", e);
    }

    let icon_bytes = if is_healthy {
        include_bytes!("../icons/icon_green.png") as &[u8]
    } else {
        include_bytes!("../icons/icon_red.png") as &[u8]
    };

    if let Ok(icon) = tauri::image::Image::from_bytes(icon_bytes) {
        if let Err(e) = tray.set_icon(Some(icon)) {
            eprintln!("[Tray] 아이콘 설정 실패: {}", e);
        }
    }
}

fn toggle_tray_popover(app: &AppHandle, click_pos: tauri::PhysicalPosition<f64>) {
    let window = match app.get_webview_window("tray-popover") {
        Some(w) => w,
        None => {
            eprintln!("[Tray] tray-popover 윈도우를 찾을 수 없습니다.");
            return;
        }
    };

    let is_visible = window.is_visible().unwrap_or(false);
    if is_visible {
        let _ = window.hide();
    } else {
        // macOS 상단 메뉴바 아래에 맞춰 위치 계산
        // 가로 중앙 맞춤: click_pos.x - (윈도우 너비 320 / 2)
        // 세로 위치: click_pos.y + 10px 마진
        let x = click_pos.x - 160.0;
        let y = click_pos.y + 10.0;
        
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: x as i32,
            y: y as i32,
        }));
        
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// 7. 팝오버 클릭 시 메인 윈도우 활성화 및 라우팅 연동
#[tauri::command]
fn focus_main_window(app_handle: AppHandle, session_id: Option<String>) -> Result<(), String> {
    if let Some(main_window) = app_handle.get_webview_window("main") {
        let _ = main_window.show();
        let _ = main_window.unminimize();
        let _ = main_window.set_focus();
        if let Some(id) = session_id {
            let _ = app_handle.emit("navigate-to-session", id);
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub log_dir: String,
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path().app_config_dir()
        .map_err(|e| format!("설정 디렉토리 경로 획득 실패: {}", e))?;
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("설정 디렉토리 생성 실패: {}", e))?;
    path.push("config.json");
    Ok(path)
}

#[tauri::command]
fn save_settings(app_handle: AppHandle, log_dir: String) -> Result<(), String> {
    let path = get_config_path(&app_handle)?;
    let settings = AppSettings { log_dir };
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("JSON 직렬화 실패: {}", e))?;
    std::fs::write(path, json)
        .map_err(|e| format!("설정 파일 쓰기 실패: {}", e))?;
    Ok(())
}

#[tauri::command]
fn load_settings(app_handle: AppHandle) -> Result<AppSettings, String> {
    let path = get_config_path(&app_handle)?;
    if !path.exists() {
        return Ok(AppSettings { log_dir: "".to_string() });
    }
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("설정 파일 읽기 실패: {}", e))?;
    let settings: AppSettings = serde_json::from_str(&json)
        .map_err(|e| format!("JSON 역직렬화 실패: {}", e))?;
    Ok(settings)
}

#[tauri::command]
fn save_api_key(provider: String, api_key: String) -> Result<(), String> {
    if provider != "anthropic" && provider != "openai" {
        return Err("지원하지 않는 플랫폼입니다.".to_string());
    }
    let entry = Entry::new("agent-token-tracker", &provider)
        .map_err(|e| format!("키체인 엔트리 생성 실패: {}", e))?;
    entry.set_password(&api_key)
        .map_err(|e| format!("API Key 저장 실패: {}", e))?;
    Ok(())
}

#[tauri::command]
fn delete_api_key(provider: String) -> Result<(), String> {
    if provider != "anthropic" && provider != "openai" {
        return Err("지원하지 않는 플랫폼입니다.".to_string());
    }
    let entry = Entry::new("agent-token-tracker", &provider)
        .map_err(|e| format!("키체인 엔트리 생성 실패: {}", e))?;
    let _ = entry.delete_credential();
    Ok(())
}

#[tauri::command]
fn get_api_keys_status() -> Result<HashMap<String, bool>, String> {
    let mut status = HashMap::new();
    
    let anthropic_entry = Entry::new("agent-token-tracker", "anthropic");
    let has_anthropic = match anthropic_entry {
        Ok(entry) => entry.get_password().is_ok(),
        Err(_) => false,
    };
    status.insert("anthropic".to_string(), has_anthropic);

    let openai_entry = Entry::new("agent-token-tracker", "openai");
    let has_openai = match openai_entry {
        Ok(entry) => entry.get_password().is_ok(),
        Err(_) => false,
    };
    status.insert("openai".to_string(), has_openai);

    Ok(status)
}

#[tauri::command]
async fn validate_stored_api_key(provider: String) -> Result<bool, String> {
    if provider != "anthropic" && provider != "openai" {
        return Err("지원하지 않는 플랫폼입니다.".to_string());
    }
    
    let entry = Entry::new("agent-token-tracker", &provider)
        .map_err(|e| format!("키체인 조회 실패: {}", e))?;
    
    let api_key = match entry.get_password() {
        Ok(k) => k,
        Err(_) => return Ok(false),
    };

    let client = reqwest::Client::new();
    
    if provider == "anthropic" {
        let response = client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "claude-3-5-sonnet-20241022",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "ping"}]
            }))
            .send()
            .await;

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status == 401 {
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
            Err(_) => Err("Anthropic API 서버에 접근할 수 없습니다.".to_string()),
        }
    } else {
        let response = client.get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status == 200 {
                    Ok(true)
                } else if status == 401 {
                    Ok(false)
                } else {
                    Ok(false)
                }
            }
            Err(_) => Err("OpenAI API 서버에 접근할 수 없습니다.".to_string()),
        }
    }
}

#[tauri::command]
fn validate_local_path(path: String) -> Result<bool, String> {
    let p = Path::new(&path);
    Ok(p.exists() && p.is_dir())
}

fn collect_files_helper(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                collect_files_helper(&path, files)?;
            } else {
                files.push(path);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResult {
    pub files_total: usize,
    pub sessions_inserted: usize,
    pub sessions_skipped: usize,
    pub sessions_failed: usize,
}

#[tauri::command]
async fn sync_local_sessions(app_handle: AppHandle) -> Result<SyncResult, String> {
    let db_path = "../atk.db";
    
    // 1. 로드 설정 경로
    let config_path = get_config_path(&app_handle)?;
    let mut log_dir = "../tests/fixtures".to_string();
    if config_path.exists() {
        if let Ok(json) = std::fs::read_to_string(config_path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&json) {
                if !settings.log_dir.is_empty() {
                    log_dir = settings.log_dir;
                }
            }
        }
    }
    
    let target_dir = Path::new(&log_dir);
    if !target_dir.exists() || !target_dir.is_dir() {
        return Err(format!("로그 디렉토리가 존재하지 않거나 폴더가 아닙니다: {}", log_dir));
    }
    
    let mut files = Vec::new();
    collect_files_helper(target_dir, &mut files)?;
    
    let conn = Connection::open(db_path)
        .map_err(|e| format!("DB 연결 실패: {}", e))?;
    let pricing_map = db::get_all_pricings(&conn)
        .map_err(|e| format!("단가 로드 실패: {}", e))?;
    
    let mut result = SyncResult {
        files_total: files.len(),
        sessions_inserted: 0,
        sessions_skipped: 0,
        sessions_failed: 0,
    };
    
    for file in files {
        let path_str = file.to_str().unwrap_or("");
        let is_vscdb = path_str.contains("state.vscdb");
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        if ext != "jsonl" && !is_vscdb {
            continue;
        }
        
        if is_vscdb {
            if let Ok(ids) = agent_token_tracker::adapters::antigravity::get_vscdb_session_ids(path_str) {
                for id in ids {
                    if db::get_session(&conn, &id).is_ok() {
                        result.sessions_skipped += 1;
                        continue;
                    }
                    
                    let virtual_path_str = format!("{}?session_id={}", path_str, id);
                    let virtual_path = PathBuf::from(virtual_path_str);
                    if let Err(_) = process_watch_file(&virtual_path, &pricing_map, db_path) {
                        result.sessions_failed += 1;
                    } else {
                        result.sessions_inserted += 1;
                    }
                }
            } else {
                result.sessions_failed += 1;
            }
        } else {
            // JSONL 파싱 및 중복 검사 후 적재
            let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let is_codex = file_name.starts_with("rollout-");
            
            let parsed_res = if is_codex {
                let adapter = CodexAdapter;
                adapter.parse_session(path_str)
            } else {
                let adapter = ClaudeCodeAdapter;
                adapter.parse_session(path_str)
            };
            
            match parsed_res {
                Ok(mut parsed_session) => {
                    let session_id = &parsed_session.session.session_id;
                    if db::get_session(&conn, session_id).is_ok() {
                        result.sessions_skipped += 1;
                        continue;
                    }
                    
                    // 비용 계산 및 적재
                    let pricing_info = parsed_session.session.model_id.as_ref()
                        .and_then(|m_id| pricing_map.get(m_id));

                    for msg in &mut parsed_session.messages {
                        if msg.role == "assistant" {
                            msg.cost_usd = pricing::calculate_cost_usd(
                                pricing_info,
                                msg.input_tokens,
                                msg.cache_read_input_tokens,
                                msg.output_tokens,
                            );
                        }
                    }

                    // DB Insert
                    if let Err(e) = db::insert_session(&conn, &parsed_session.session) {
                        eprintln!("세션 insert 에러: {}", e);
                        result.sessions_failed += 1;
                        continue;
                    }
                    for msg in &parsed_session.messages {
                        let _ = db::insert_message(&conn, msg);
                    }
                    for node in &parsed_session.nodes {
                        let _ = db::insert_node(&conn, node);
                    }
                    for tc in &parsed_session.tool_calls {
                        let _ = db::insert_tool_call(&conn, tc);
                    }
                    
                    result.sessions_inserted += 1;
                }
                Err(_) => {
                    result.sessions_failed += 1;
                }
            }
        }
    }
    
    if result.sessions_inserted > 0 {
        update_tray_status(&app_handle);
        let _ = app_handle.emit("db-updated", ());
    }
    
    Ok(result)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();

            // 1. 트레이 팝오버 윈도우 획득 및 blur 이벤트 핸들링
            if let Some(popover) = app.get_webview_window("tray-popover") {
                let popover_clone = popover.clone();
                popover.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = popover_clone.hide();
                    }
                });
            }

            // 2. 트레이 아이콘 초기 설정
            let icon_green_bytes = include_bytes!("../icons/icon_green.png");
            let initial_icon = tauri::image::Image::from_bytes(icon_green_bytes)
                .expect("Green icon load failed");

            let _tray = tauri::tray::TrayIconBuilder::with_id("main-tray")
                .icon(initial_icon)
                .title("$0.00")
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event: tauri::tray::TrayIconEvent| {
                    if let tauri::tray::TrayIconEvent::Click { button, position, .. } = event {
                        if button == tauri::tray::MouseButton::Left {
                            let app = tray.app_handle();
                            toggle_tray_popover(app, position);
                        }
                    }
                })
                .build(app)
                .expect("TrayIcon 생성 실패");

            // 초기 트레이 상태 갱신
            update_tray_status(&app_handle);

            std::thread::spawn(move || {
                if let Err(e) = start_watch_loop(app_handle) {
                    eprintln!("Watch Loop Error: {}", e);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_active_sessions,
            get_agent_summaries,
            get_loop_signals,
            get_daily_costs,
            get_session_details,
            interrupt_agent,
            focus_main_window,
            save_settings,
            load_settings,
            save_api_key,
            delete_api_key,
            get_api_keys_status,
            validate_stored_api_key,
            validate_local_path,
            sync_local_sessions
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 구동 중 에러 발생");
}
