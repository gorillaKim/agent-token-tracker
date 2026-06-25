#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Serialize, Deserialize};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};

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
pub struct DailyTokenUsage {
    pub date: String,
    pub total_tokens: u64,
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
                agy_sum.total_input_tokens += s.total_input_tokens;
                agy_sum.total_output_tokens += s.total_output_tokens;
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

/// 최근 14일간의 일별 토큰 사용량 집계
#[tauri::command]
fn get_daily_token_usage() -> Result<Vec<DailyTokenUsage>, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "WITH RECURSIVE dates(date) AS (
            SELECT date('now', '-13 day')
            UNION ALL
            SELECT date(date, '+1 day') FROM dates WHERE date < date('now')
         )
         SELECT 
            d.date, 
            COALESCE(SUM(s.total_input_tokens + s.total_output_tokens), 0) as total_tokens
         FROM dates d
         LEFT JOIN sessions s ON date(s.started_at) = d.date
         GROUP BY d.date
         ORDER BY d.date ASC;"
    ).map_err(|e| format!("SQL 쿼리 준비 에러: {}", e))?;

    let rows = stmt.query_map([], |row| {
        Ok(DailyTokenUsage {
            date: row.get(0)?,
            total_tokens: row.get(1)?,
        })
    }).map_err(|e| format!("쿼리 실행 에러: {}", e))?;

    let mut daily_tokens = Vec::new();
    for r in rows {
        daily_tokens.push(r.map_err(|e| format!("데이터 매핑 에러: {}", e))?);
    }

    Ok(daily_tokens)
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
    let _model_id_opt = parsed_session.session.model_id.as_deref().unwrap_or("unknown");
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
        
        // macOS 전체화면 및 Spaces 오버레이 처리를 위해 objc 크레이트를 사용하여 안전하게 네이티브 속성 주입
        // (objc 크레이트는 objc2와 달리 메모리 수명 어설션으로 인한 강제 Abort가 발생하지 않아 런타임에 매우 안전합니다.)
        #[cfg(target_os = "macos")]
        {
            use objc::{msg_send, sel, sel_impl};
            
            if let Ok(ns_window) = window.ns_window() {
                let ns_window_ptr = ns_window as *mut objc::runtime::Object;
                if !ns_window_ptr.is_null() {
                    unsafe {
                        // Window Level 설정: 25 (NSStatusWindowLevel - 상태 바 오버레이 가능 레벨)
                        let _: () = msg_send![ns_window_ptr, setLevel: 25isize]; 
                        
                        // Collection Behavior 설정: CanJoinAllSpaces(1) | MoveToActiveSpace(2) | Stationary(16) | FullScreenAuxiliary(256) (합산 값 275)
                        let _: () = msg_send![ns_window_ptr, setCollectionBehavior: 275usize];
                    }
                }
            }
        }
    }
}

/// 7. 팝오버 클릭 시 메인 윈도우 활성화 및 라우팅 연동
#[tauri::command]
fn focus_main_window(app_handle: AppHandle, session_id: Option<String>) -> Result<(), String> {
    if let Some(main_window) = app_handle.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);

        let _ = main_window.show();
        let _ = main_window.unminimize();
        let _ = main_window.set_focus();
        if let Some(id) = session_id {
            let _ = app_handle.emit("navigate-to-session", id);
        }
    }
    Ok(())
}

fn default_token_limit() -> u64 {
    50_000_000
}

fn default_claude_plan() -> String {
    "pro".to_string()
}

fn default_openai_plan() -> String {
    "tier1".to_string()
}

fn default_token_display_mode() -> String {
    "tokens".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub log_dir: String,
    #[serde(default = "default_token_limit")]
    pub token_limit: u64,
    #[serde(default = "default_token_limit")]
    pub token_limit_claude: u64,
    #[serde(default = "default_token_limit")]
    pub token_limit_codex: u64,
    #[serde(default = "default_token_limit")]
    pub token_limit_antigravity: u64,
    /// Anthropic 구독 플랜: "free" | "pro" | "max5x" | "max20x" | "api"
    #[serde(default = "default_claude_plan")]
    pub claude_plan: String,
    /// OpenAI 구독 티어: "free" | "tier1" | "tier2" | "tier5"
    #[serde(default = "default_openai_plan")]
    pub openai_plan: String,
    #[serde(default = "default_token_display_mode")]
    pub token_display_mode: String,
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
fn save_settings(
    app_handle: AppHandle,
    log_dir: String,
    token_limit: u64,
    token_limit_claude: u64,
    token_limit_codex: u64,
    token_limit_antigravity: u64,
    claude_plan: Option<String>,
    openai_plan: Option<String>,
    token_display_mode: Option<String>,
) -> Result<(), String> {
    let path = get_config_path(&app_handle)?;
    // 기존 설정을 읽어 플랜 필드를 보존
    let existing = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<AppSettings>(&s).ok())
    } else {
        None
    };
    let settings = AppSettings {
        log_dir,
        token_limit,
        token_limit_claude,
        token_limit_codex,
        token_limit_antigravity,
        claude_plan: claude_plan.unwrap_or_else(|| {
            existing.as_ref().map(|s| s.claude_plan.clone()).unwrap_or_else(default_claude_plan)
        }),
        openai_plan: openai_plan.unwrap_or_else(|| {
            existing.as_ref().map(|s| s.openai_plan.clone()).unwrap_or_else(default_openai_plan)
        }),
        token_display_mode: token_display_mode.unwrap_or_else(|| {
            existing.as_ref().map(|s| s.token_display_mode.clone()).unwrap_or_else(default_token_display_mode)
        }),
    };
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
        return Ok(AppSettings {
            log_dir: "".to_string(),
            token_limit: 50_000_000,
            token_limit_claude: 50_000_000,
            token_limit_codex: 50_000_000,
            token_limit_antigravity: 50_000_000,
            claude_plan: default_claude_plan(),
            openai_plan: default_openai_plan(),
            token_display_mode: default_token_display_mode(),
        });
    }
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("설정 파일 읽기 실패: {}", e))?;
    let settings: AppSettings = serde_json::from_str(&json)
        .map_err(|e| format!("JSON 역직렬화 실패: {}", e))?;
    Ok(settings)
}

// ────────────────────────────────────────────────────────────
// 구독 플랜 한도 테이블 (ccusage 방식 — 플랜별 내장값)
// ────────────────────────────────────────────────────────────

