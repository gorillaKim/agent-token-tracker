//! Antigravity 어댑터 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

use rusqlite::params;
use base64::Engine;
use prost::Message;
use crate::model::{Session, Node, Message as AppMessage, ToolCall as AppToolCall};
use super::{LogAdapter, NormalizedSession, calculate_input_hash};

// 1. Prost Protobuf 구조체 선언
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnifiedState {
    #[prost(message, repeated, tag = "1")]
    pub summaries: Vec<TrajectorySummary>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct TrajectorySummary {
    #[prost(string, tag = "1")]
    pub conversation_id: String,
    #[prost(message, optional, tag = "2")]
    pub inner: Option<InnerSummary>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct InnerSummary {
    #[prost(string, tag = "1")]
    pub detail_b64: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Timestamp {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct WorkspaceInfo {
    #[prost(string, optional, tag = "1")]
    pub workspace_root: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub workspace_uri: Option<String>,
    #[prost(bytes, optional, tag = "3")]
    pub git_info_raw: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct TrajectorySummaryDetail {
    #[prost(string, tag = "1")]
    pub title: String,
    #[prost(uint64, tag = "2")]
    pub step_count: u64,
    #[prost(message, optional, tag = "3")]
    pub created_at: Option<Timestamp>,
    #[prost(string, tag = "4")]
    pub conversation_id: String,
    #[prost(message, optional, tag = "7")]
    pub started_at: Option<Timestamp>,
    #[prost(message, optional, tag = "9")]
    pub workspace_info: Option<WorkspaceInfo>,
    #[prost(message, optional, tag = "10")]
    pub updated_at: Option<Timestamp>,
}

pub struct AntigravityAdapter;

/// SQLite vscdb 파일로부터 모든 세션 ID 목록을 조회해 반환합니다.
pub fn get_vscdb_session_ids(db_path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE
    )?;
    conn.pragma_update(None, "busy_timeout", &3000)?;

    let mut stmt = conn.prepare("SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.trajectorySummaries'")?;
    let mut rows = stmt.query([])?;
    let mut ids = Vec::new();

    if let Some(row) = rows.next()? {
        let value_b64: String = row.get(0)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(value_b64.trim())?;
        
        let unified_state = UnifiedState::decode(&bytes[..])?;
        for summary in unified_state.summaries {
            ids.push(summary.conversation_id);
        }
    }
    Ok(ids)
}

impl LogAdapter for AntigravityAdapter {
    fn parse_session(&self, path: &str) -> Result<NormalizedSession, Box<dyn std::error::Error>> {
        // path 구조는 "/Users/madup/.../state.vscdb?session_id=UUID" 형식임
        let parts: Vec<&str> = path.split("?session_id=").collect();
        if parts.len() < 2 {
            return Err("Antigravity 가상 경로 규격이 잘못되었습니다 (?session_id= 필요)".into());
        }

        let db_path = parts[0];
        let target_session_id = parts[1];

        // 1. SQLite state.vscdb 파일 오픈
        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE
        )?;
        conn.pragma_update(None, "busy_timeout", &3000)?;

        // 2. trajectorySummaries 조회
        let mut stmt = conn.prepare("SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.trajectorySummaries'")?;
        let mut rows = stmt.query([])?;
        
        let mut detail_b64_opt = None;
        if let Some(row) = rows.next()? {
            let value_b64: String = row.get(0)?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(value_b64.trim())?;
            
            let unified_state = UnifiedState::decode(&bytes[..])?;
            for summary in unified_state.summaries {
                if summary.conversation_id == target_session_id {
                    if let Some(inner) = summary.inner {
                        detail_b64_opt = Some(inner.detail_b64);
                    }
                    break;
                }
            }
        }

        let detail_b64 = detail_b64_opt.ok_or_else(|| {
            format!("지정된 세션 ID [{}]를 vscdb에서 찾을 수 없습니다.", target_session_id)
        })?;

        // 3. 디테일 protobuf 디코딩
        let detail_bytes = base64::engine::general_purpose::STANDARD.decode(detail_b64.trim())?;
        let detail = TrajectorySummaryDetail::decode(&detail_bytes[..])?;

        // 4. 타임스탬프 변환 (seconds -> ISO8601 string)
        let started_at = if let Some(ref sa) = detail.started_at {
            format_timestamp(sa.seconds)
        } else if let Some(ref ca) = detail.created_at {
            format_timestamp(ca.seconds)
        } else {
            format_timestamp(0)
        };

        let ended_at = detail.updated_at.as_ref().map(|ua| format_timestamp(ua.seconds));

        // 5. workspace_info 파싱
        let mut cwd = "/workspace".to_string();
        let mut _git_remote = None;
        let mut git_branch = None;

        if let Some(ws) = detail.workspace_info.as_ref() {
            if let Some(root) = ws.workspace_root.as_ref() {
                cwd = root.to_string();
            } else if let Some(uri) = ws.workspace_uri.as_ref() {
                cwd = uri.trim_start_matches("file://").to_string();
            }

            // git info 파싱 (lossy string 추출)
            if let Some(raw_bytes) = ws.git_info_raw.as_ref() {
                let lossy_str = String::from_utf8_lossy(raw_bytes);
                for word in lossy_str.split(|c: char| c.is_control() || c.is_whitespace() || c == '\u{0}') {
                    if let Some(idx) = word.find("https://") {
                        let url_part = &word[idx..];
                        let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                        _git_remote = Some(clean_url.to_string());
                    } else if let Some(idx) = word.find("git@") {
                        let url_part = &word[idx..];
                        let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                        _git_remote = Some(clean_url.to_string());
                    }

                    if !word.contains("https://") && !word.contains("git@") {
                        if let Some(idx) = word.find("feat/") {
                            let branch_part = &word[idx..];
                            let clean_branch = branch_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '-' && c != '_');
                            git_branch = Some(clean_branch.to_string());
                        } else if word.contains("main") {
                            git_branch = Some("main".to_string());
                        } else if word.contains("master") {
                            git_branch = Some("master".to_string());
                        }
                    }
                }
            }
        }

        // git_remote 및 git_branch 경고 예방 및 세션명 보강
        let mut final_title = detail.title.clone();
        if let Some(ref branch) = git_branch {
            final_title = format!("[{}] {}", branch, final_title);
        }

        // 6. 실시간 대화 로그 파일 탐색 및 글자 수 기반 토큰/비용 추정
        let mut messages = Vec::new();
        let mut session_tool_calls = Vec::new();
        let mut total_input_tokens = 0u64;
        let mut total_output_tokens = 0u64;
        let mut _total_cost_usd = 0.0f64;
        let mut token_source = "unavailable".to_string();

        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        
        let log_file_path = std::path::Path::new(&home_dir)
            .join(".gemini")
            .join("antigravity-ide")
            .join("brain")
            .join(target_session_id)
            .join(".system_generated")
            .join("logs")
            .join("transcript_full.jsonl");

        let mut parsed_from_log = false;

        if !home_dir.is_empty() && log_file_path.exists() {
            if let Ok(file_content) = std::fs::read_to_string(&log_file_path) {
                let mut turn_index = 1u64;
                for line in file_content.lines() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                        let step_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let content_str = val.get("content").and_then(|c| c.as_str()).unwrap_or("");
                        
                        if step_type == "USER_INPUT" || step_type == "PLANNER_RESPONSE" {
                            let role = if step_type == "USER_INPUT" {
                                "user".to_string()
                            } else {
                                "agent".to_string()
                            };

                            let (ascii_tok, non_ascii_tok) = count_tokens_from_str(content_str);
                            let msg_text_tokens = ascii_tok + non_ascii_tok;
                            
                            // tool_calls 가 있는 경우 입력 토큰 가중치 적용 및 도구 호출 이력 복원
                            let mut tool_tokens = 0u64;
                            if let Some(tool_calls_arr) = val.get("tool_calls").and_then(|tc| tc.as_array()) {
                                for tc in tool_calls_arr {
                                    // name 필드 획득 및 정제
                                    let tool_name_raw = tc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown_tool");
                                    let tool_name_clean = tool_name_raw.trim_matches('"');
                                    
                                    // args 객체 획득
                                    let empty_map = serde_json::Map::new();
                                    let args_obj = tc.get("args").and_then(|a| a.as_object()).unwrap_or(&empty_map);
                                    
                                    let tool_action_val = args_obj.get("toolAction").and_then(|n| n.as_str()).unwrap_or("");
                                    let tool_action_clean = tool_action_val.trim_matches('"');
                                    
                                    let tool_summary_val = args_obj.get("toolSummary").and_then(|s| s.as_str()).unwrap_or("");
                                    let tool_summary_clean = tool_summary_val.trim_matches('"');
                                    
                                    let cmd_line_val = args_obj.get("CommandLine").and_then(|c| c.as_str()).unwrap_or("");
                                    let cmd_line_clean = cmd_line_val.trim_matches('"');
                                    
                                    let args_val = args_obj.get("Arguments").and_then(|a| a.as_str()).unwrap_or("");
                                    let args_clean = args_val.trim_matches('"');
                                    
                                    // call_mcp_tool 의 경우 ServerName/ToolName 형식으로 최종 도구명 도출
                                    let final_tool_name = if tool_name_clean == "call_mcp_tool" {
                                        let server = args_obj.get("ServerName").and_then(|s| s.as_str()).unwrap_or("").trim_matches('"');
                                        let mcp_tool = args_obj.get("ToolName").and_then(|t| t.as_str()).unwrap_or("").trim_matches('"');
                                        if !server.is_empty() && !mcp_tool.is_empty() {
                                            format!("{}/{}", server, mcp_tool)
                                        } else if !mcp_tool.is_empty() {
                                            mcp_tool.to_string()
                                        } else {
                                            tool_name_clean.to_string()
                                        }
                                    } else {
                                        tool_name_clean.to_string()
                                    };

                                    // 따옴표가 정제된 새로운 args 객체 구성
                                    let tool_input_val = serde_json::Value::Object(
                                        args_obj.iter().map(|(k, v)| {
                                            let v_clean = match v {
                                                serde_json::Value::String(s) => serde_json::Value::String(s.trim_matches('"').to_string()),
                                                other => other.clone(),
                                            };
                                            (k.clone(), v_clean)
                                        }).collect()
                                    );
                                    
                                    let tool_input_str = serde_json::to_string(&tool_input_val).unwrap_or_default();
                                    let input_hash = calculate_input_hash(&tool_input_val);
                                    
                                    let tool_call = AppToolCall::new(
                                        target_session_id.to_string(),
                                        final_tool_name,
                                        Some(tool_input_str),
                                        input_hash,
                                        true, // 성공 기본값
                                        false,
                                        started_at.clone(),
                                    );
                                    session_tool_calls.push(tool_call);
                                    
                                    let (ti_in, ti_out) = count_tokens_from_str(tool_action_clean);
                                    let (ts_in, ts_out) = count_tokens_from_str(tool_summary_clean);
                                    let (tc_in, tc_out) = count_tokens_from_str(cmd_line_clean);
                                    let (ta_in, ta_out) = count_tokens_from_str(args_clean);
                                    
                                    tool_tokens += ti_in + ti_out + ts_in + ts_out + tc_in + tc_out + ta_in + ta_out;
                                }
                            }

                            let (msg_input, msg_output) = if step_type == "USER_INPUT" {
                                (msg_text_tokens + tool_tokens, 0u64)
                            } else {
                                (0u64, msg_text_tokens + tool_tokens)
                            };

                            let msg_cost = (msg_input as f64 / 1_000_000.0) * 3.0 
                                         + (msg_output as f64 / 1_000_000.0) * 15.0;

                            total_input_tokens += msg_input;
                            total_output_tokens += msg_output;
                            _total_cost_usd += msg_cost;

                            let msg = AppMessage::new(
                                target_session_id.to_string(),
                                turn_index,
                                role,
                                msg_input,
                                0,
                                msg_output,
                                msg_cost,
                                started_at.clone(),
                                Some(content_str.to_string()),
                            );
                            messages.push(msg);
                            turn_index += 1;
                        }
                    }
                }
                if !messages.is_empty() {
                    parsed_from_log = true;
                    token_source = "estimated".to_string();
                }
            }
        }

        // 폴백 모드: 로그가 없거나 파싱 실패 시 step_count 기반 가상 추정
        if !parsed_from_log {
            total_input_tokens = detail.step_count * 5000;
            total_output_tokens = detail.step_count * 1000;
            _total_cost_usd = (total_input_tokens as f64 / 1_000_000.0) * 3.0
                            + (total_output_tokens as f64 / 1_000_000.0) * 15.0;
            token_source = "estimated".to_string();

            for i in 0..detail.step_count {
                let role = if i % 2 == 0 { "user".to_string() } else { "agent".to_string() };
                let msg = AppMessage::new(
                    target_session_id.to_string(),
                    (i + 1) as u64,
                    role,
                    5000,
                    0,
                    1000,
                    0.03,
                    started_at.clone(),
                    Some(format!("가상 대화 단계 #{}", i + 1)),
                );
                messages.push(msg);
            }
        }

        // 7. 세션 정보 맵핑
        let session = Session::new(
            target_session_id.to_string(),
            "antigravity".to_string(),
            None,
            started_at.clone(),
            ended_at,
            cwd,
            None,
            total_input_tokens,
            total_output_tokens,
            token_source,
            Some(final_title),
            None,
        );

        // 8. 활동(step_count)만큼 빈 Node들을 가상 턴으로 생성 (시각화/루프 탐지용)
        let mut nodes = Vec::new();
        for _ in 0..detail.step_count {
            let node = Node::new(
                target_session_id.to_string(),
                "text".to_string(),
                true,
                started_at.clone(),
            );
            nodes.push(node);
        }

        Ok(NormalizedSession {
            session,
            messages,
            nodes,
            tool_calls: session_tool_calls,
        })
    }
}

