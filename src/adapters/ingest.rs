//! 로그 스캔 및 DB 적재(ingest) 로직
//!
//! 에이전트 로그 경로의 자동 감지 및 멱등성 있는 데이터베이스 적재를 수행합니다.

use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub log_dir: String,
    #[serde(default)]
    pub claude_log_dir: String,
    #[serde(default)]
    pub codex_log_dir: String,
    #[serde(default)]
    pub antigravity_log_dir: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub files_total: usize,
    pub sessions_scanned: usize,
    pub sessions_inserted: usize,
    pub sessions_skipped: usize,
    pub sessions_failed: usize,
}

/// 에이전트별 로그 경로를 OS 환경 및 저장된 설정에 따라 감지합니다.
pub fn detect_default_log_paths() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut roots = Vec::new();
    
    if home.is_empty() {
        return roots;
    }

    // 1. Desktop App 설정 파일(config.json) 위치 추정 시도
    let mut config_path = PathBuf::from(&home);
    #[cfg(target_os = "macos")]
    {
        config_path.push("Library");
        config_path.push("Application Support");
        config_path.push("com.atk.desktop");
        config_path.push("config.json");
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            config_path = PathBuf::from(appdata);
            config_path.push("com.atk.desktop");
            config_path.push("config.json");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        config_path.push(".config");
        config_path.push("com.atk.desktop");
        config_path.push("config.json");
    }

    let mut settings = None;
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(s) = serde_json::from_str::<AppSettings>(&content) {
                settings = Some(s);
            }
        }
    }

    // 2. 사용자 설정 경로 취합
    if let Some(ref s) = settings {
        if !s.log_dir.is_empty() {
            roots.push(PathBuf::from(&s.log_dir));
        }
        if !s.claude_log_dir.is_empty() {
            roots.push(PathBuf::from(&s.claude_log_dir));
        }
        if !s.codex_log_dir.is_empty() {
            roots.push(PathBuf::from(&s.codex_log_dir));
        }
        if !s.antigravity_log_dir.is_empty() {
            roots.push(PathBuf::from(&s.antigravity_log_dir));
        }
    }

    // 3. 감지 경로 추가 (설정이 비어있는 경우 등을 대비해 디폴트 경로 감지)
    let claude = PathBuf::from(&home).join(".claude").join("projects");
    if claude.exists() {
        roots.push(claude);
    }

    let codex = PathBuf::from(&home).join(".codex").join("sessions");
    if codex.is_dir() {
        roots.push(codex);
    }
    let codex_archived = PathBuf::from(&home).join(".codex").join("archived_sessions");
    if codex_archived.is_dir() {
        roots.push(codex_archived);
    }

    // Antigravity candidates
    let mut anti_candidates = Vec::new();
    #[cfg(target_os = "macos")]
    {
        let base = PathBuf::from(&home).join("Library").join("Application Support");
        anti_candidates.push(base.join("Antigravity IDE").join("User").join("globalStorage").join("state.vscdb"));
        anti_candidates.push(base.join("Antigravity").join("User").join("globalStorage").join("state.vscdb"));
        anti_candidates.push(base.join("Code").join("User").join("globalStorage").join("state.vscdb"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let base = PathBuf::from(appdata);
            anti_candidates.push(base.join("Antigravity IDE").join("User").join("globalStorage").join("state.vscdb"));
            anti_candidates.push(base.join("Antigravity").join("User").join("globalStorage").join("state.vscdb"));
            anti_candidates.push(base.join("Code").join("User").join("globalStorage").join("state.vscdb"));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let base = PathBuf::from(&home).join(".config");
        anti_candidates.push(base.join("Antigravity IDE").join("User").join("globalStorage").join("state.vscdb"));
        anti_candidates.push(base.join("Antigravity").join("User").join("globalStorage").join("state.vscdb"));
        anti_candidates.push(base.join("Code").join("User").join("globalStorage").join("state.vscdb"));
    }

    for c in anti_candidates {
        if c.exists() {
            roots.push(c);
            break;
        }
    }

    roots.sort();
    roots.dedup();

    roots
}

/// 경로를 순회하며 파일 목록을 재귀적으로 수집하는 헬퍼 함수
pub fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        let path_str = path.to_str().unwrap_or("");
        if path_str.ends_with("state.vscdb") {
            match crate::adapters::antigravity::get_vscdb_session_ids(path_str) {
                Ok(ids) => {
                    for id in ids {
                        let virtual_path = format!("{}?session_id={}", path_str, id);
                        files.push(PathBuf::from(virtual_path));
                    }
                }
                Err(_) => {
                    files.push(path.to_path_buf());
                }
            }
        } else {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            collect_files(&entry.path(), files)?;
        }
    }
    Ok(())
}

