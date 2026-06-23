//! 루프 및 이상 징후 탐지 엔진 통합 테스트 (이슈 #707)
//!
//! `tests/fixtures/` 경로의 합성/실제 루프 픽스처 데이터를 활용하여
//! `detect_session_anomalies`의 동작 및 임계치 회귀(threshold regression)를 검증합니다.

use agent_token_tracker::adapters::claude_code::ClaudeCodeAdapter;
use agent_token_tracker::adapters::codex::CodexAdapter;
use agent_token_tracker::adapters::LogAdapter;
use agent_token_tracker::detect::loops::{
    detect_session_anomalies, DetectorConfig,
};

// ────────────────────────────────────────────────────────────
// 헬퍼: 픽스처 파일의 절대 경로 획득
// ────────────────────────────────────────────────────────────
fn fixture_path(name: &str) -> String {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p.to_str().unwrap().to_string()
}

// ════════════════════════════════════════════════════════════
// 1. 합성 픽스처 검증 (양성 케이스)
// ════════════════════════════════════════════════════════════

/// [LOOP-01] 동일 도구 동일 인자 호출 반복 시그널 탐지 검증
#[test]
fn test_loop_repeated_calls_detection() {
    let adapter = ClaudeCodeAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_repeated_calls.jsonl"))
        .expect("repeated_calls 픽스처 파싱 실패");

    let config = DetectorConfig::default(); // max_repeated_calls = 3
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    assert!(result.is_anomaly, "동일 호출 반복 이상 징후가 탐지되어야 함");
    let has_signal = result
        .signals
        .iter()
        .any(|s| s.signal_type == "repeated_call");
    assert!(has_signal, "repeated_call 시그널이 포함되어야 함");
}

/// [LOOP-02] 동일 도구 연속 실패 시그널 탐지 검증
#[test]
fn test_loop_repeated_failures_detection() {
    let adapter = CodexAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_repeated_failures.jsonl"))
        .expect("repeated_failures 픽스처 파싱 실패");

    let config = DetectorConfig::default(); // max_repeated_failures = 3
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    assert!(result.is_anomaly, "동일 도구 연속 실패 이상 징후가 탐지되어야 함");
    let has_signal = result
        .signals
        .iter()
        .any(|s| s.signal_type == "repeated_failure");
    assert!(has_signal, "repeated_failure 시그널이 포함되어야 함");
}

/// [LOOP-03] 무진전 토큰 급증 시그널 탐지 검증
#[test]
fn test_loop_token_inflation_detection() {
    let adapter = CodexAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_token_inflation.jsonl"))
        .expect("token_inflation 픽스처 파싱 실패");

    let config = DetectorConfig::default(); // token_inflation_threshold = 50,000
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    assert!(result.is_anomaly, "무진전 토큰 급증 이상 징후가 탐지되어야 함");
    let has_signal = result
        .signals
        .iter()
        .any(|s| s.signal_type == "token_inflation");
    assert!(has_signal, "token_inflation 시그널이 포함되어야 함");
}

/// [LOOP-04] 도구 호출 핑퐁 루프 시그널 탐지 검증
#[test]
fn test_loop_ping_pong_detection() {
    let adapter = ClaudeCodeAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_ping_pong.jsonl"))
        .expect("ping_pong 픽스처 파싱 실패");

    let config = DetectorConfig::default(); // max_ping_pong_cycles = 3
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    assert!(result.is_anomaly, "핑퐁 루프 이상 징후가 탐지되어야 함");
    let has_signal = result
        .signals
        .iter()
        .any(|s| s.signal_type == "ping_pong");
    assert!(has_signal, "ping_pong 시그널이 포함되어야 함");
}

// ════════════════════════════════════════════════════════════
// 2. 음성 케이스 (정상 세션) 및 복합 실제 케이스 검증
// ════════════════════════════════════════════════════════════

/// [LOOP-05] 정상 세션의 음성 케이스 검증 (이상 징후가 없어야 함)
#[test]
fn test_loop_anomaly_free_case() {
    let adapter = ClaudeCodeAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_anomaly_free.jsonl"))
        .expect("anomaly_free 픽스처 파싱 실패");

    let config = DetectorConfig::default();
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    assert!(!result.is_anomaly, "정상 세션은 이상 징후로 판정되면 안 됨");
    assert_eq!(result.signals.len(), 0, "탐지된 시그널이 0개여야 함");
}

