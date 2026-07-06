//! Claude Code 로그 수집 어댑터 모듈
//!
//! ~/.claude/projects/ 아래에 생성되는 JSONL 세션 로그를 파싱하고 정규화합니다.
//! 사용자의 한국어 문서화 규칙에 맞춰 주석이 작성되었습니다.

use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::{LogAdapter, NormalizedSession};
use crate::model::{Message, Node, Session, ToolCall};

pub struct ClaudeCodeAdapter;

impl LogAdapter for ClaudeCodeAdapter {
    fn parse_session(&self, path: &str) -> Result<NormalizedSession, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        // 1. 기본 fallback 메타정보 설정 (파일명 기반)
        let file_name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown_session");

        let mut session_id = file_name.to_string();
        let mut agent_version = None;
        let mut started_at = "1970-01-01T00:00:00Z".to_string();
        let mut ended_at = None;
        let mut cwd = "/Unknown".to_string();
        let mut model_id = None;
        let mut total_input_tokens = 0;
        let mut total_output_tokens = 0;
        let mut total_cache_creation_tokens = 0;
        let mut session_name = None;

        let mut messages = Vec::new();
        let mut nodes = Vec::new();
        let mut tool_calls = Vec::new();
        let mut turn_index = 0;

        // 2. JSONL 줄 단위 스트리밍 순회
        for line_result in reader.lines() {
            let line = line_result?;
            if line.trim().is_empty() {
                continue;
            }

            let log_val: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue, // 포맷이 깨진 줄은 유연하게 스킵 (Graceful Degrade 정책)
            };

            // session_meta가 유실된 로그의 경우 대비: 최초 발견된 유효 타임스탬프를 started_at으로 설정 (한국어 주석 적용)
            if started_at == "1970-01-01T00:00:00Z" {
                if let Some(ts) = log_val.get("timestamp").and_then(|t| t.as_str()) {
                    started_at = ts.to_string();
                }
            }

