//! Codex 어댑터 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use serde_json::Value;
use rusqlite::params;
use crate::model::{Session, Message, Node, ToolCall};
use super::{LogAdapter, NormalizedSession, calculate_input_hash};

pub struct CodexAdapter;

impl LogAdapter for CodexAdapter {
    fn parse_session(&self, path: &str) -> Result<NormalizedSession, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut session_id = String::new();
        let mut started_at = String::new();
        let mut ended_at: Option<String> = None;
        let mut cwd = String::new();
        let mut agent_version: Option<String> = None;
        let mut model_id: Option<String> = None;

        let mut total_input_tokens = 0;
        let mut total_output_tokens = 0;

        let mut messages = Vec::new();
        let mut nodes = Vec::new();
        let mut tool_calls = Vec::new();

        let mut turn_index = 0;

        for line_res in reader.lines() {
            let line = line_res?;
            if line.trim().is_empty() {
                continue;
            }

            let mut log_val: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue, // 포맷 오류 시 조용히 스킵 (degrade 정책)
            };

            // 최상위 type이 "event_msg"이고, payload 필드가 존재하면 payload 내부 내용을 최상위로 끌어올림
            if log_val.get("type").and_then(|t| t.as_str()) == Some("event_msg") {
                if let Some(payload) = log_val.get("payload").cloned() {
                    if let Some(payload_obj) = payload.as_object() {
                        if let Some(log_obj) = log_val.as_object_mut() {
                            for (k, v) in payload_obj {
                                log_obj.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }

            let event_type = log_val.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match event_type {
                "session_meta" => {
                    if let Some(payload) = log_val.get("payload") {
                        if let Some(id) = payload.get("id").and_then(|i| i.as_str()) {
                            session_id = id.to_string();
                        }
                        if let Some(ts) = payload.get("timestamp").and_then(|t| t.as_str()) {
                            started_at = ts.to_string();
                        }
                        if let Some(dir) = payload.get("cwd").and_then(|c| c.as_str()) {
                            cwd = dir.to_string();
                        }
                        if let Some(ver) = payload.get("cli_version").and_then(|v| v.as_str()) {
                            agent_version = Some(ver.to_string());
                        }
                    }
                }
                "turn_context" => {
                    if let Some(payload) = log_val.get("payload") {
                        if let Some(model) = payload.get("model").and_then(|m| m.as_str()) {
                            model_id = Some(model.to_string());
                        }
                    }
                }
                "task_started" => {
                    turn_index += 1;
                }
                "token_count" => {
                    if let Some(info) = log_val.get("info") {
                        if let Some(last) = info.get("last_token_usage") {
                            let mut in_t = last.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let cache_t = last.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let out_t = last.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let tot_t = last.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

                            if in_t == 0 && out_t == 0 && tot_t > 0 {
                                in_t = tot_t;
                            }

                            if in_t > 0 || out_t > 0 {
                                let msg = Message::new(
                                    session_id.clone(),
                                    turn_index,
                                    "assistant".to_string(),
                                    in_t,
                                    cache_t,
                                    out_t,
                                    0.0,
                                    started_at.clone(),
                                    None,
                                );
                                messages.push(msg);
                            }
                        }

                        if let Some(total) = info.get("total_token_usage") {
                            let in_t = total.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let out_t = total.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            let tot_t = total.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

                            if in_t == 0 && out_t == 0 && tot_t > 0 {
                                total_input_tokens = tot_t;
                                total_output_tokens = 0;
                            } else {
                                total_input_tokens = in_t;
                                total_output_tokens = out_t;
                            }
                        }
                    }
                }
                "mcp_tool_call_end" => {
                    if let Some(invocation) = log_val.get("invocation") {
                        let server = invocation.get("server").and_then(|s| s.as_str()).unwrap_or("");
                        let tool = invocation.get("tool").and_then(|t| t.as_str()).unwrap_or("");
                        let tool_name = if server.is_empty() {
                            tool.to_string()
                        } else {
                            format!("{}/{}", server, tool)
                        };

                        let arguments = invocation.get("arguments").unwrap_or(&Value::Null);
                        let tool_input = Some(serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string()));
                        let input_hash = calculate_input_hash(arguments);

                        let success = log_val.get("result")
                            .and_then(|r| r.get("Ok"))
                            .is_some();

                        let node = Node::new(
                            session_id.clone(),
                            "tool_call".to_string(),
                            success,
                            started_at.clone(),
                        );
                        nodes.push(node);

                        let tc = ToolCall::new(
                            session_id.clone(),
                            tool_name,
                            tool_input,
                            input_hash,
                            success,
                            false,
                            true, // is_mcp
                            Some(server.to_string()),
                            Some(tool.to_string()),
                            started_at.clone(),
                        );
                        tool_calls.push(tc);
                    }
                }
                "patch_apply_end" => {
                    let success = log_val.get("success").and_then(|s| s.as_bool()).unwrap_or(true);
                    let node = Node::new(
                        session_id.clone(),
                        "patch".to_string(),
                        success,
                        started_at.clone(),
                    );
                    nodes.push(node);
                }
                _ => {}
            }
        }

        if session_id.is_empty() {
            return Err("필수 필드인 session_id가 누락되었습니다.".into());
        }

        let mut session_name: Option<String> = None;
        let mut parent_session_id: Option<String> = None;

        // 메타데이터 보강 (session_index.jsonl 및 sqlite 계보)
        // 에러를 무시하는 Graceful Degrade 적용
        enhance_session_metadata(
            &session_id,
            &mut session_name,
            &mut ended_at,
            &mut parent_session_id
        ).ok();

        let session = Session::new(
            session_id,
            "codex".to_string(),
            agent_version,
            started_at,
            ended_at,
            cwd,
            model_id,
            total_input_tokens,
            total_output_tokens,
            "api".to_string(),
            session_name,
            parent_session_id,
        );

        Ok(NormalizedSession {
            session,
            messages,
            nodes,
            tool_calls,
        })
    }
}

/// 홈 디렉토리 ~/.codex 에서 외부 파일들을 참고하여 세션 정보를 보강합니다.
fn enhance_session_metadata(
    session_id: &str,
    session_name: &mut Option<String>,
    ended_at: &mut Option<String>,
    parent_session_id: &mut Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let home_str = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "홈 디렉토리를 찾을 수 없습니다.")?;
    let codex_dir = Path::new(&home_str).join(".codex");
    if !codex_dir.exists() {
        return Ok(());
    }