/// 플랜 이름 → (5시간 윈도우 토큰 한도, 설명)
fn claude_plan_quota(plan: &str) -> (u64, &'static str) {
    match plan {
        "free"   => (10_000_000,   "Claude Free (10M / 5hr window)"),
        "pro"    => (44_000_000,   "Claude Pro ($20/mo, ~44M / 5hr window)"),
        "max5x"  => (220_000_000,  "Claude Max 5x ($100/mo, ~220M / 5hr window)"),
        "max20x" => (880_000_000,  "Claude Max 20x ($200/mo, ~880M / 5hr window)"),
        "api"    => (u64::MAX / 2, "Claude API (rate limit 기반)"),
        _        => (44_000_000,   "Claude Pro (기본값)"),
    }
}

/// OpenAI 티어 → (월간 토큰 한도, 설명)
fn openai_tier_quota(tier: &str) -> (u64, &'static str) {
    match tier {
        "free"  => (1_000_000,    "OpenAI Free Tier"),
        "tier1" => (100_000_000,  "OpenAI Tier 1 ($5+ 충전)"),
        "tier2" => (500_000_000,  "OpenAI Tier 2 ($50+ 지출)"),
        "tier5" => (5_000_000_000, "OpenAI Tier 5 (최고 한도)"),
        _       => (100_000_000,  "OpenAI Tier 1 (기본값)"),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlanQuotaInfo {
    pub provider: String,
    pub plan_key: String,
    pub plan_label: String,
    pub quota_tokens: u64,
    /// 5시간 윈도우 내 사용량 (Claude) or 이번 달 사용량 (OpenAI)
    pub used_tokens: u64,
    pub remaining_tokens: u64,
    pub usage_pct: f64,
    /// 5시간 윈도우 리셋 예상 시각 (ISO 8601, Claude 전용)
    pub window_reset_at: Option<String>,
    pub window_hours: u32,

    // Claude 주간 모든 모델 한도 필드 (Option)
    pub weekly_quota_tokens: Option<u64>,
    pub weekly_used_tokens: Option<u64>,
    pub weekly_remaining_tokens: Option<u64>,
    pub weekly_usage_pct: Option<f64>,
    pub weekly_reset_at: Option<String>,
}

/// 현재 5시간 롤링 윈도우 내의 Claude 토큰 사용량 조회
#[tauri::command]
fn get_rolling_window_usage() -> Result<u64, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(total_input_tokens + total_output_tokens), 0)
         FROM sessions
         WHERE started_at >= datetime('now', '-5 hours')"
    ).map_err(|e| e.to_string())?;
    let used: u64 = stmt.query_row([], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(used)
}

/// 이번 달 OpenAI 누적 토큰 사용량 조회
fn get_monthly_usage_openai() -> Result<u64, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(total_input_tokens + total_output_tokens), 0)
         FROM sessions
         WHERE agent_type = 'codex'
           AND strftime('%Y-%m', started_at) = strftime('%Y-%m', 'now')"
    ).map_err(|e| e.to_string())?;
    let used: u64 = stmt.query_row([], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(used)
}

/// 오늘(24시간) OpenAI Codex 토큰 사용량 조회
fn get_today_usage_openai() -> Result<u64, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(total_input_tokens + total_output_tokens), 0)
         FROM sessions
         WHERE agent_type = 'codex'
           AND started_at >= datetime('now', '-24 hours')"
    ).map_err(|e| e.to_string())?;
    let used: u64 = stmt.query_row([], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(used)
}

/// 최근 7일(주간) Antigravity 토큰 사용량 조회
fn get_weekly_usage_antigravity() -> Result<u64, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(total_input_tokens + total_output_tokens), 0)
         FROM sessions
         WHERE agent_type = 'antigravity'
           AND started_at >= datetime('now', '-7 days')"
    ).map_err(|e| e.to_string())?;
    let used: u64 = stmt.query_row([], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(used)
}

/// 가장 최근 세션의 시작 시각 기준 5시간 윈도우 리셋 예상 시각 계산
fn calc_window_reset_at() -> Result<Option<String>, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT MAX(started_at) FROM sessions WHERE started_at >= datetime('now', '-5 hours')"
    ).map_err(|e| e.to_string())?;
    let oldest: Option<String> = stmt.query_row([], |r| r.get(0)).ok().flatten();
    if let Some(ts) = oldest {
        // 가장 오래된 세션 시각 + 5시간 = 리셋 예상 시각
        let conn2 = get_db_conn()?;
        let mut stmt2 = conn2.prepare(
            "SELECT MIN(started_at) FROM sessions WHERE started_at >= datetime('now', '-5 hours')"
        ).map_err(|e| e.to_string())?;
        let _ = ts;
        let earliest: Option<String> = stmt2.query_row([], |r| r.get(0)).ok().flatten();
        if let Some(earliest_ts) = earliest {
            // earliest_ts + 5h → reset_at (ISO 문자열로 반환)
            let reset_sql_result: Result<String, _> = conn2.query_row(
                &format!("SELECT datetime('{}', '+5 hours')", earliest_ts),
                [],
                |r| r.get(0),
            );
            return Ok(reset_sql_result.ok());
        }
    }
    Ok(None)
}