/// 단일 파일에 대해 파싱 및 데이터베이스 멱등 적재를 처리합니다.
pub fn process_single_file(
    conn: &rusqlite::Connection,
    file_path: &Path,
    agent_filter: Option<&str>,
    pricing_cache: &HashMap<String, crate::model::Pricing>,
    force_update: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = file_path.to_str().unwrap_or("");
    let is_vscdb = path_str.contains("state.vscdb");
    let has_vscdb_param = path_str.contains("state.vscdb?session_id=");

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "jsonl" && !is_vscdb && !has_vscdb_param {
        return Ok(());
    }

    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let is_codex = file_name.starts_with("rollout-") || file_name.contains("codex") || agent_filter == Some("codex");
    let is_antigravity = is_vscdb || has_vscdb_param || file_name.contains("antigravity") || agent_filter == Some("antigravity");

    let parsed_res = if is_antigravity {
        let adapter = crate::adapters::antigravity::AntigravityAdapter;
        use crate::adapters::LogAdapter;
        adapter.parse_session(path_str)
    } else if is_codex {
        let adapter = crate::adapters::codex::CodexAdapter;
        use crate::adapters::LogAdapter;
        adapter.parse_session(path_str)
    } else {
        let adapter = crate::adapters::claude_code::ClaudeCodeAdapter;
        use crate::adapters::LogAdapter;
        adapter.parse_session(path_str)
    };

    let mut parsed_session = parsed_res?;

    let model_id_opt = parsed_session.session.model_id.as_deref().unwrap_or("unknown");
    let pricing_info = parsed_session.session.model_id.as_ref()
        .and_then(|m_id| pricing_cache.get(m_id));

    if pricing_info.is_none() && model_id_opt != "unknown" {
        eprintln!(
            "모델 단가 누락 경고: '{}' 모델의 단가 정보가 pricing 테이블에 없습니다. 기본 fallback 단가를 적용합니다.",
            model_id_opt
        );
    }

    for msg in &mut parsed_session.messages {
        if msg.role == "assistant" || msg.role == "agent" {
            msg.cost_usd = crate::pricing::calculate_cost_usd(
                pricing_info,
                msg.input_tokens,
                msg.cache_read_input_tokens,
                msg.cache_creation_input_tokens,
                msg.output_tokens,
            );
        }
    }

    let exists = match crate::db::get_session(conn, &parsed_session.session.session_id)? {
        Some(_) => true,
        None => false,
    };

    if exists {
        if force_update {
            crate::db::delete_session(conn, &parsed_session.session.session_id)?;
        } else {
            return Err("already_exists".into());
        }
    }

    crate::db::insert_session(conn, &parsed_session.session)?;
    for msg in &parsed_session.messages {
        crate::db::insert_message(conn, msg)?;
    }
    for node in &parsed_session.nodes {
        crate::db::insert_node(conn, node)?;
    }
    for tc in &parsed_session.tool_calls {
        crate::db::insert_tool_call(conn, tc)?;
    }

    Ok(())
}

/// 감지된 경로들을 일괄 스캔하여 DB에 반영합니다. (멱등성 보장)
pub fn ingest_logs(
    conn: &rusqlite::Connection,
    paths: &[PathBuf],
    agent_filter: Option<&str>,
    force: bool,
) -> Result<IngestResult, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for p in paths {
        let _ = collect_files(p, &mut files);
    }
    
    let files_total = files.len();
    let pricing_cache = crate::db::get_all_pricings(conn)?;
    
    let mut result = IngestResult {
        files_total,
        ..Default::default()
    };

    for f in files {
        result.sessions_scanned += 1;
        match process_single_file(conn, &f, agent_filter, &pricing_cache, force) {
            Ok(_) => {
                result.sessions_inserted += 1;
            }
            Err(e) => {
                if e.to_string() == "already_exists" {
                    result.sessions_skipped += 1;
                } else {
                    result.sessions_failed += 1;
                    eprintln!("[Ingest] 파일 처리 실패 ({}): {:?}", f.display(), e);
                }
            }
        }
    }

    Ok(result)
}