/// [LOOP-06] 실제 디버깅 과정에서의 루핑 세션 익명화 픽스처 검증 (복합 탐지)
#[test]
fn test_loop_real_anonymized_detection() {
    let adapter = CodexAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_real_anonymized.jsonl"))
        .expect("real_anonymized 픽스처 파싱 실패");

    let config = DetectorConfig::default();
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    assert!(result.is_anomaly, "실제 익명화 루프 세션은 이상 징후로 탐지되어야 함");
    
    // 이 픽스처는 동일 파일에 대한 view_file 4회 호출과, 50,000 초과 토큰 상황을 모두 가집니다.
    let has_repeated = result.signals.iter().any(|s| s.signal_type == "repeated_call");
    let has_inflation = result.signals.iter().any(|s| s.signal_type == "token_inflation");

    assert!(has_repeated, "repeated_call 시그널이 탐지되어야 함");
    assert!(has_inflation, "token_inflation 시그널이 탐지되어야 함");
}

// ════════════════════════════════════════════════════════════
// 3. 임계치 설정 회귀 검증 (Threshold Regression)
// ════════════════════════════════════════════════════════════

/// [LOOP-07] 동일 호출 반복 임계치를 높였을 때 (3 -> 4) 미탐지 검증
#[test]
fn test_threshold_regression_repeated_calls() {
    let adapter = ClaudeCodeAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_repeated_calls.jsonl"))
        .expect("파싱 실패");

    // 픽스처 내의 연속 호출 횟수는 정확히 3회입니다.
    // 임계값을 4로 설정하면 탐지되지 않아야 합니다.
    let config = DetectorConfig {
        max_repeated_calls: 4,
        ..DetectorConfig::default()
    };
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    let has_signal = result.signals.iter().any(|s| s.signal_type == "repeated_call");
    assert!(!has_signal, "임계치를 4로 상향 시 repeated_call이 탐지되지 않아야 함");
}

/// [LOOP-08] 동일 도구 연속 실패 임계치를 높였을 때 (3 -> 4) 미탐지 검증
#[test]
fn test_threshold_regression_repeated_failures() {
    let adapter = CodexAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_repeated_failures.jsonl"))
        .expect("파싱 실패");

    // 픽스처 내의 연속 실패 횟수는 정확히 3회입니다.
    // 임계값을 4로 설정하면 탐지되지 않아야 합니다.
    let config = DetectorConfig {
        max_repeated_failures: 4,
        ..DetectorConfig::default()
    };
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    let has_signal = result.signals.iter().any(|s| s.signal_type == "repeated_failure");
    assert!(!has_signal, "임계치를 4로 상향 시 repeated_failure가 탐지되지 않아야 함");
}

/// [LOOP-09] 토큰 임계치를 높였을 때 (50,000 -> 70,000) 미탐지 검증
#[test]
fn test_threshold_regression_token_inflation() {
    let adapter = CodexAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_token_inflation.jsonl"))
        .expect("파싱 실패");

    // 픽스처 내의 입력 토큰 총합은 60,000입니다.
    // 임계값을 70,000으로 설정하면 탐지되지 않아야 합니다.
    let config = DetectorConfig {
        token_inflation_threshold: 70_000,
        ..DetectorConfig::default()
    };
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    let has_signal = result.signals.iter().any(|s| s.signal_type == "token_inflation");
    assert!(!has_signal, "임계치를 70,000으로 상향 시 token_inflation이 탐지되지 않아야 함");
}

/// [LOOP-10] 핑퐁 루프 왕복 사이클 임계치를 높였을 때 (3 -> 4) 미탐지 검증
#[test]
fn test_threshold_regression_ping_pong() {
    let adapter = ClaudeCodeAdapter;
    let norm = adapter
        .parse_session(&fixture_path("loop_ping_pong.jsonl"))
        .expect("파싱 실패");

    // 픽스처 내의 핑퐁 사이클 횟수는 정확히 3회입니다. (A->B 가 3회 왕복, 총 6개 호출)
    // 임계값을 4로 설정하면 탐지되지 않아야 합니다. (4회 왕복은 총 8개 호출 필요)
    let config = DetectorConfig {
        max_ping_pong_cycles: 4,
        ..DetectorConfig::default()
    };
    let result = detect_session_anomalies(&norm.session, &norm.messages, &norm.tool_calls, &config);

    let has_signal = result.signals.iter().any(|s| s.signal_type == "ping_pong");
    assert!(!has_signal, "임계치를 4로 상향 시 ping_pong이 탐지되지 않아야 함");
}