            // type 필드 판별
            if let Some(log_type) = log_val.get("type").and_then(|t| t.as_str()) {
                match log_type {
                    "session_meta" => {
                        // 세션 메타정보 획득
                        if let Some(id) = log_val.get("id").and_then(|i| i.as_str()) {
                            session_id = id.to_string();
                        }
                        // sessionId 키 변형도 지원 (서브에이전트 로그 호환)
                        if session_id == file_name {
                            if let Some(id) = log_val.get("sessionId").and_then(|i| i.as_str()) {
                                session_id = id.to_string();
                            }
                        }
                        if let Some(dir) = log_val.get("cwd").and_then(|c| c.as_str()) {
                            cwd = dir.to_string();
                        }
                        if let Some(ts) = log_val.get("timestamp").and_then(|t| t.as_str()) {
                            started_at = ts.to_string();
                        }
                        if let Some(ver) = log_val.get("cli_version").and_then(|v| v.as_str()) {
                            agent_version = Some(ver.to_string());
                        }
                    }
                    "message" | "user" | "assistant" | "attachment" => {
                        // 서브에이전트 로그: session_meta 없이 user/attachment 이벤트에
                        // 최상위로 cwd, sessionId가 포함된 경우 보완 파싱
                        if cwd == "/Unknown" {
                            if let Some(dir) = log_val.get("cwd").and_then(|c| c.as_str()) {
                                cwd = dir.to_string();
                            }
                        }
                        if session_id == file_name {
                            if let Some(id) = log_val.get("sessionId").and_then(|i| i.as_str()) {
                                session_id = id.to_string();
                            }
                        }
                        // 메시지 및 블록 분석 (user, assistant, message 타입 지원)
                        if let Some(msg_obj) = log_val.get("message") {
                            let role = msg_obj
                                .get("role")
                                .and_then(|r| r.as_str())
                                .unwrap_or("unknown");
                            let timestamp = log_val
                                .get("timestamp")
                                .and_then(|t| t.as_str())
                                .unwrap_or(&started_at);

                            // 토큰 사용량(usage) 추출 (최상위 usage를 우선 조회하며, 없을 시 message 내부 usage 폴백)
                            let mut input_tokens = 0;
                            let mut cache_read_tokens = 0;
                            let mut cache_creation_tokens = 0;
                            let mut output_tokens = 0;

                            let usage_opt = log_val.get("usage").or_else(|| msg_obj.get("usage"));

                            if let Some(usage) = usage_opt {
                                input_tokens = usage
                                    .get("input_tokens")
                                    .and_then(|i| i.as_u64())
                                    .unwrap_or(0);
                                cache_read_tokens = usage
                                    .get("cache_read_input_tokens")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(0);
                                cache_creation_tokens = usage
                                    .get("cache_creation_input_tokens")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(0);
                                output_tokens = usage
                                    .get("output_tokens")
                                    .and_then(|o| o.as_u64())
                                    .unwrap_or(0);

                                // 누계 합산
                                total_input_tokens += input_tokens;
                                total_output_tokens += output_tokens;
                                total_cache_creation_tokens += cache_creation_tokens;
                            }

                            // model 설정
                            if let Some(m_id) = msg_obj.get("model").and_then(|m| m.as_str()) {
                                model_id = Some(m_id.to_string());
                            }

                            // content 블록 파싱하여 텍스트 결합
                            let mut text_content = String::new();
                            if let Some(content_array) = msg_obj.get("content").and_then(|c| c.as_array()) {
                                for block in content_array {
                                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
                                    match block_type {
                                        "thinking" => {
                                            if let Some(thinking_text) = block.get("thinking").and_then(|t| t.as_str()) {
                                                if !text_content.is_empty() {
                                                    text_content.push('\n');
                                                }
                                                text_content.push_str("[Thinking] ");
                                                text_content.push_str(thinking_text);
                                            }
                                        }
                                        "text" => {
                                            if let Some(text_val) = block.get("text").and_then(|t| t.as_str()) {
                                                if !text_content.is_empty() {
                                                    text_content.push('\n');
                                                }
                                                text_content.push_str(text_val);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            let msg_content_opt = if text_content.is_empty() {
                                None
                            } else {
                                Some(text_content)
                            };

                            if role == "user" && session_name.is_none() {
                                if let Some(ref text) = msg_content_opt {
                                    let clean_text = text.replace('\n', " ").trim().to_string();
                                    let name_candidate: String = clean_text.chars().take(40).collect();
                                    if !name_candidate.is_empty() {
                                        session_name = Some(name_candidate);
                                    }
                                }
                            }

                            // 턴 메시지 추가
                            let msg = Message::new(
                                session_id.clone(),
                                turn_index,
                                role.to_string(),
                                input_tokens,
                                cache_read_tokens,
                                cache_creation_tokens,
                                output_tokens,
                                0.0, // cost_usd는 추후 pricing 모듈에서 계산
                                timestamp.to_string(),
                                msg_content_opt,
                            );
                            messages.push(msg);
                            turn_index += 1;

                            // content 블록 파싱 (thinking, text, tool_use, tool_result 등)
                            let msg_tool_use_id = msg_obj.get("tool_use_id").and_then(|v| v.as_str());
                            let mut has_tool_result_block = false;

                            if let Some(content_array) =
                                msg_obj.get("content").and_then(|c| c.as_array())
                            {
                                for block in content_array {
                                    let block_type = block
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("unknown");
                                    match block_type {
                                        "thinking" | "text" => {
                                            nodes.push(Node::new(
                                                session_id.clone(),
                                                "text".to_string(),
                                                true,
                                                timestamp.to_string(),
                                            ));
                                        }
                                        "tool_use" => {
                                            nodes.push(Node::new(
                                                session_id.clone(),
                                                "tool_call".to_string(),
                                                true,
                                                timestamp.to_string(),
                                            ));

                                            if let Some(tool_name) =
                                                block.get("name").and_then(|n| n.as_str())
                                            {
                                                let tool_input_val =
                                                    block.get("input").unwrap_or(&Value::Null);

                                                // 정규화된 tool_input 획득 및 멱등 input_hash 산출
                                                let normalized_input_str =
                                                    super::normalize_tool_input(tool_input_val);
                                                let input_hash =
                                                    super::calculate_input_hash(tool_input_val);

                                                let (is_mcp, mcp_server, mcp_tool, final_tool_name) = if tool_name.starts_with("mcp__") {
                                                    let remains = &tool_name["mcp__".len()..];
                                                    if let Some((srv, tl)) = remains.split_once("__") {
                                                        (true, Some(srv.to_string()), Some(tl.to_string()), format!("{}/{}", srv, tl))
                                                    } else {
                                                        (true, Some("unknown_server".to_string()), Some(remains.to_string()), tool_name.to_string())
                                                    }
                                                } else {
                                                    (false, None, None, tool_name.to_string())
                                                };

                                                let block_id = block.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                                                let mut tc = ToolCall::new(
                                                    session_id.clone(),
                                                    final_tool_name,
                                                    Some(normalized_input_str),
                                                    input_hash,
                                                    true,
                                                    false,
                                                    is_mcp,
                                                    mcp_server,
                                                    mcp_tool,
                                                    timestamp.to_string(),
                                                );
                                                tc.tool_use_id = block_id;
                                                tool_calls.push(tc);
                                            }
                                        }
                                        "tool_result" => {
                                            has_tool_result_block = true;
                                            let tool_use_id = block.get("tool_use_id").and_then(|v| v.as_str()).map(|s| s.to_string())
                                                .or_else(|| msg_tool_use_id.map(|s| s.to_string()));
                                            let content_val = block.get("content");
                                            let content_str = match content_val {
                                                Some(Value::String(s)) => s.clone(),
                                                Some(other) => serde_json::to_string(other).unwrap_or_default(),
                                                None => "".to_string(),
                                            };
                                            if let Some(id) = tool_use_id {
                                                let char_count = content_str.chars().count() as i64;
                                                let est_tokens = (char_count + 3) / 4;
                                                if let Some(tc) = tool_calls.iter_mut().rev().find(|tc| tc.tool_use_id.as_ref() == Some(&id)) {
                                                    tc.result_char_count = Some(char_count);
                                                    tc.result_est_tokens = Some(est_tokens);
                                                } else if let Some(tc) = tool_calls.iter_mut().rev().find(|tc| tc.result_char_count.is_none()) {
                                                    tc.tool_use_id = Some(id);
                                                    tc.result_char_count = Some(char_count);
                                                    tc.result_est_tokens = Some(est_tokens);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            // Format B 대응 (role == "tool"이고 content에 tool_result 블록은 없는데 message 레벨에 tool_use_id가 있는 경우)
                            if role == "tool" && !has_tool_result_block {
                                if let Some(id) = msg_tool_use_id {
                                    let mut accumulated_text = String::new();
                                    if let Some(content_array) = msg_obj.get("content").and_then(|c| c.as_array()) {
                                        for block in content_array {
                                            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                                if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                                                    accumulated_text.push_str(txt);
                                                }
                                            }
                                        }
                                    }
                                    let char_count = accumulated_text.chars().count() as i64;
                                    let est_tokens = (char_count + 3) / 4;
                                    let id_str = id.to_string();
                                    if let Some(tc) = tool_calls.iter_mut().rev().find(|tc| tc.tool_use_id.as_ref() == Some(&id_str)) {
                                        tc.result_char_count = Some(char_count);
                                        tc.result_est_tokens = Some(est_tokens);
                                    } else if let Some(tc) = tool_calls.iter_mut().rev().find(|tc| tc.result_char_count.is_none()) {
                                        tc.tool_use_id = Some(id_str);
                                        tc.result_char_count = Some(char_count);
                                        tc.result_est_tokens = Some(est_tokens);
                                    }
                                }
                            }
                        }
                    }
                    "session_end" => {
                        // 세션 종료 시간 획득
                        if let Some(ts) = log_val.get("timestamp").and_then(|t| t.as_str()) {
                            ended_at = Some(ts.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        // 3. 최종 Session 객체 생성
        let session = Session::new(
            session_id,
            "claude_code".to_string(),
            agent_version,
            started_at,
            ended_at,
            cwd,
            model_id,
            total_input_tokens,
            total_output_tokens,
            total_cache_creation_tokens,
            "api".to_string(), // Claude Code는 실측 토큰 제공
            session_name,
            None,
        );

        Ok(NormalizedSession {
            session,
            messages,
            nodes,
            tool_calls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_claude_code_adapter_parsing() {
        // 임시 테스트용 JSONL 파일 생성 (std::env::temp_dir() 사용)
        let mut temp_path = std::env::temp_dir();
        temp_path.push("test_claude_code_session.jsonl");

        let mut temp_file = File::create(&temp_path).expect("임시 파일 생성 실패");

        let log_data = r#"{"type": "session_meta", "id": "session-xyz", "cwd": "/Users/test/dir", "timestamp": "2026-06-23T10:00:00Z", "cli_version": "0.2.1"}
{"type": "user", "timestamp": "2026-06-23T10:01:00Z", "message": {"role": "user", "content": [{"type": "text", "text": "안녕"}]}}
{"type": "assistant", "timestamp": "2026-06-23T10:01:05Z", "message": {"role": "assistant", "model": "claude-3-5-sonnet", "content": [{"type": "thinking", "thinking": "사용자 질문을 분석합니다."}, {"type": "tool_use", "name": "view_file", "input": {"AbsolutePath": "/test.txt"}}]}, "usage": {"input_tokens": 100, "cache_read_input_tokens": 40, "output_tokens": 50}}
{"type": "session_end", "timestamp": "2026-06-23T10:02:00Z"}
"#;

        write!(temp_file, "{}", log_data).expect("임시 파일 쓰기 실패");
        // 파일 쓰기 스트림 닫기
        drop(temp_file);

        let path = temp_path.to_str().unwrap();

        let adapter = ClaudeCodeAdapter;
        let result = adapter.parse_session(path).expect("세션 파싱 실패");

        // 임시 파일 삭제
        let _ = std::fs::remove_file(&temp_path);

        // 1. Session 데이터 검증
        assert_eq!(result.session.session_id, "session-xyz");
        assert_eq!(result.session.agent_type, "claude_code");
        assert_eq!(result.session.cwd, "/Users/test/dir");
        assert_eq!(result.session.started_at, "2026-06-23T10:00:00Z");
        assert_eq!(
            result.session.ended_at,
            Some("2026-06-23T10:02:00Z".to_string())
        );
        assert_eq!(result.session.agent_version, Some("0.2.1".to_string()));
        assert_eq!(result.session.total_input_tokens, 100);
        assert_eq!(result.session.total_output_tokens, 50);

        // 2. Message 데이터 검증
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[0].content, Some("안녕".to_string()));
        assert_eq!(result.messages[1].role, "assistant");
        assert_eq!(result.messages[1].input_tokens, 100);
        assert_eq!(result.messages[1].cache_read_input_tokens, 40);
        assert_eq!(result.messages[1].output_tokens, 50);
        assert_eq!(result.messages[1].content, Some("[Thinking] 사용자 질문을 분석합니다.".to_string()));

        // 3. Node 데이터 검증
        assert_eq!(result.nodes.len(), 3); // user text, assistant thinking, assistant tool_use
        assert_eq!(result.nodes[0].node_type, "text");
        assert_eq!(result.nodes[1].node_type, "text");
        assert_eq!(result.nodes[2].node_type, "tool_call");

        // 4. ToolCall 데이터 검증
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].tool_name, "view_file");
        assert_eq!(
            result.tool_calls[0].tool_input,
            Some("{\"AbsolutePath\":\"/test.txt\"}".to_string())
        );
        // input_hash가 16진수 포맷인지 검증
        assert!(!result.tool_calls[0].input_hash.is_empty());
    }

    #[test]
    fn test_claude_code_adapter_parsing_missing_session_meta() {
        // session_meta가 없는 임시 JSONL 파일 생성 (한국어 주석)
        let mut temp_path = std::env::temp_dir();
        temp_path.push("test_claude_code_missing_meta.jsonl");

        let mut temp_file = File::create(&temp_path).expect("임시 파일 생성 실패");

        // 첫 번째 이벤트의 timestamp가 fallback으로 사용되어야 함
        let log_data = r#"{"type": "message", "timestamp": "2026-06-24T12:00:00Z", "message": {"role": "user", "content": [{"type": "text", "text": "Hello"}]}}
{"type": "message", "timestamp": "2026-06-24T12:01:00Z", "message": {"role": "assistant", "model": "claude-3-5-sonnet", "usage": {"input_tokens": 50, "cache_read_input_tokens": 0, "output_tokens": 20}, "content": [{"type": "text", "text": "World"}]}}
"#;

        write!(temp_file, "{}", log_data).expect("임시 파일 쓰기 실패");
        drop(temp_file);

        let path = temp_path.to_str().unwrap();
        let adapter = ClaudeCodeAdapter;
        let result = adapter.parse_session(path).expect("세션 파싱 실패");

        // 임시 파일 삭제
        let _ = std::fs::remove_file(&temp_path);

        // 검증: started_at이 1970-01-01이 아닌 첫 번째 이벤트의 타임스탬프인 2026-06-24T12:00:00Z 여야 함
        assert_eq!(result.session.started_at, "2026-06-24T12:00:00Z");
        assert_eq!(result.session.total_input_tokens, 50);
        assert_eq!(result.session.total_output_tokens, 20);
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn test_claude_code_cache_creation_and_tool_results() {
        let mut temp_path = std::env::temp_dir();
        temp_path.push("test_claude_code_cache_tool_results.jsonl");

        let mut temp_file = File::create(&temp_path).expect("임시 파일 생성 실패");

        let log_data = r#"{"type": "session_meta", "id": "session-cache-test", "cwd": "/Users/test/dir", "timestamp": "2026-06-23T10:00:00Z", "cli_version": "0.2.1"}
{"type": "assistant", "timestamp": "2026-06-23T10:01:05Z", "message": {"role": "assistant", "model": "claude-3-5-sonnet", "content": [{"type": "tool_use", "id": "call-1", "name": "run_command", "input": {"CommandLine": "ls"}}, {"type": "tool_use", "id": "call-2", "name": "view_file", "input": {"AbsolutePath": "/test.txt"}}]}, "usage": {"input_tokens": 100, "cache_read_input_tokens": 40, "cache_creation_input_tokens": 25, "output_tokens": 50}}
{"type": "user", "timestamp": "2026-06-23T10:01:10Z", "message": {"role": "tool", "tool_use_id": "call-1", "content": [{"type": "tool_result", "tool_use_id": "call-1", "content": "file1\nfile2\n"}]}}
{"type": "user", "timestamp": "2026-06-23T10:01:15Z", "message": {"role": "tool", "tool_use_id": "call-2", "content": [{"type": "text", "text": "This is a test file content."}]}}
"#;

        write!(temp_file, "{}", log_data).expect("임시 파일 쓰기 실패");
        drop(temp_file);

        let path = temp_path.to_str().unwrap();
        let adapter = ClaudeCodeAdapter;
        let result = adapter.parse_session(path).expect("세션 파싱 실패");

        let _ = std::fs::remove_file(&temp_path);

        // 1. cache_creation_input_tokens 검증
        assert_eq!(result.session.total_cache_creation_input_tokens, 25);
        assert_eq!(result.messages[0].cache_creation_input_tokens, 25);

        // 2. ToolCall 및 tool_result 바인딩 검증
        assert_eq!(result.tool_calls.len(), 2);
        
        // call-1 (Format A: tool_result block)
        assert_eq!(result.tool_calls[0].tool_use_id, Some("call-1".to_string()));
        assert_eq!(result.tool_calls[0].result_char_count, Some(12)); // "file1\nfile2\n" -> 12 chars
        assert_eq!(result.tool_calls[0].result_est_tokens, Some(3)); // (12 + 3) / 4 = 3 tokens

        // call-2 (Format B: message tool_use_id + text content blocks)
        assert_eq!(result.tool_calls[1].tool_use_id, Some("call-2".to_string()));
        assert_eq!(result.tool_calls[1].result_char_count, Some(28)); // "This is a test file content." -> 28 chars
        assert_eq!(result.tool_calls[1].result_est_tokens, Some(7)); // (28 + 3) / 4 = 7 tokens
    }
}