    // 1. session_index.jsonl 에서 세션명(thread_name) 및 종료시간(updated_at) 보강
    let index_path = codex_dir.join("session_index.jsonl");
    if index_path.exists() {
        if let Ok(file) = File::open(&index_path) {
            let reader = BufReader::new(file);
            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    if let Ok(val) = serde_json::from_str::<Value>(&line) {
                        if let Some(id) = val.get("id").and_then(|i| i.as_str()) {
                            if id == session_id {
                                if let Some(name) = val.get("thread_name").and_then(|t| t.as_str()) {
                                    *session_name = Some(name.to_string());
                                }
                                if let Some(updated) = val.get("updated_at").and_then(|u| u.as_str()) {
                                    *ended_at = Some(updated.to_string());
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. state_5.sqlite 에서 부모 세션 ID 쿼리
    let db_path = codex_dir.join("state_5.sqlite");
    if db_path.exists() {
        if let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE
        ) {
            conn.pragma_update(None, "busy_timeout", &3000).ok();
            
            let mut stmt = conn.prepare(
                "SELECT parent_thread_id FROM thread_spawn_edges WHERE child_thread_id = ?1"
            )?;
            let mut rows = stmt.query(params![session_id])?;
            if let Some(row) = rows.next()? {
                let parent: String = row.get(0)?;
                *parent_session_id = Some(parent);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_codex_adapter_parsing() {
        // 임시 rollout jsonl 파일 생성
        let mut temp_path = std::env::temp_dir();
        temp_path.push("test_codex_session.jsonl");

        let mut temp_file = File::create(&temp_path).expect("임시 파일 생성 실패");
        writeln!(
            temp_file,
            r#"{{"type":"session_meta","payload":{{"id":"test-uuid-999","timestamp":"2026-06-23T11:00:00Z","cwd":"/work","cli_version":"0.2.0"}}}}"#
        ).unwrap();
        writeln!(
            temp_file,
            r#"{{"type":"turn_context","payload":{{"model":"claude-3-5-sonnet"}}}}"#
        ).unwrap();
        writeln!(temp_file, r#"{{"type":"task_started","turn_id":"t1"}}"#).unwrap();
        // 1. 기존 token_count 포맷
        writeln!(
            temp_file,
            r#"{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":100,"cached_input_tokens":20,"output_tokens":50}},"total_token_usage":{{"input_tokens":100,"output_tokens":50}}}}}}"#
        ).unwrap();
        // 2. 신규 event_msg 래핑된 token_count 포맷
        writeln!(
            temp_file,
            r#"{{"type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":150,"cached_input_tokens":30,"output_tokens":70}},"total_token_usage":{{"input_tokens":250,"output_tokens":120}}}}}}}}"#
        ).unwrap();
        writeln!(
            temp_file,
            r#"{{"type":"mcp_tool_call_end","invocation":{{"server":"test_server","tool":"test_tool","arguments":{{"key":"val"}}}},"result":{{"Ok":{{"content":[]}}}}}}"#
        ).unwrap();
        writeln!(
            temp_file,
            r#"{{"type":"patch_apply_end","success":true}}"#
        ).unwrap();

        drop(temp_file);

        let path = temp_path.to_str().unwrap();
        let adapter = CodexAdapter;
        let result = adapter.parse_session(path).unwrap();

        let _ = std::fs::remove_file(&temp_path);

        assert_eq!(result.session.session_id, "test-uuid-999");
        assert_eq!(result.session.agent_type, "codex");
        assert_eq!(result.session.agent_version, Some("0.2.0".to_string()));
        assert_eq!(result.session.started_at, "2026-06-23T11:00:00Z");
        assert_eq!(result.session.cwd, "/work");
        assert_eq!(result.session.model_id, Some("claude-3-5-sonnet".to_string()));
        assert_eq!(result.session.total_input_tokens, 250);
        assert_eq!(result.session.total_output_tokens, 120);

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].input_tokens, 100);
        assert_eq!(result.messages[0].cache_read_input_tokens, 20);
        assert_eq!(result.messages[0].output_tokens, 50);

        assert_eq!(result.messages[1].input_tokens, 150);
        assert_eq!(result.messages[1].cache_read_input_tokens, 30);
        assert_eq!(result.messages[1].output_tokens, 70);

        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].tool_name, "test_server/test_tool");
        assert!(result.tool_calls[0].success);

        assert_eq!(result.nodes.len(), 2); // tool_call 노드 + patch 노드
    }
}