// 헬퍼 함수: 로컬 fetch-claude-usage.swift 파일에서 세션 키 파싱
fn get_local_session_key_from_swift() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() { return None; }
    let swift_path = Path::new(&home).join(".claude").join("fetch-claude-usage.swift");
    if !swift_path.exists() || !swift_path.is_file() { return None; }
    if let Ok(content) = std::fs::read_to_string(swift_path) {
        for line in content.lines() {
            if line.contains("injectedKey") {
                if let Some(start_idx) = line.find("\"") {
                    if let Some(end_idx) = line[start_idx + 1..].find("\"") {
                        let key = &line[start_idx + 1..start_idx + 1 + end_idx];
                        if !key.trim().is_empty() {
                            return Some(key.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

// 헬퍼 함수: 로컬 fetch-claude-usage.swift 파일에서 orgId 파싱
fn get_local_org_id_from_swift() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() { return None; }
    let swift_path = Path::new(&home).join(".claude").join("fetch-claude-usage.swift");
    if !swift_path.exists() || !swift_path.is_file() { return None; }
    if let Ok(content) = std::fs::read_to_string(swift_path) {
        for line in content.lines() {
            if line.contains("injectedOrgId") {
                if let Some(start_idx) = line.find("\"") {
                    if let Some(end_idx) = line[start_idx + 1..].find("\"") {
                        let org_id = &line[start_idx + 1..start_idx + 1 + end_idx];
                        if !org_id.trim().is_empty() {
                            return Some(org_id.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

// 헬퍼 함수: 세션 키를 활용해 organizations 리스트를 받아와 첫 번째 orgId 반환
async fn fetch_first_org_id(session_key: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let response = client.get("https://claude.ai/api/organizations")
        .header("Cookie", format!("sessionKey={}", session_key))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Organization 조회 요청 실패: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Organization 조회 실패, status: {}", response.status()));
    }

    let json: serde_json::Value = response.json()
        .await
        .map_err(|e| format!("Organization JSON 파싱 실패: {}", e))?;

    if let Some(arr) = json.as_array() {
        if let Some(first_org) = arr.first() {
            if let Some(uuid) = first_org.get("uuid").and_then(|u| u.as_str()) {
                return Ok(uuid.to_string());
            }
        }
    }
    
    Err("유효한 Organization ID를 찾을 수 없습니다.".to_string())
}

// 헬퍼 함수: 세션 키와 orgId로 실제 Claude Usage 데이터 실시간 조회
async fn fetch_claude_usage_from_api(session_key: &str, org_id: &str) -> Result<(f64, Option<String>, Option<f64>, Option<String>), String> {
    let client = reqwest::Client::new();
    let url = format!("https://claude.ai/api/organizations/{}/usage", org_id);
    let response = client.get(&url)
        .header("Cookie", format!("sessionKey={}", session_key))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Claude usage API 호출 실패: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Claude usage API 응답 에러, status: {}", response.status()));
    }

    let json: serde_json::Value = response.json()
        .await
        .map_err(|e| format!("Claude usage JSON 파싱 실패: {}", e))?;

    let mut five_hour_util = 0.0;
    let mut five_hour_reset = None;

    if let Some(five_hour) = json.get("five_hour") {
        if let Some(utilization_val) = five_hour.get("utilization") {
            five_hour_util = if let Some(u_f64) = utilization_val.as_f64() {
                u_f64
            } else if let Some(u_i64) = utilization_val.as_i64() {
                u_i64 as f64
            } else {
                return Err("utilization 값 포맷 에러".to_string());
            };
        }
        five_hour_reset = five_hour.get("resets_at")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
    }

    let mut weekly_util = None;
    let mut weekly_reset = None;

    if let Some(seven_day) = json.get("seven_day") {
        if let Some(utilization_val) = seven_day.get("utilization") {
            let utilization = if let Some(u_f64) = utilization_val.as_f64() {
                u_f64
            } else if let Some(u_i64) = utilization_val.as_i64() {
                u_i64 as f64
            } else {
                0.0
            };
            weekly_util = Some(utilization);
        }
        weekly_reset = seven_day.get("resets_at")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
    }

    Ok((five_hour_util, five_hour_reset, weekly_util, weekly_reset))
}

#[derive(Debug, Deserialize)]
struct OpenAIUsageResponse {
    data: Option<Vec<OpenAIUsageItem>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsageItem {
    n_context_tokens_total: Option<u64>,
    n_generated_tokens_total: Option<u64>,
}

async fn fetch_openai_usage_from_api(api_key: &str) -> Result<u64, String> {
    let now = chrono::Local::now();
    let start_date = now.format("%Y-%m-01").to_string();
    let end_date = now.format("%Y-%m-%d").to_string();

    let client = reqwest::Client::new();
    let url = format!(
        "https://api.openai.com/v1/usage?start_date={}&end_date={}",
        start_date, end_date
    );

    let response = client.get(&url)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("OpenAI API 호출 실패: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("OpenAI API 응답 에러, status: {}", response.status()));
    }

    let usage_res: OpenAIUsageResponse = response.json()
        .await
        .map_err(|e| format!("OpenAI JSON 파싱 실패: {}", e))?;

    let mut total_tokens = 0;
    if let Some(items) = usage_res.data {
        for item in items {
            total_tokens += item.n_context_tokens_total.unwrap_or(0);
            total_tokens += item.n_generated_tokens_total.unwrap_or(0);
        }
    }

    Ok(total_tokens)
}

fn get_today_usage_antigravity() -> Result<u64, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(total_input_tokens + total_output_tokens), 0)
         FROM sessions
         WHERE agent_type = 'antigravity'
           AND started_at >= datetime('now', '-24 hours')"
    ).map_err(|e| e.to_string())?;
    let used: u64 = stmt.query_row([], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(used)
}

/// 구독 플랜별 토큰 사용 현황 조회
#[tauri::command]
async fn get_subscription_quota(app_handle: AppHandle) -> Result<Vec<PlanQuotaInfo>, String> {
    let settings = load_settings(app_handle)?;

    let mut result = Vec::new();

    // ── Claude (5시간 롤링 윈도우) ──
    let mut claude_used = get_rolling_window_usage().unwrap_or(0);
    let (claude_quota, claude_label) = claude_plan_quota(&settings.claude_plan);
    let mut claude_remaining = claude_quota.saturating_sub(claude_used);
    let mut claude_pct = if claude_quota == 0 || claude_quota == u64::MAX / 2 {
        0.0
    } else {
        (claude_used as f64 / claude_quota as f64 * 100.0).min(100.0)
    };
    let mut reset_at = calc_window_reset_at().unwrap_or(None);

    // Claude 주간 롤링 한도 변수 초기화 (기본값)
    let weekly_quota = if claude_quota == u64::MAX / 2 { u64::MAX / 2 } else { claude_quota * 10 }; // 5시간의 10배 (Pro 기준 440M)
    let mut claude_weekly_used = 0;
    let mut claude_weekly_pct = 0.0;
    let mut claude_weekly_reset = None;

    // 키체인 또는 로컬 swift 스크립트에서 세션 키 획득 시도
    let mut resolved_session_key = None;
    if let Ok(entry) = Entry::new("agent-token-tracker", "anthropic") {
        if let Ok(session_key) = entry.get_password() {
            let trimmed = session_key.trim().to_string();
            if trimmed.starts_with("sk-ant-sid02-") && !trimmed.is_empty() {
                resolved_session_key = Some(trimmed);
            }
        }
    }
    
    // 키체인에서 획득하지 못한 경우 로컬 swift 스크립트에서 직접 파싱 시도
    if resolved_session_key.is_none() {
        if let Some(key) = get_local_session_key_from_swift() {
            println!("[Quota] swift 스크립트로부터 세션 키 파싱 성공!");
            resolved_session_key = Some(key);
        }
    }

    if let Some(session_key) = resolved_session_key {
        println!("[Quota] Anthropic 웹 세션 키를 활용하여 Claude 실시간 사용량 조회를 시작합니다.");
        
        // orgId 결정 (로컬 swift 스크립트 파싱 -> 실패 시 API 조회)
        let mut target_org_id = get_local_org_id_from_swift();
        if target_org_id.is_none() {
            if let Ok(api_org_id) = fetch_first_org_id(&session_key).await {
                target_org_id = Some(api_org_id);
            }
        }

        if let Some(org_id) = target_org_id {
            println!("[Quota] Claude usage 조회를 위해 org_id: {} 를 사용합니다.", org_id);
            match fetch_claude_usage_from_api(&session_key, &org_id).await {
                Ok((five_hour_utilization, api_resets_at, weekly_utilization, weekly_api_resets_at)) => {
                    // 1. 5시간 롤링 한도 가공
                    claude_pct = if five_hour_utilization < 1.0 {
                        five_hour_utilization * 100.0
                    } else {
                        five_hour_utilization
                    };
                    if claude_quota != u64::MAX / 2 && claude_quota > 0 {
                        claude_used = (claude_quota as f64 * (claude_pct / 100.0)) as u64;
                        claude_remaining = claude_quota.saturating_sub(claude_used);
                    }
                    if let Some(r_at) = api_resets_at {
                        reset_at = Some(r_at);
                    }

                    // 2. 주간 롤링 한도 가공
                    if let Some(w_util) = weekly_utilization {
                        claude_weekly_pct = if w_util < 1.0 {
                            w_util * 100.0
                        } else {
                            w_util
                        };
                        if weekly_quota != u64::MAX / 2 && weekly_quota > 0 {
                            claude_weekly_used = (weekly_quota as f64 * (claude_weekly_pct / 100.0)) as u64;
                        }
                    }
                    if let Some(w_r_at) = weekly_api_resets_at {
                        claude_weekly_reset = Some(w_r_at);
                    }

                    println!(
                        "[Quota] Claude 실시간 조회 성공: 소진율 = {:.2}%, 주간 소진율 = {:.2}%, 리셋시각 = {:?}",
                        claude_pct, claude_weekly_pct, reset_at
                    );
                }
                Err(e) => {
                    eprintln!("[Quota] Claude 실시간 usage API 호출 실패 (로컬 DB 폴백 사용): {}", e);
                }
            }
        }
    } else {
        println!("[Quota] 유효한 Claude 웹 세션 키를 발견하지 못했습니다. 로컬 DB 집계를 사용합니다.");
    }

    let weekly_remaining = weekly_quota.saturating_sub(claude_weekly_used);

    result.push(PlanQuotaInfo {
        provider: "anthropic".to_string(),
        plan_key: settings.claude_plan.clone(),
        plan_label: claude_label.to_string(),
        quota_tokens: claude_quota,
        used_tokens: claude_used,
        remaining_tokens: claude_remaining,
        usage_pct: claude_pct,
        window_reset_at: reset_at,
        window_hours: 5,
        weekly_quota_tokens: Some(weekly_quota),
        weekly_used_tokens: Some(claude_weekly_used),
        weekly_remaining_tokens: Some(weekly_remaining),
        weekly_usage_pct: Some(claude_weekly_pct),
        weekly_reset_at: claude_weekly_reset,
    });

    // ── OpenAI Codex (이번 달 누적) ──
    let mut openai_used = get_monthly_usage_openai().unwrap_or(0);
    let (openai_quota, openai_label) = openai_tier_quota(&settings.openai_plan);

    // 키체인 혹은 환경변수에서 OpenAI API Key 획득 시도
    let mut resolved_openai_key = None;
    if let Ok(entry) = Entry::new("agent-token-tracker", "openai") {
        if let Ok(api_key) = entry.get_password() {
            let trimmed = api_key.trim().to_string();
            if !trimmed.is_empty() {
                resolved_openai_key = Some(trimmed);
            }
        }
    }

    if resolved_openai_key.is_none() {
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            let trimmed = api_key.trim().to_string();
            if !trimmed.is_empty() {
                resolved_openai_key = Some(trimmed);
            }
        }
    }

    if let Some(api_key) = resolved_openai_key {
        println!("[Quota] OpenAI API 키를 활용하여 OpenAI 실시간 사용량 조회를 시작합니다.");
        match fetch_openai_usage_from_api(&api_key).await {
            Ok(total_tokens) => {
                println!("[Quota] OpenAI 실시간 조회 성공: total_tokens = {}", total_tokens);
                openai_used = total_tokens;
            }
            Err(e) => {
                eprintln!("[Quota] OpenAI 실시간 usage API 호출 실패 (로컬 DB 폴백 사용): {}", e);
            }
        }
    } else {
        println!("[Quota] 유효한 OpenAI API 키를 발견하지 못했습니다. 로컬 DB 집계를 사용합니다.");
    }

    let openai_today_used = get_today_usage_openai().unwrap_or(0);
    let openai_today_pct = if settings.token_limit_codex == 0 {
        0.0
    } else {
        (openai_today_used as f64 / settings.token_limit_codex as f64 * 100.0).min(100.0)
    };
    let openai_today_remaining = settings.token_limit_codex.saturating_sub(openai_today_used);

    let openai_remaining = openai_quota.saturating_sub(openai_used);
    let openai_pct = if openai_quota == 0 {
        0.0
    } else {
        (openai_used as f64 / openai_quota as f64 * 100.0).min(100.0)
    };

    result.push(PlanQuotaInfo {
        provider: "openai".to_string(),
        plan_key: settings.openai_plan.clone(),
        plan_label: openai_label.to_string(),
        quota_tokens: settings.token_limit_codex,
        used_tokens: openai_today_used,
        remaining_tokens: openai_today_remaining,
        usage_pct: openai_today_pct,
        window_reset_at: None,
        window_hours: 24, 
        weekly_quota_tokens: Some(openai_quota),
        weekly_used_tokens: Some(openai_used),
        weekly_remaining_tokens: Some(openai_remaining),
        weekly_usage_pct: Some(openai_pct),
        weekly_reset_at: None,
    });

    // ── Antigravity (24시간 누적) ──
    let agy_used = get_today_usage_antigravity().unwrap_or(0);
    let agy_quota = settings.token_limit_antigravity;
    let agy_remaining = agy_quota.saturating_sub(agy_used);
    let agy_pct = if agy_quota == 0 {
        0.0
    } else {
        (agy_used as f64 / agy_quota as f64 * 100.0).min(100.0)
    };
    
    let agy_reset_at = {
        let conn = get_db_conn()?;
        let mut stmt = conn.prepare(
            "SELECT MIN(started_at) FROM sessions WHERE agent_type = 'antigravity' AND started_at >= datetime('now', '-24 hours')"
        ).map_err(|e| e.to_string())?;
        let oldest: Option<String> = stmt.query_row([], |r| r.get(0)).ok().flatten();
        if let Some(earliest_ts) = oldest {
            let reset_sql_result: Result<String, _> = conn.query_row(
                &format!("SELECT datetime('{}', '+24 hours')", earliest_ts),
                [],
                |r| r.get(0),
            );
            reset_sql_result.ok()
        } else {
            None
        }
    };

    let agy_weekly_used = get_weekly_usage_antigravity().unwrap_or(0);
    let agy_weekly_quota = agy_quota * 7;
    let agy_weekly_pct = if agy_weekly_quota == 0 {
        0.0
    } else {
        (agy_weekly_used as f64 / agy_weekly_quota as f64 * 100.0).min(100.0)
    };
    let agy_weekly_remaining = agy_weekly_quota.saturating_sub(agy_weekly_used);

    result.push(PlanQuotaInfo {
        provider: "antigravity".to_string(),
        plan_key: "local".to_string(),
        plan_label: "Antigravity Local Quota".to_string(),
        quota_tokens: agy_quota,
        used_tokens: agy_used,
        remaining_tokens: agy_remaining,
        usage_pct: agy_pct,
        window_reset_at: agy_reset_at,
        window_hours: 24,
        weekly_quota_tokens: Some(agy_weekly_quota),
        weekly_used_tokens: Some(agy_weekly_used),
        weekly_remaining_tokens: Some(agy_weekly_remaining),
        weekly_usage_pct: Some(agy_weekly_pct),
        weekly_reset_at: None,
    });

    Ok(result)
}

// ────────────────────────────────────────────────────────────
// 세션 심층 분석 커맨드
// ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct TurnTokenUsage {
    pub turn_index: i64,
    pub role: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCostRank {
    pub tool_name: String,
    pub call_count: u64,
    pub success_count: u64,
    pub estimated_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionAnalysis {
    pub session_id: String,
    pub agent_type: String,
    pub model_id: Option<String>,
    pub started_at: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cost_usd: f64,
    /// 캐시 히트율 (0.0 ~ 1.0)
    pub cache_hit_rate: f64,
    /// 캐시로 절감된 예상 비용 (USD)
    pub cache_saved_cost: f64,
    /// 턴별 토큰 소비 내역
    pub turns: Vec<TurnTokenUsage>,
    /// 도구별 비용 랭킹 (상위 10개)
    pub tool_cost_rank: Vec<ToolCostRank>,
    /// 이상 탐지 시그널
    pub anomaly_signals: Vec<agent_token_tracker::detect::loops::LoopSignal>,
    /// 이상 탐지 여부
    pub is_anomaly: bool,
}

#[tauri::command]
fn get_session_analysis(session_id: String) -> Result<SessionAnalysis, String> {
    let conn = get_db_conn()?;

    // 세션 기본 정보
    let sessions = db::get_all_sessions(&conn)
        .map_err(|e| format!("세션 조회 실패: {}", e))?;
    let sess = sessions.into_iter().find(|s| s.session_id == session_id)
        .ok_or_else(|| format!("세션 ID를 찾을 수 없습니다: {}", session_id))?;

    // 메시지 조회
    let messages = db::get_messages_by_session(&conn, &session_id)
        .map_err(|e| format!("메시지 조회 실패: {}", e))?;

    // 도구 호출 조회
    let tool_calls = db::get_tool_calls_by_session(&conn, &session_id)
        .map_err(|e| format!("도구 호출 조회 실패: {}", e))?;

    // 캐시 관련 집계
    let total_input: u64 = messages.iter().map(|m| m.input_tokens).sum();
    let total_output: u64 = messages.iter().map(|m| m.output_tokens).sum();
    let total_cache_read: u64 = messages.iter().map(|m| m.cache_read_input_tokens).sum();
    let total_cost: f64 = messages.iter().map(|m| m.cost_usd).sum();

    let cache_hit_rate = if total_input > 0 {
        total_cache_read as f64 / total_input as f64
    } else {
        0.0
    };

    // 캐시 절감 비용 추정 (캐시 읽기 비용 vs 일반 입력 비용 차이)
    // claude-3-5-sonnet: input $3/MTok, cache_read $0.30/MTok → 약 90% 절감
    let cache_saved_cost = total_cache_read as f64 * 2.7 / 1_000_000.0;

    // 턴별 내역
    let turns: Vec<TurnTokenUsage> = messages.iter().map(|m| TurnTokenUsage {
        turn_index: m.turn_index as i64,
        role: m.role.clone(),
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        cache_read_tokens: m.cache_read_input_tokens,
        cost_usd: m.cost_usd,
        created_at: m.created_at.clone(),
    }).collect();

    // 도구별 비용 랭킹 (세션의 총 토큰을 도구 호출 수에 비례 배분)
    let total_tool_calls = tool_calls.len() as u64;
    let token_per_call = if total_tool_calls > 0 {
        (total_input + total_output) / total_tool_calls
    } else {
        0
    };
    let cost_per_call = if total_tool_calls > 0 {
        total_cost / total_tool_calls as f64
    } else {
        0.0
    };

    let mut tool_map: std::collections::HashMap<String, (u64, u64, f64)> = std::collections::HashMap::new();
    for tc in &tool_calls {
        let entry = tool_map.entry(tc.tool_name.clone()).or_insert((0, 0, 0.0));
        entry.0 += 1; // call_count
        if tc.success { entry.1 += 1; } // success_count
        entry.2 += cost_per_call; // cost_usd 비례배분
    }

    let mut tool_cost_rank: Vec<ToolCostRank> = tool_map.into_iter().map(|(name, (calls, successes, cost))| {
        ToolCostRank {
            tool_name: name,
            call_count: calls,
            success_count: successes,
            estimated_tokens: calls * token_per_call,
            total_cost_usd: cost,
        }
    }).collect();
    tool_cost_rank.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    tool_cost_rank.truncate(10);

    // 이상 탐지
    let config = DetectorConfig::default();
    let detect_result = detect_session_anomalies(&sess, &messages, &tool_calls, &config);

    Ok(SessionAnalysis {
        session_id: sess.session_id.clone(),
        agent_type: sess.agent_type.clone(),
        model_id: sess.model_id.clone(),
        started_at: sess.started_at.clone(),
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_read_tokens: total_cache_read,
        total_cost_usd: total_cost,
        cache_hit_rate,
        cache_saved_cost,
        turns,
        tool_cost_rank,
        anomaly_signals: detect_result.signals,
        is_anomaly: detect_result.is_anomaly,
    })
}

// ────────────────────────────────────────────────────────────
// 이슈 #791: 로컬 인증 토큰 자동 감지 및 간편 연동 커맨드
// ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetectedCredential {
    pub provider: String,          // "anthropic" | "openai"
    pub token_type: String,        // "oauth_token" | "api_key"
    pub value: String,             // 마스킹 처리된 토큰 값 (예: "sk-ant-oat01...xxxx")
    pub raw_value: String,         // 실제 토큰 값
    pub source: String,            // "Keychain" | "EnvVar" | "ConfigFile"
    pub description: String,       // 설명 텍스트
}

fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        return "****".to_string();
    }
    let prefix = &token[0..8];
    let suffix = &token[token.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

#[tauri::command]
fn get_local_credentials() -> Result<Vec<DetectedCredential>, String> {
    let mut detected = Vec::new();

    // 1. macOS Keychain에서 claude-code OAuth 토큰 조회 시도
    // A. security CLI 커맨드 직접 쿼리 (가장 확실함)
    let output = std::process::Command::new("security")
        .args(&["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let password = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !password.is_empty() {
                detected.push(DetectedCredential {
                    provider: "anthropic".to_string(),
                    token_type: "oauth_token".to_string(),
                    value: mask_token(&password),
                    raw_value: password.clone(),
                    source: "Keychain".to_string(),
                    description: "macOS 키체인 (Claude Code-credentials)".to_string(),
                });
            }
        }
    }

    // B. keyring 크레이트를 통한 백업 스캔
    if detected.is_empty() {
        let user = std::env::var("USER").unwrap_or_default();
        let username = std::env::var("USERNAME").unwrap_or_default();
        let mut accounts = vec![
            "oauth".to_string(),
            "claude-code".to_string(),
            "current".to_string(),
            "default".to_string(),
            "session".to_string(),
            "token".to_string(),
        ];
        if !user.is_empty() { accounts.push(user); }
        if !username.is_empty() { accounts.push(username); }

        let services = vec!["Claude Code-credentials", "Claude Code", "claude-code"];

        'outer: for svc in services {
            for acct in &accounts {
                if let Ok(entry) = Entry::new(svc, acct) {
                    if let Ok(password) = entry.get_password() {
                        if !password.trim().is_empty() {
                            detected.push(DetectedCredential {
                                provider: "anthropic".to_string(),
                                token_type: "oauth_token".to_string(),
                                value: mask_token(&password),
                                raw_value: password.clone(),
                                source: "Keychain".to_string(),
                                description: format!("macOS 키체인 (서비스: {}, 계정: {})", svc, acct),
                            });
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    // 2. 로컬 파일시스템 스캔
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let home_path = Path::new(&home);
        
        // A. 기존 설정 파일 감지
        let possible_files = vec![
            (home_path.join(".claude").join(".credentials.json"), "oauthToken"),
            (home_path.join(".claude").join("login.json"), "accessToken"),
            (home_path.join(".claude").join("config.json"), "oauthToken"),
        ];

        for (path, key) in possible_files {
            if path.exists() && path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(token) = val.get(key).and_then(|v| v.as_str()) {
                            if !token.trim().is_empty() {
                                detected.push(DetectedCredential {
                                    provider: "anthropic".to_string(),
                                    token_type: "oauth_token".to_string(),
                                    value: mask_token(token),
                                    raw_value: token.to_string(),
                                    source: "ConfigFile".to_string(),
                                    description: format!("로컬 설정 파일 ({})", path.file_name().unwrap().to_str().unwrap()),
                                });
                            }
                        }
                    }
                }
            }
        }

        // B. fetch-claude-usage.swift 파일 스캔 및 세션 키 파싱
        let swift_script_path = home_path.join(".claude").join("fetch-claude-usage.swift");
        if swift_script_path.exists() && swift_script_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&swift_script_path) {
                for line in content.lines() {
                    if line.contains("injectedKey") && line.contains("sk-ant-sid02") {
                        if let Some(start_idx) = line.find("\"") {
                            if let Some(end_idx) = line[start_idx + 1..].find("\"") {
                                let token = &line[start_idx + 1..start_idx + 1 + end_idx];
                                if !token.trim().is_empty() {
                                    detected.push(DetectedCredential {
                                        provider: "anthropic".to_string(),
                                        token_type: "oauth_token".to_string(),
                                        value: mask_token(token),
                                        raw_value: token.to_string(),
                                        source: "ConfigFile".to_string(),
                                        description: "Claude.ai 세션 키 (fetch-claude-usage.swift)".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. 환경 변수 스캔
    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !token.trim().is_empty() {
            detected.push(DetectedCredential {
                provider: "anthropic".to_string(),
                token_type: "oauth_token".to_string(),
                value: mask_token(&token),
                raw_value: token,
                source: "EnvVar".to_string(),
                description: "환경 변수 CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            });
        }
    }

    if let Ok(token) = std::env::var("ANTHROPIC_API_KEY") {
        if !token.trim().is_empty() {
            detected.push(DetectedCredential {
                provider: "anthropic".to_string(),
                token_type: "api_key".to_string(),
                value: mask_token(&token),
                raw_value: token,
                source: "EnvVar".to_string(),
                description: "환경 변수 ANTHROPIC_API_KEY".to_string(),
            });
        }
    }

    if let Ok(token) = std::env::var("OPENAI_API_KEY") {
        if !token.trim().is_empty() {
            detected.push(DetectedCredential {
                provider: "openai".to_string(),
                token_type: "api_key".to_string(),
                value: mask_token(&token),
                raw_value: token,
                source: "EnvVar".to_string(),
                description: "환경 변수 OPENAI_API_KEY".to_string(),
            });
        }
    }

    // 4. macOS Keychain에서 openai API 키 조회 시도
    if let Ok(entry) = Entry::new("agent-token-tracker", "openai") {
        if let Ok(api_key) = entry.get_password() {
            let trimmed = api_key.trim().to_string();
            if !trimmed.is_empty() {
                detected.push(DetectedCredential {
                    provider: "openai".to_string(),
                    token_type: "api_key".to_string(),
                    value: mask_token(&trimmed),
                    raw_value: trimmed,
                    source: "Keychain".to_string(),
                    description: "macOS 키체인 (agent-token-tracker / openai)".to_string(),
                });
            }
        }
    }

    // 감지된 openai 항목이 없으면 시각적 연동 테스트를 위해 Mock 감지 자격증명을 추가
    let has_openai = detected.iter().any(|c| c.provider == "openai");
    if !has_openai {
        detected.push(DetectedCredential {
            provider: "openai".to_string(),
            token_type: "api_key".to_string(),
            value: mask_token("sk-proj-mockOpenaiKey1234567890123"),
            raw_value: "sk-proj-mockOpenaiKey1234567890123".to_string(),
            source: "EnvVar (시뮬레이션)".to_string(),
            description: "환경 변수 OPENAI_API_KEY (자동 감지 데모)".to_string(),
        });
    }

    Ok(detected)
}

#[tauri::command]
fn auto_apply_credential(provider: String, raw_value: String) -> Result<(), String> {
    println!("[Credential] auto_apply_credential 호출 - provider: {}, raw_value 길이: {}", provider, raw_value.len());
    let res = save_api_key(provider, raw_value);
    println!("[Credential] auto_apply_credential 결과: {:?}", res);
    res
}

#[tauri::command]
fn save_api_key(provider: String, api_key: String) -> Result<(), String> {
    println!("[Credential] save_api_key 호출 - provider: {}, api_key 길이: {}", provider, api_key.len());
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

    // OAuth 토큰(sk-ant-oat) 또는 웹 세션 키(sk-ant-sid)는 공식 API(/v1/messages) 인증에 쓸 수 없으므로 
    // 포맷 매칭 시 즉시 유효(true) 판정을 내립니다.
    if api_key.starts_with("sk-ant-oat") || api_key.starts_with("sk-ant-sid") {
        println!("[Credential] Anthropic OAuth 또는 웹 세션 토큰 감지 - API 테스트를 우회하여 유효 판정");
        return Ok(true);
    }

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
    println!("[Sync] sync_local_sessions command triggered!");
    if let Ok(cwd) = std::env::current_dir() {
        println!("[Sync] Current working directory: {:?}", cwd);
    }
    let db_path = "../atk.db";
    
    // 1. 로드 설정 경로 및 기본 에이전트 경로들을 취합
    let config_path = get_config_path(&app_handle)?;
    println!("[Sync] config_path: {:?}", config_path);
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
    println!("[Sync] Final log_dir: {}", log_dir);
    
    let mut target_paths = Vec::new();
    if !log_dir.is_empty() {
        target_paths.push(PathBuf::from(&log_dir));
    }
    
    // 기본 OS별 에이전트 경로 추가
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        // Claude Code 기본 경로
        let claude_path = Path::new(&home).join(".claude").join("projects");
        if claude_path.exists() && claude_path.is_dir() {
            println!("[Sync] Added default Claude Code path: {:?}", claude_path);
            target_paths.push(claude_path);
        }
        
        // Codex 기본 경로
        let codex_path = Path::new(&home).join(".codex").join("sessions");
        if codex_path.exists() && codex_path.is_dir() {
            println!("[Sync] Added default Codex path: {:?}", codex_path);
            target_paths.push(codex_path);
        }
        
        // Antigravity 기본 state.vscdb 경로 (macOS)
        let vscdb_path = Path::new(&home)
            .join("Library")
            .join("Application Support")
            .join("Code")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb");
        if vscdb_path.exists() && vscdb_path.is_file() {
            println!("[Sync] Added default Antigravity path: {:?}", vscdb_path);
            target_paths.push(vscdb_path);
        }
    }
    
    let mut files = Vec::new();
    for p in target_paths {
        if p.is_file() {
            files.push(p.clone());
        } else if p.is_dir() {
            let _ = collect_files_helper(&p, &mut files);
        }
    }
    println!("[Sync] Collected {} files total", files.len());
    
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
        println!("[Sync] Processing file: {}", path_str);
        
        if is_vscdb {
            if let Ok(ids) = agent_token_tracker::adapters::antigravity::get_vscdb_session_ids(path_str) {
                println!("[Sync] vscdb session ids found: {:?}", ids);
                for id in ids {
                    if matches!(db::get_session(&conn, &id), Ok(Some(_))) {
                        println!("[Sync] vscdb session already exists: {}", id);
                        result.sessions_skipped += 1;
                        continue;
                    }
                    
                    let virtual_path_str = format!("{}?session_id={}", path_str, id);
                    let virtual_path = PathBuf::from(virtual_path_str);
                    if let Err(e) = process_watch_file(&virtual_path, &pricing_map, db_path) {
                        println!("[Sync] vscdb insert failed for {}: {:?}", id, e);
                        result.sessions_failed += 1;
                    } else {
                        println!("[Sync] vscdb insert success: {}", id);
                        result.sessions_inserted += 1;
                    }
                }
            } else {
                println!("[Sync] vscdb get ids failed for: {}", path_str);
                result.sessions_failed += 1;
            }
        } else {
            // JSONL 파싱 및 중복 검사 후 적재
            let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let is_codex = file_name.starts_with("rollout-") || file_name.contains("codex");
            
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
                    println!("[Sync] Parsed session_id: {}", session_id);
                    if matches!(db::get_session(&conn, session_id), Ok(Some(_))) {
                        println!("[Sync] Session already exists in DB: {}", session_id);
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
                        println!("[Sync] 세션 insert 에러 for {}: {}", session_id, e);
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
                    println!("[Sync] Successfully inserted session: {}", session_id);
                    result.sessions_inserted += 1;
                }
                Err(e) => {
                    println!("[Sync] Parsing failed for {}: {:?}", path_str, e);
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

#[derive(Debug, Serialize, Deserialize)]
pub struct HourlyTokenUsage {
    pub hour: String,
    pub total_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenUsageBreakdown {
    pub models: Vec<ModelTokenUsage>,
    pub plugins: Vec<PluginTokenUsage>,
    pub skills: Vec<SkillTokenUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelTokenUsage {
    pub model_id: String,
    pub total_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginTokenUsage {
    pub plugin_name: String,
    pub total_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillTokenUsage {
    pub skill_name: String,
    pub total_tokens: u64,
}

#[tauri::command]
fn get_hourly_token_usage() -> Result<Vec<HourlyTokenUsage>, String> {
    let conn = get_db_conn()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(substr(started_at, 12, 2), '00') as hour, SUM(total_input_tokens + total_output_tokens) as tokens
         FROM sessions
         GROUP BY hour
         ORDER BY hour ASC"
    ).map_err(|e| e.to_string())?;
    
    let rows = stmt.query_map([], |row| {
        Ok(HourlyTokenUsage {
            hour: row.get(0)?,
            total_tokens: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for r in rows {
        if let Ok(item) = r {
            result.push(item);
        }
    }
    
    let mut hourly_map = std::collections::HashMap::new();
    for item in result {
        hourly_map.insert(item.hour.clone(), item.total_tokens);
    }
    
    let mut interpolated = Vec::new();
    for h in 0..24 {
        let hour_str = format!("{:02}", h);
        let total_tokens = *hourly_map.get(&hour_str).unwrap_or(&0);
        interpolated.push(HourlyTokenUsage {
            hour: hour_str,
            total_tokens,
        });
    }
    
    Ok(interpolated)
}

#[tauri::command]
fn get_token_usage_breakdown() -> Result<TokenUsageBreakdown, String> {
    let conn = get_db_conn()?;
    
    let mut stmt_model = conn.prepare(
        "SELECT COALESCE(model_id, 'unknown') as model, SUM(total_input_tokens + total_output_tokens) as tokens
         FROM sessions
         GROUP BY model
         ORDER BY tokens DESC"
    ).map_err(|e| e.to_string())?;
    
    let model_rows = stmt_model.query_map([], |row| {
        Ok(ModelTokenUsage {
            model_id: row.get(0)?,
            total_tokens: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut models = Vec::new();
    for m in model_rows {
        if let Ok(item) = m {
            models.push(item);
        }
    }
    
    let mut stmt_sess = conn.prepare("SELECT session_id, total_input_tokens, total_output_tokens FROM sessions")
        .map_err(|e| e.to_string())?;
    let sess_rows = stmt_sess.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)? + row.get::<_, u64>(2)?))
    }).map_err(|e| e.to_string())?;
    
    let mut sess_map = std::collections::HashMap::new();
    for r in sess_rows {
        if let Ok((id, tokens)) = r {
            sess_map.insert(id, tokens);
        }
    }
    
    let mut stmt_tools = conn.prepare("SELECT session_id, tool_name FROM tool_calls")
        .map_err(|e| e.to_string())?;
    let tool_rows = stmt_tools.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).map_err(|e| e.to_string())?;
    
    let mut skill_tokens = std::collections::HashMap::new();
    let mut plugin_tokens = std::collections::HashMap::new();
    
    for r in tool_rows {
        if let Ok((sess_id, tool_name)) = r {
            if let Some(&tokens) = sess_map.get(&sess_id) {
                *skill_tokens.entry(tool_name.clone()).or_insert(0) += tokens;
                
                let plugin_name = if tool_name.starts_with("mcp_doxus_") || tool_name.contains("doxus") {
                    "doxus".to_string()
                } else if tool_name.starts_with("mcp_engram_") || tool_name.contains("engram") {
                    "engram".to_string()
                } else if tool_name.starts_with("mcp_playwright_") || tool_name.contains("playwright") {
                    "playwright".to_string()
                } else if tool_name.contains("android-cli") || tool_name.contains("android") {
                    "android-cli".to_string()
                } else if tool_name.contains("chrome-extensions") || tool_name.contains("chrome") {
                    "chrome-extensions".to_string()
                } else {
                    "other".to_string()
                };
                *plugin_tokens.entry(plugin_name).or_insert(0) += tokens;
            }
        }
    }
    
    let mut skills: Vec<SkillTokenUsage> = skill_tokens.into_iter().map(|(k, v)| SkillTokenUsage {
        skill_name: k,
        total_tokens: v,
    }).collect();
    skills.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
    skills.truncate(10);
    
    let mut plugins: Vec<PluginTokenUsage> = plugin_tokens.into_iter().map(|(k, v)| PluginTokenUsage {
        plugin_name: k,
        total_tokens: v,
    }).collect();
    plugins.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
    
    Ok(TokenUsageBreakdown {
        models,
        plugins,
        skills,
    })
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();

            // macOS의 경우 백그라운드 트레이 전용 모드(Accessory)로 시작
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // 메인 윈도우의 CloseRequested 이벤트를 가로채어 창을 숨기고 Accessory 모드로 복구
            if let Some(main_win) = app.get_webview_window("main") {
                let main_clone = main_win.clone();
                let app_handle_clone = app_handle.clone();
                main_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_clone.hide();
                        #[cfg(target_os = "macos")]
                        let _ = app_handle_clone.set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                });
            }

            // 팝오버 닫힘 시간 추적을 위한 스레드 안전 변수
            let last_hide = std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1)));
            let last_hide_for_blur = last_hide.clone();
            let last_hide_for_click = last_hide.clone();

            // 1. 트레이 팝오버 윈도우 획득 및 blur 이벤트 핸들링
            if let Some(popover) = app.get_webview_window("tray-popover") {
                let popover_clone = popover.clone();
                popover.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = popover_clone.hide();
                        if let Ok(mut last) = last_hide_for_blur.lock() {
                            *last = std::time::Instant::now();
                        }
                    }
                });

                // macOS의 경우 모든 가상 데스크톱(Spaces)에 창이 함께 참여하도록 활성화
                #[cfg(target_os = "macos")]
                let _ = popover.set_visible_on_all_workspaces(true);
            }

            // 2. 트레이 아이콘 초기 설정
            let icon_green_bytes = include_bytes!("../icons/icon_green.png");
            let initial_icon = tauri::image::Image::from_bytes(icon_green_bytes)
                .expect("Green icon load failed");

            let _tray = tauri::tray::TrayIconBuilder::with_id("main-tray")
                .icon(initial_icon)
                .title("$0.00")
                .on_tray_icon_event(move |tray: &tauri::tray::TrayIcon, event: tauri::tray::TrayIconEvent| {
                    if let tauri::tray::TrayIconEvent::Click { button, button_state, position, .. } = event {
                        if button == tauri::tray::MouseButton::Left && button_state == tauri::tray::MouseButtonState::Up {
                            if let Ok(last) = last_hide_for_click.lock() {
                                if last.elapsed() < std::time::Duration::from_millis(250) {
                                    return;
                                }
                            }
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
            get_daily_token_usage,
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
            sync_local_sessions,
            get_hourly_token_usage,
            get_token_usage_breakdown,
            get_subscription_quota,
            get_rolling_window_usage,
            get_session_analysis,
            get_local_credentials,
            auto_apply_credential
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 구동 중 에러 발생");
}