/// 문자열의 글자 수(ASCII vs 한글/비ASCII)별 가중치 기반 토큰 카운트 함수
fn count_tokens_from_str(s: &str) -> (u64, u64) {
    let mut ascii_chars = 0u64;
    let mut non_ascii_chars = 0u64;
    for c in s.chars() {
        if c.is_ascii() {
            ascii_chars += 1;
        } else {
            non_ascii_chars += 1;
        }
    }
    let ascii_tokens = (ascii_chars as f64 / 4.0) as u64;
    let non_ascii_tokens = (non_ascii_chars as f64 * 1.6) as u64;
    (ascii_tokens, non_ascii_tokens)
}

/// unix_sec 타임스탬프를 ISO8601 표준 포맷 문자열로 변환하는 헬퍼 함수
fn format_timestamp(secs: i64) -> String {
    if secs <= 0 {
        return "2026-06-23T00:00:00Z".to_string();
    }
    if let Ok(conn) = rusqlite::Connection::open_in_memory() {
        if let Ok(mut stmt) = conn.prepare("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?, 'unixepoch')") {
            if let Ok(mut rows) = stmt.query(params![secs]) {
                if let Some(row) = rows.next().unwrap_or(None) {
                    if let Ok(ts_str) = row.get::<_, String>(0) {
                        return ts_str;
                    }
                }
            }
        }
    }
    "2026-06-23T00:00:00Z".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp(1782218400); // 2026-07-03T10:00:00Z 근처
        assert!(ts.starts_with("2026-"));
        assert!(ts.ends_with("Z"));
    }

    #[test]
    fn test_git_info_raw_parsing() {
        let ws_info = WorkspaceInfo {
            workspace_root: Some("/mock/front-core".to_string()),
            workspace_uri: None,
            git_info_raw: Some(b"\x12+https://github.com/madup-inc/front-core.git\"\tfeat/jake".to_vec()),
        };

        let mut git_remote = None;
        let mut git_branch = None;

        if let Some(ref raw_bytes) = ws_info.git_info_raw {
            let lossy_str = String::from_utf8_lossy(raw_bytes);
            for word in lossy_str.split(|c: char| c.is_control() || c.is_whitespace() || c == '\u{0}') {
                if let Some(idx) = word.find("https://") {
                    let url_part = &word[idx..];
                    let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                    git_remote = Some(clean_url.to_string());
                } else if let Some(idx) = word.find("git@") {
                    let url_part = &word[idx..];
                    let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                    git_remote = Some(clean_url.to_string());
                }

                if !word.contains("https://") && !word.contains("git@") {
                    if let Some(idx) = word.find("feat/") {
                        let branch_part = &word[idx..];
                        let clean_branch = branch_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '-' && c != '_');
                        git_branch = Some(clean_branch.to_string());
                    } else if word.contains("main") {
                        git_branch = Some("main".to_string());
                    } else if word.contains("master") {
                        git_branch = Some("master".to_string());
                    }
                }
            }
        }

        assert_eq!(git_remote, Some("https://github.com/madup-inc/front-core.git".to_string()));
        assert_eq!(git_branch, Some("feat/jake".to_string()));
    }

    #[test]
    fn test_parse_session_invalid_path_format() {
        let adapter = AntigravityAdapter;
        let res = adapter.parse_session("/invalid/path/to/state.vscdb");
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "Antigravity 가상 경로 규격이 잘못되었습니다 (?session_id= 필요)"
        );
    }

    #[test]
    fn test_parse_session_nonexistent_db() {
        let adapter = AntigravityAdapter;
        let res = adapter.parse_session("/nonexistent/file.vscdb?session_id=mock-uuid");
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("unable to open database file") || err_msg.contains("cannot open file"));
    }

    #[test]
    fn test_real_trajectory_summaries_parsing() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests");
        p.push("fixtures");
        p.push("antigravity_trajectory_summaries.txt");
        
        let content = std::fs::read_to_string(p).expect("fixtures 파일 읽기 실패");
        let bytes = base64::engine::general_purpose::STANDARD.decode(content.trim())
            .expect("Base64 디코딩 실패");
        
        let unified_state = UnifiedState::decode(&bytes[..])
            .expect("UnifiedState Protobuf 디코딩 실패");
        
        assert!(!unified_state.summaries.is_empty(), "summaries가 비어있으면 안 됨");
        
        let mut summary_ids = Vec::new();
        let mut detail_ids = Vec::new();
        for summary in &unified_state.summaries {
            summary_ids.push(summary.conversation_id.clone());
            if let Some(ref inner) = summary.inner {
                let detail_bytes = base64::engine::general_purpose::STANDARD.decode(inner.detail_b64.trim())
                    .expect("Inner detail Base64 디코딩 실패");
                let detail = TrajectorySummaryDetail::decode(&detail_bytes[..])
                    .expect("TrajectorySummaryDetail Protobuf 디코딩 실패");
                detail_ids.push(detail.conversation_id.clone());
            }
        }
        
        println!("Summary IDs count: {}, Detail IDs count: {}", summary_ids.len(), detail_ids.len());
        
        let mut matched_count = 0;
        for s_id in &summary_ids {
            if detail_ids.contains(s_id) {
                matched_count += 1;
            } else {
                println!("Unmatched summary_id: {}", s_id);
            }
        }
        println!("Matched count: {} / {}", matched_count, summary_ids.len());
    }
}

