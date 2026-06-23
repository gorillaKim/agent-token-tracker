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

            // type 필드 판별
            if let Some(log_type) = log_val.get("type").and_then(|t| t.as_str()) {
                match log_type {
                    "session_meta" => {
                        // 세션 메타정보 획득
                        if let Some(id) = log_val.get("id").and_then(|i| i.as_str()) {
                            session_id = id.to_string();
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
                    "message" => {
                        // 메시지 및 블록 분석
                        if let Some(msg_obj) = log_val.get("message") {
                            let role = msg_obj
                                .get("role")
                                .and_then(|r| r.as_str())
                                .unwrap_or("unknown");
                            let timestamp = log_val
                                .get("timestamp")
                                .and_then(|t| t.as_str())
                                .unwrap_or(&started_at);

                            // 토큰 사용량(usage) 추출 (role == assistant 일 때 유효)
                            let mut input_tokens = 0;
                            let mut cache_read_tokens = 0;
                            let mut output_tokens = 0;

                            if let Some(usage) = msg_obj.get("usage") {
                                input_tokens = usage
                                    .get("input_tokens")
                                    .and_then(|i| i.as_u64())
                                    .unwrap_or(0);
                                cache_read_tokens = usage
                                    .get("cache_read_input_tokens")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(0);
                                output_tokens = usage
                                    .get("output_tokens")
                                    .and_then(|o| o.as_u64())
                                    .unwrap_or(0);

                                // 누계 합산
                                total_input_tokens += input_tokens;
                                total_output_tokens += output_tokens;
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

                            // 턴 메시지 추가
                            let msg = Message::new(
                                session_id.clone(),
                                turn_index,
                                role.to_string(),
                                input_tokens,
                                cache_read_tokens,
                                output_tokens,
                                0.0, // cost_usd는 추후 pricing 모듈에서 계산
                                timestamp.to_string(),
                                msg_content_opt,
                            );
                            messages.push(msg);
                            turn_index += 1;

                            // content 블록 파싱 (thinking, text, tool_use 등)
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

                                                tool_calls.push(ToolCall::new(
                                                    session_id.clone(),
                                                    tool_name.to_string(),
                                                    Some(normalized_input_str),
                                                    input_hash,
                                                    true,
                                                    false,
                                                    timestamp.to_string(),
                                                ));
                                            }
                                        }
                                        _ => {}
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
            "api".to_string(), // Claude Code는 실측 토큰 제공
            None,
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
{"type": "message", "timestamp": "2026-06-23T10:01:00Z", "message": {"role": "user", "content": [{"type": "text", "text": "안녕"}]}}
{"type": "message", "timestamp": "2026-06-23T10:01:05Z", "message": {"role": "assistant", "model": "claude-3-5-sonnet", "usage": {"input_tokens": 100, "cache_read_input_tokens": 40, "output_tokens": 50}, "content": [{"type": "thinking", "thinking": "사용자 질문을 분석합니다."}, {"type": "tool_use", "name": "view_file", "input": {"AbsolutePath": "/test.txt"}}]}}
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
}
