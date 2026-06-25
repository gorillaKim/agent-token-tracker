use std::fs::File;
use std::path::{Path, PathBuf};
use rusqlite::Connection;
use std::collections::HashMap;

use agent_token_tracker::db;
use agent_token_tracker::pricing;
use agent_token_tracker::adapters::{
    LogAdapter,
    codex::CodexAdapter,
};

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

fn process_file(
    file_path: &Path,
    pricing_cache: &HashMap<String, agent_token_tracker::model::Pricing>,
    db_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = file_path.to_str().unwrap_or("");
    let adapter = CodexAdapter;
    let mut parsed_session = adapter.parse_session(path_str)?;

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

    let conn = Connection::open(db_path)?;
    let _ = conn.pragma_update(None, "foreign_keys", "ON");
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "busy_timeout", &5000);

    // 기존 세션이 있다면 삭제 후 재생성 (덮어쓰기)
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "../atk.db";
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Err("HOME env not set".into());
    }

    let codex_path = Path::new(&home).join(".codex").join("sessions");
    if !codex_path.exists() {
        println!("Codex sessions path does not exist: {:?}", codex_path);
        return Ok(());
    }

    let mut files = Vec::new();
    collect_files_helper(&codex_path, &mut files).map_err(|e| e.to_string())?;
    println!("Collected {} files from {:?}", files.len(), codex_path);

    let conn = Connection::open(db_path)?;
    let pricing_map = db::get_all_pricings(&conn)?;

    let mut success_count = 0;
    let mut fail_count = 0;

    for file in files {
        let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with("rollout-") && file.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            println!("Processing: {}", file.display());
            match process_file(&file, &pricing_map, db_path) {
                Ok(_) => {
                    success_count += 1;
                }
                Err(e) => {
                    println!("Failed to process {}: {:?}", file.display(), e);
                    fail_count += 1;
                }
            }
        }
    }

    println!("Sync complete: {} succeeded, {} failed", success_count, fail_count);
    Ok(())
}
