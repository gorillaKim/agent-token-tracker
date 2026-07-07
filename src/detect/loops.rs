//! 루프 및 오작동 이상 징후 탐지 엔진 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

use serde::{Deserialize, Serialize};
use crate::model::{Session, Message, ToolCall};

/// 루프 및 이상 징후 분석 결과 구조체
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopDetectionResult {
    pub session_id: String,
    pub is_anomaly: bool,
    pub signals: Vec<LoopSignal>,
    pub is_false_positive: bool,
}

/// 탐지된 개별 이상 징후 시그널
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopSignal {
    pub signal_type: String, // "repeated_call" | "repeated_failure" | "token_inflation" | "ping_pong"
    pub description: String,
    pub evidence: String,
}

/// 탐지 임계치 설정 구조체
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub max_repeated_calls: usize,
    pub max_repeated_failures: usize,
    pub token_inflation_threshold: u64,
    pub max_ping_pong_cycles: usize,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            max_repeated_calls: 3,
            max_repeated_failures: 3,
            token_inflation_threshold: 50_000,
            max_ping_pong_cycles: 3,
        }
    }
}

/// 특정 세션의 메시지 및 도구 호출 기록을 기반으로 오작동/루프 이상 징후를 판정합니다.
pub fn detect_session_anomalies(
    session: &Session,
    _messages: &[Message],
    tool_calls: &[ToolCall],
    config: &DetectorConfig,
) -> LoopDetectionResult {
    let mut signals = Vec::new();

    // 1. 시그널 1: 동일 호출 반복 (동일 tool_name + input_hash 연속 반복)
    detect_repeated_calls(tool_calls, config.max_repeated_calls, &mut signals);

    // 2. 시그널 2: 반복 실패 (동일 도구 연속 실패)
    detect_repeated_failures(tool_calls, config.max_repeated_failures, &mut signals);

    // 3. 시그널 3: 무진전 토큰 증가
    detect_token_inflation(session, tool_calls, config.token_inflation_threshold, &mut signals);

    // 4. 시그널 4: 핑퐁 루프 (A <-> B 왕복)
    detect_ping_pong(tool_calls, config.max_ping_pong_cycles, &mut signals);

    let is_anomaly = !signals.is_empty();

    LoopDetectionResult {
        session_id: session.session_id.clone(),
        is_anomaly,
        signals,
        is_false_positive: false,
    }
}

fn detect_repeated_calls(tool_calls: &[ToolCall], threshold: usize, signals: &mut Vec<LoopSignal>) {
    if tool_calls.len() < threshold {
        return;
    }

    let mut consecutive_count = 1;
    for i in 1..tool_calls.len() {
        let prev = &tool_calls[i - 1];
        let curr = &tool_calls[i];

        if prev.tool_name == curr.tool_name && prev.input_hash == curr.input_hash {
            consecutive_count += 1;
            if consecutive_count >= threshold {
                signals.push(LoopSignal {
                    signal_type: "repeated_call".to_string(),
                    description: format!(
                        "동일 도구 동일 인자 호출 반복: '{}' 도구가 같은 인자로 연속 {}회 실행되었습니다.",
                        curr.tool_name, consecutive_count
                    ),
                    evidence: format!(
                        "tool_name={}, input_hash={}, count={}",
                        curr.tool_name, curr.input_hash, consecutive_count
                    ),
                });
                consecutive_count = 1;
            }
        } else {
            consecutive_count = 1;
        }
    }
}

fn detect_repeated_failures(tool_calls: &[ToolCall], threshold: usize, signals: &mut Vec<LoopSignal>) {
    if tool_calls.len() < threshold {
        return;
    }

    let mut consecutive_failures = 0;
    let mut current_tool = String::new();

    for tc in tool_calls {
        if !tc.success {
            if current_tool == tc.tool_name {
                consecutive_failures += 1;
            } else {
                current_tool = tc.tool_name.clone();
                consecutive_failures = 1;
            }

            if consecutive_failures >= threshold {
                signals.push(LoopSignal {
                    signal_type: "repeated_failure".to_string(),
                    description: format!(
                        "동일 도구 연속 실패: '{}' 도구가 연속 {}회 실패했습니다.",
                        tc.tool_name, consecutive_failures
                    ),
                    evidence: format!("tool_name={}, failure_count={}", tc.tool_name, consecutive_failures),
                });
                consecutive_failures = 0;
            }
        } else {
            consecutive_failures = 0;
            current_tool.clear();
        }
    }
}

fn detect_token_inflation(
    session: &Session,
    tool_calls: &[ToolCall],
    threshold: u64,
    signals: &mut Vec<LoopSignal>,
) {
    if session.total_input_tokens > threshold {
        let total_calls = tool_calls.len();
        let success_calls = tool_calls.iter().filter(|tc| tc.success).count();

        let is_inflation = if total_calls > 0 {
            (success_calls as f64) / (total_calls as f64) < 0.1
        } else {
            true
        };

        if is_inflation {
            signals.push(LoopSignal {
                signal_type: "token_inflation".to_string(),
                description: format!(
                    "무진전 토큰 급증: 누적 입력 토큰이 {}를 초과했으나 유의미한 도구 호출 성공이 저조합니다.",
                    session.total_input_tokens
                ),
                evidence: format!(
                    "total_input_tokens={}, total_tool_calls={}, success_tool_calls={}",
                    session.total_input_tokens, total_calls, success_calls
                ),
            });
        }
    }
}

fn detect_ping_pong(tool_calls: &[ToolCall], threshold: usize, signals: &mut Vec<LoopSignal>) {
    let min_len = threshold * 2;
    if tool_calls.len() < min_len {
        return;
    }

    let names: Vec<&str> = tool_calls.iter().map(|tc| tc.tool_name.as_str()).collect();
    let w_size = threshold * 2;

    for i in 0..=(names.len() - w_size) {
        let window = &names[i..i + w_size];
        let tool_a = window[0];
        let tool_b = window[1];

        if tool_a == tool_b {
            continue;
        }

        let mut is_ping_pong = true;
        for j in 0..w_size {
            let expected = if j % 2 == 0 { tool_a } else { tool_b };
            if window[j] != expected {
                is_ping_pong = false;
                break;
            }
        }

        if is_ping_pong {
            signals.push(LoopSignal {
                signal_type: "ping_pong".to_string(),
                description: format!(
                    "도구 호출 핑퐁 루프: '{}' 도구와 '{}' 도구가 교대로 {}회 연속 왕복 호출되었습니다.",
                    tool_a, tool_b, threshold
                ),
                evidence: format!("tool_A={}, tool_B={}, cycles={}", tool_a, tool_b, threshold),
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_dummy_session(id: &str, input_tokens: u64) -> Session {
        Session::new(
            id.to_string(),
            "claude_code".to_string(),
            None,
            "2026-06-23T10:00:00Z".to_string(),
            None,
            "/tmp".to_string(),
            None,
            input_tokens,
            0,
            0, // total_cache_creation_input_tokens
            "api".to_string(),
            None,
            None,
        )
    }

    #[test]
    fn test_detect_repeated_calls() {
        let sess = create_dummy_session("sess-1", 1000);
        let config = DetectorConfig {
            max_repeated_calls: 3,
            ..Default::default()
        };

        let tool_calls = vec![
            ToolCall::new("sess-1".to_string(), "view_file".to_string(), None, "hash1".to_string(), true, false, false, None, None, "1".to_string()),
            ToolCall::new("sess-1".to_string(), "view_file".to_string(), None, "hash1".to_string(), true, false, false, None, None, "2".to_string()),
            ToolCall::new("sess-1".to_string(), "view_file".to_string(), None, "hash1".to_string(), true, false, false, None, None, "3".to_string()),
        ];

        let result = detect_session_anomalies(&sess, &[], &tool_calls, &config);
        assert!(result.is_anomaly);
        assert_eq!(result.signals[0].signal_type, "repeated_call");
    }

    #[test]
    fn test_detect_repeated_failures() {
        let sess = create_dummy_session("sess-2", 1000);
        let config = DetectorConfig {
            max_repeated_failures: 3,
            ..Default::default()
        };

        let tool_calls = vec![
            ToolCall::new("sess-2".to_string(), "run_command".to_string(), None, "h1".to_string(), false, false, false, None, None, "1".to_string()),
            ToolCall::new("sess-2".to_string(), "run_command".to_string(), None, "h2".to_string(), false, false, false, None, None, "2".to_string()),
            ToolCall::new("sess-2".to_string(), "run_command".to_string(), None, "h3".to_string(), false, false, false, None, None, "3".to_string()),
        ];

        let result = detect_session_anomalies(&sess, &[], &tool_calls, &config);
        assert!(result.is_anomaly);
        assert_eq!(result.signals[0].signal_type, "repeated_failure");
    }

    #[test]
    fn test_detect_token_inflation() {
        let sess = create_dummy_session("sess-3", 60_000);
        let config = DetectorConfig {
            token_inflation_threshold: 50_000,
            ..Default::default()
        };

        let tool_calls = vec![
            ToolCall::new("sess-3".to_string(), "run_command".to_string(), None, "h1".to_string(), false, false, false, None, None, "1".to_string()),
        ];

        let result = detect_session_anomalies(&sess, &[], &tool_calls, &config);
        assert!(result.is_anomaly);
        assert_eq!(result.signals[0].signal_type, "token_inflation");
    }

    #[test]
    fn test_detect_ping_pong() {
        let sess = create_dummy_session("sess-4", 1000);
        let config = DetectorConfig {
            max_ping_pong_cycles: 3,
            ..Default::default()
        };

        let tool_calls = vec![
            ToolCall::new("sess-4".to_string(), "view_file".to_string(), None, "h".to_string(), true, false, false, None, None, "1".to_string()),
            ToolCall::new("sess-4".to_string(), "run_command".to_string(), None, "h".to_string(), true, false, false, None, None, "2".to_string()),
            ToolCall::new("sess-4".to_string(), "view_file".to_string(), None, "h".to_string(), true, false, false, None, None, "3".to_string()),
            ToolCall::new("sess-4".to_string(), "run_command".to_string(), None, "h".to_string(), true, false, false, None, None, "4".to_string()),
            ToolCall::new("sess-4".to_string(), "view_file".to_string(), None, "h".to_string(), true, false, false, None, None, "5".to_string()),
            ToolCall::new("sess-4".to_string(), "run_command".to_string(), None, "h".to_string(), true, false, false, None, None, "6".to_string()),
        ];

        let result = detect_session_anomalies(&sess, &[], &tool_calls, &config);
        assert!(result.is_anomaly);
        assert_eq!(result.signals[0].signal_type, "ping_pong");
    }
}
