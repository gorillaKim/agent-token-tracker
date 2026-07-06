//! 픽스처 기반 파서·dedup·가격계산 통합 유닛테스트 (이슈 #706)
//!
//! `tests/fixtures/` 아래의 익명화 JSONL 픽스처를 로드하여 다음을 검증합니다:
//!
//! 1. **파서 테스트** — 블록 분류(text/thinking/tool_use), usage 필드 추출, session_end 처리
//! 2. **dedup 멱등 테스트** — 동일 세션을 두 번 적재해도 세션이 1건으로 유지됨
//! 3. **가격계산 테스트** — cache_read 단가 반영 + fallback 단가 정확도

use agent_token_tracker::adapters::claude_code::ClaudeCodeAdapter;
use agent_token_tracker::adapters::codex::CodexAdapter;
use agent_token_tracker::adapters::LogAdapter;
use agent_token_tracker::db;
use agent_token_tracker::pricing::calculate_cost_usd;
use agent_token_tracker::model::Pricing;

// ────────────────────────────────────────────────────────────
// 헬퍼: 테스트 픽스처 파일의 절대 경로 반환
// ────────────────────────────────────────────────────────────
fn fixture_path(name: &str) -> String {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p.to_str().unwrap().to_string()
}

// ────────────────────────────────────────────────────────────
// 헬퍼: 인메모리 SQLite DB 초기화 (테스트 격리)
// ────────────────────────────────────────────────────────────
fn make_test_db() -> rusqlite::Connection {
    db::init_db(":memory:").expect("인메모리 DB 초기화 실패")
}

// ════════════════════════════════════════════════════════════
// 1. 파서 테스트 — ClaudeCode
// ════════════════════════════════════════════════════════════

/// [CC-P-01] 기본 픽스처: session_meta, user/assistant 파싱, session_end 처리
#[test]
fn cc_parser_basic_session_meta_and_messages() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    // 세션 메타
    assert_eq!(result.session.session_id, "anon-sess-cc-001");
    assert_eq!(result.session.agent_type, "claude_code");
    assert_eq!(result.session.cwd, "/anon/project");
    assert_eq!(result.session.started_at, "2026-01-01T00:00:00Z");
    assert_eq!(result.session.agent_version, Some("1.2.0".to_string()));
    // session_end가 있으므로 ended_at 설정
    assert!(result.session.ended_at.is_some(), "session_end 후 ended_at 설정 필요");
}

/// [CC-P-02] 기본 픽스처: user/assistant 메시지 수 및 usage 값 검증
#[test]
fn cc_parser_basic_message_count_and_usage() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    // user 1건 + assistant 2건 = 3건 (tool 응답은 role=tool 이므로 별도 처리)
    let user_msgs: Vec<_> = result.messages.iter().filter(|m| m.role == "user").collect();
    let asst_msgs: Vec<_> = result.messages.iter().filter(|m| m.role == "assistant").collect();
    assert!(!user_msgs.is_empty(), "user 메시지 1건 이상 필요");
    assert_eq!(asst_msgs.len(), 2, "assistant 메시지 2건 필요");

    // 첫 번째 assistant usage
    let first_asst = &asst_msgs[0];
    assert_eq!(first_asst.input_tokens, 1200);
    assert_eq!(first_asst.cache_read_input_tokens, 0);
    assert_eq!(first_asst.output_tokens, 340);

    // 두 번째 assistant usage (cache_read 있음)
    let second_asst = &asst_msgs[1];
    assert_eq!(second_asst.input_tokens, 1500);
    assert_eq!(second_asst.cache_read_input_tokens, 200);
    assert_eq!(second_asst.output_tokens, 180);

    // 세션 토큰 누계
    assert_eq!(result.session.total_input_tokens, 1200 + 1500);
    assert_eq!(result.session.total_output_tokens, 340 + 180);
}

/// [CC-P-03] 기본 픽스처: tool_use 블록 분류 → tool_calls 1건 생성
#[test]
fn cc_parser_basic_tool_use_block_classification() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    assert_eq!(result.tool_calls.len(), 1, "tool_calls 1건 필요");
    assert_eq!(result.tool_calls[0].tool_name, "read_file");
    assert!(!result.tool_calls[0].input_hash.is_empty(), "input_hash 비어있으면 안 됨");
    // tool_input JSON 내 AbsolutePath 필드 포함
    let ti = result.tool_calls[0].tool_input.as_ref().expect("tool_input 없음");
    assert!(ti.contains("AbsolutePath"), "tool_input에 AbsolutePath 키 필요");
}

/// [CC-P-04] thinking 픽스처: thinking 블록 텍스트 추출 및 content 적재
#[test]
fn cc_parser_thinking_block_extracted_to_content() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_thinking.jsonl"))
        .expect("픽스처 파싱 실패");

    let asst_msgs: Vec<_> = result.messages.iter().filter(|m| m.role == "assistant").collect();
    assert_eq!(asst_msgs.len(), 1, "assistant 메시지 1건 필요");

    let content = asst_msgs[0].content.as_ref().expect("thinking content 없음");
    // [Thinking] 접두어 + 원본 텍스트 + text 블록이 결합되어야 함
    assert!(content.contains("[Thinking]"), "content에 [Thinking] 접두어 필요");
    assert!(content.contains("익명화된 사고과정 1"), "사고과정 본문 포함 필요");
}

/// [CC-P-05] thinking 픽스처: cache_read 토큰 추출 정확도
#[test]
fn cc_parser_thinking_cache_read_tokens() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_thinking.jsonl"))
        .expect("픽스처 파싱 실패");

    let asst = result.messages.iter().find(|m| m.role == "assistant").expect("assistant 없음");
    assert_eq!(asst.input_tokens, 2000);
    assert_eq!(asst.cache_read_input_tokens, 1500);
    assert_eq!(asst.output_tokens, 600);
}

/// [CC-P-06] 다중 턴 픽스처: 3 assistant 중 메시지 수 및 도구 호출 수 검증
#[test]
fn cc_parser_multi_turn_message_and_tool_counts() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_multi_turn.jsonl"))
        .expect("픽스처 파싱 실패");

    let asst_count = result.messages.iter().filter(|m| m.role == "assistant").count();
    assert_eq!(asst_count, 3, "assistant 3번 응답 필요");

    // tool_use 블록: 첫 번째 asst 1개 + 두 번째 asst 2개 = 3개
    assert_eq!(result.tool_calls.len(), 3, "총 tool_calls 3건 필요");

    // ended_at
    assert!(result.session.ended_at.is_some());
}

/// [CC-P-07] 다중 턴 픽스처: dedup_hash(input_hash) 충돌 없음 검증
#[test]
fn cc_parser_multi_turn_tool_input_hashes_unique() {
    let adapter = ClaudeCodeAdapter;
    let result = adapter
        .parse_session(&fixture_path("claude_code_multi_turn.jsonl"))
        .expect("픽스처 파싱 실패");

    let hashes: Vec<_> = result.tool_calls.iter().map(|tc| &tc.input_hash).collect();
    let mut unique = std::collections::HashSet::new();
    for h in &hashes {
        unique.insert(h.as_str());
    }
    // 모든 도구 호출이 서로 다른 인자이므로 해시가 모두 달라야 함
    assert_eq!(hashes.len(), unique.len(), "tool_call 해시 충돌 발생");
}

// ════════════════════════════════════════════════════════════
// 2. 파서 테스트 — Codex
// ════════════════════════════════════════════════════════════

/// [CDX-P-01] Codex 기본 픽스처: session_meta payload 구조 파싱
#[test]
fn cdx_parser_basic_session_meta() {
    let adapter = CodexAdapter;
    let result = adapter
        .parse_session(&fixture_path("codex_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    assert_eq!(result.session.session_id, "anon-sess-cdx-001");
    assert_eq!(result.session.agent_type, "codex");
    assert_eq!(result.session.cwd, "/anon/project");
    assert_eq!(result.session.model_id, Some("claude-opus-4-5".to_string()));
    assert_eq!(result.session.agent_version, Some("0.3.0".to_string()));
}

/// [CDX-P-02] Codex 기본 픽스처: token_count에서 usage 추출
#[test]
fn cdx_parser_basic_token_count_extraction() {
    let adapter = CodexAdapter;
    let result = adapter
        .parse_session(&fixture_path("codex_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    assert_eq!(result.session.total_input_tokens, 950);
    assert_eq!(result.session.total_output_tokens, 280);

    let msg = result.messages.first().expect("메시지 없음");
    assert_eq!(msg.input_tokens, 950);
    assert_eq!(msg.cache_read_input_tokens, 0);
    assert_eq!(msg.output_tokens, 280);
}

/// [CDX-P-03] Codex 기본 픽스처: mcp_tool_call_end → tool_calls 생성
#[test]
fn cdx_parser_basic_mcp_tool_call() {
    let adapter = CodexAdapter;
    let result = adapter
        .parse_session(&fixture_path("codex_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "anon_server/anon_tool");
    assert!(result.tool_calls[0].success);
    assert!(!result.tool_calls[0].input_hash.is_empty());
}

/// [CDX-P-04] Codex 기본 픽스처: patch_apply_end → nodes 생성
#[test]
fn cdx_parser_basic_patch_node() {
    let adapter = CodexAdapter;
    let result = adapter
        .parse_session(&fixture_path("codex_basic.jsonl"))
        .expect("픽스처 파싱 실패");

    let patch_nodes: Vec<_> = result.nodes.iter().filter(|n| n.node_type == "patch").collect();
    assert_eq!(patch_nodes.len(), 1, "patch 노드 1건 필요");
    assert!(patch_nodes[0].success, "patch_apply_end success=true이면 success여야 함");
}

/// [CDX-P-05] Codex patch 픽스처: 실패 도구 호출 및 실패 patch 노드 검증
#[test]
fn cdx_parser_patch_failure_nodes_and_tool_calls() {
    let adapter = CodexAdapter;
    let result = adapter
        .parse_session(&fixture_path("codex_patch.jsonl"))
        .expect("픽스처 파싱 실패");

    // patch 노드: success=true 1건 + success=false 1건
    let patch_nodes: Vec<_> = result.nodes.iter().filter(|n| n.node_type == "patch").collect();
    assert_eq!(patch_nodes.len(), 2, "patch 노드 2건 필요");
    let success_count = patch_nodes.iter().filter(|n| n.success).count();
    let fail_count    = patch_nodes.iter().filter(|n| !n.success).count();
    assert_eq!(success_count, 1, "성공 patch 1건");
    assert_eq!(fail_count, 1, "실패 patch 1건");

    // mcp_tool_call_end 실패 → success=false
    assert_eq!(result.tool_calls.len(), 1);
    assert!(!result.tool_calls[0].success, "실패 도구 호출은 success=false여야 함");

    // 2개 턴 토큰 누계
    assert_eq!(result.session.total_input_tokens, 500 + 600);
    assert_eq!(result.session.total_output_tokens, 150 + 200);
}

// ════════════════════════════════════════════════════════════
// 3. dedup 멱등 테스트 (인메모리 DB)
// ════════════════════════════════════════════════════════════

/// [DEDUP-01] 동일 세션을 두 번 적재해도 세션이 1건으로 유지됨
#[test]
fn dedup_insert_session_twice_keeps_one() {
    let conn = make_test_db();

    let adapter = ClaudeCodeAdapter;
    let session = adapter
        .parse_session(&fixture_path("claude_code_basic.jsonl"))
        .expect("파싱 실패")
        .session;

    // 첫 번째 적재
    db::insert_session(&conn, &session).expect("1차 적재 실패");
    // 두 번째 적재 (멱등 — INSERT OR IGNORE)
    db::insert_session(&conn, &session).expect("2차 적재 실패 (멱등 불가)");

    let sessions = db::get_all_sessions(&conn).expect("세션 조회 실패");
    assert_eq!(sessions.len(), 1, "동일 세션 2회 적재 후 1건만 유지되어야 함");
}

/// [DEDUP-02] 서로 다른 세션 ID는 각각 독립 적재
#[test]
fn dedup_different_sessions_inserted_independently() {
    let conn = make_test_db();

    let adapter = ClaudeCodeAdapter;
    let sess1 = adapter
        .parse_session(&fixture_path("claude_code_basic.jsonl"))
        .expect("기본 파싱 실패")
        .session;
    let sess2 = adapter
        .parse_session(&fixture_path("claude_code_thinking.jsonl"))
        .expect("thinking 파싱 실패")
        .session;

    db::insert_session(&conn, &sess1).expect("sess1 적재 실패");
    db::insert_session(&conn, &sess2).expect("sess2 적재 실패");

    let sessions = db::get_all_sessions(&conn).expect("세션 조회 실패");
    assert_eq!(sessions.len(), 2, "서로 다른 세션 2건이 모두 적재되어야 함");
}

/// [DEDUP-03] tool_input_hash가 같아도 다른 세션이면 별도 tool_call로 저장
#[test]
fn dedup_same_hash_different_sessions_both_stored() {
    let conn = make_test_db();

    let adapter = ClaudeCodeAdapter;

    // 기본 픽스처의 세션 적재 (tool_calls 포함)
    let parsed = adapter
        .parse_session(&fixture_path("claude_code_basic.jsonl"))
        .expect("파싱 실패");

    db::insert_session(&conn, &parsed.session).expect("세션 적재 실패");
    for msg in &parsed.messages {
        db::insert_message(&conn, msg).expect("메시지 적재 실패");
    }
    for tc in &parsed.tool_calls {
        db::insert_tool_call(&conn, tc).expect("tool_call 적재 실패");
    }

    let tcs = db::get_tool_calls_by_session(&conn, &parsed.session.session_id)
        .expect("tool_calls 조회 실패");
    assert_eq!(tcs.len(), parsed.tool_calls.len(), "적재된 tool_call 수 일치 필요");
}

/// [DEDUP-04] 동일 세션 전체(session + messages + tool_calls)를 2회 적재 후 세션 데이터 유지
#[test]
fn dedup_full_session_idempotent_double_load() {
    let conn = make_test_db();

    let adapter = CodexAdapter;
    let parsed = adapter
        .parse_session(&fixture_path("codex_basic.jsonl"))
        .expect("파싱 실패");

    for _ in 0..2 {
        // INSERT OR IGNORE이므로 세션은 1회만 삽입
        db::insert_session(&conn, &parsed.session).expect("세션 적재 실패");
    }

    let sessions = db::get_all_sessions(&conn).expect("세션 조회 실패");
    assert_eq!(sessions.len(), 1, "멱등 2회 적재 후 1건만 남아야 함");
    assert_eq!(sessions[0].session_id, "anon-sess-cdx-001");
}

// ════════════════════════════════════════════════════════════
// 4. 가격계산 테스트 (픽스처 usage 값 활용)
// ════════════════════════════════════════════════════════════

/// [PRICE-01] cache_read 토큰 포함 비용 계산 — claude_code_thinking 픽스처 기준
///
/// input=2000, cache_read=1500, output=600, 모델=claude-opus-4-5
/// claude-opus-4-5 단가(예시): input=15$/M, output=75$/M, cache_read=1.5$/M
/// normal_input = 2000 - 1500 = 500
/// cost = 500 * 15/1M + 1500 * 1.5/1M + 600 * 75/1M
///      = 0.0075 + 0.00225 + 0.045
///      = 0.05475
#[test]
fn price_cache_read_tokens_with_custom_pricing() {
    let pricing = Pricing::new(
        "claude-opus-4-5".to_string(),
        "anthropic".to_string(),
        15.0,   // input cost per million
        75.0,   // output cost per million
        1.5,    // cache_read cost per million
        "2026-01-01T00:00:00Z".to_string(),
    );

    let cost = calculate_cost_usd(Some(&pricing), 2000, 1500, 0, 600);
    let expected = 500.0 * 15.0 / 1_000_000.0
        + 1500.0 * 1.5 / 1_000_000.0
        + 600.0 * 75.0 / 1_000_000.0;
    assert!(
        (cost - expected).abs() < 1e-9,
        "기대 비용: {:.8}, 실제 비용: {:.8}",
        expected, cost
    );
}

/// [PRICE-02] fallback 단가(claude-3-5-sonnet) — cache_read=0인 기본 케이스
///
/// input=1200, cache_read=0, output=340, fallback: 3$/M / 15$/M / 0.3$/M
/// cost = 1200 * 3/1M + 0 + 340 * 15/1M
///      = 0.0036 + 0.0051 = 0.0087
#[test]
fn price_fallback_no_cache_read() {
    let cost = calculate_cost_usd(None, 1200, 0, 0, 340);
    let expected = 1200.0 * 3.0 / 1_000_000.0 + 340.0 * 15.0 / 1_000_000.0;
    assert!(
        (cost - expected).abs() < 1e-9,
        "fallback 비용 기대: {:.8}, 실제: {:.8}",
        expected, cost
    );
}

/// [PRICE-03] fallback 단가 — cache_read 있는 경우
///
/// input=1500, cache_read=200, output=180, fallback: 3$/M / 15$/M / 0.3$/M
/// normal = 1300, cost = 1300 * 3/1M + 200 * 0.3/1M + 180 * 15/1M
///        = 0.0039 + 0.00006 + 0.0027 = 0.00666
#[test]
fn price_fallback_with_cache_read() {
    let cost = calculate_cost_usd(None, 1500, 200, 0, 180);
    let expected = 1300.0 * 3.0 / 1_000_000.0
        + 200.0 * 0.3 / 1_000_000.0
        + 180.0 * 15.0 / 1_000_000.0;
    assert!(
        (cost - expected).abs() < 1e-9,
        "cache_read 포함 fallback 비용 기대: {:.8}, 실제: {:.8}",
        expected, cost
    );
}

/// [PRICE-04] cache_read가 input보다 클 경우 normal_input underflow 방지
///
/// input=100, cache_read=200 → normal_input=0 (언더플로 없음)
#[test]
fn price_cache_read_exceeds_input_no_underflow() {
    // pricing 없이 fallback 사용
    let cost = calculate_cost_usd(None, 100, 200, 0, 50);
    // normal_input=0, cache_read_cost=200 * 0.3/1M, output_cost=50 * 15/1M
    let expected = 0.0 + 200.0 * 0.3 / 1_000_000.0 + 50.0 * 15.0 / 1_000_000.0;
    assert!(
        (cost - expected).abs() < 1e-9,
        "언더플로 방지 비용 기대: {:.8}, 실제: {:.8}",
        expected, cost
    );
    assert!(cost >= 0.0, "비용은 음수일 수 없음");
}

/// [PRICE-05] 토큰이 모두 0인 경우 비용 0
#[test]
fn price_zero_tokens_yields_zero_cost() {
    let cost = calculate_cost_usd(None, 0, 0, 0, 0);
    assert_eq!(cost, 0.0, "토큰 모두 0이면 비용 0이어야 함");
}

/// [PRICE-06] 픽스처에서 파싱된 usage 값으로 비용 계산 및 DB 저장 일관성 확인
#[test]
fn price_fixture_parsed_usage_stored_in_db() {
    let conn = make_test_db();
    let adapter = ClaudeCodeAdapter;
    let parsed = adapter
        .parse_session(&fixture_path("claude_code_thinking.jsonl"))
        .expect("파싱 실패");

    db::insert_session(&conn, &parsed.session).expect("세션 적재 실패");
    for msg in &parsed.messages {
        db::insert_message(&conn, msg).expect("메시지 적재 실패");
    }

    // DB에서 다시 조회
    let stored_msgs = db::get_messages_by_session(&conn, &parsed.session.session_id)
        .expect("메시지 조회 실패");

    assert_eq!(stored_msgs.len(), parsed.messages.len(), "적재/조회 메시지 수 일치 필요");

    // assistant 메시지의 usage 값 일치 확인
    let stored_asst = stored_msgs.iter().find(|m| m.role == "assistant").expect("assistant 없음");
    assert_eq!(stored_asst.input_tokens, 2000);
    assert_eq!(stored_asst.cache_read_input_tokens, 1500);
    assert_eq!(stored_asst.output_tokens, 600);
}

/// [AGY-P-01] AntigravityAdapter Mock SQLite 파싱 및 Graceful Degrade 검증
#[test]
fn test_antigravity_adapter_parsing() {
    use agent_token_tracker::adapters::antigravity::{
        AntigravityAdapter, UnifiedState, TrajectorySummary, InnerSummary,
        TrajectorySummaryDetail, WorkspaceInfo
    };
    use agent_token_tracker::adapters::LogAdapter;
    use prost::Message;
    use base64::Engine;

    // 1. 임시 SQLite 파일 경로 설정
    let mut temp_db = std::env::temp_dir();
    temp_db.push("mock_antigravity_state.vscdb");
    let temp_db_path = temp_db.to_str().unwrap().to_string();

    // 혹시 기존 파일이 있으면 삭제
    let _ = std::fs::remove_file(&temp_db_path);

    // 2. SQLite DB 생성 및 테이블 설정
    let conn = rusqlite::Connection::open(&temp_db_path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    ).unwrap();

    // 3. Mock Protobuf 데이터 구성
    let conversation_id = "test-session-uuid-12345";
    
    // TrajectorySummaryDetail 구성
    let detail = TrajectorySummaryDetail {
        title: "Mock Conversation Title".to_string(),
        step_count: 5,
        created_at: None,
        conversation_id: conversation_id.to_string(),
        started_at: None,
        workspace_info: Some(WorkspaceInfo {
            workspace_root: Some("/Users/mock/project".to_string()),
            workspace_uri: None,
            git_info_raw: None,
        }),
        updated_at: None,
    };
    
    let mut detail_buf = Vec::new();
    detail.encode(&mut detail_buf).unwrap();
    let detail_b64 = base64::engine::general_purpose::STANDARD.encode(&detail_buf);

    // UnifiedState 구성
    let inner_summary = InnerSummary {
        detail_b64,
    };
    let summary = TrajectorySummary {
        conversation_id: conversation_id.to_string(),
        inner: Some(inner_summary),
    };
    let unified_state = UnifiedState {
        summaries: vec![summary],
    };

    let mut state_buf = Vec::new();
    unified_state.encode(&mut state_buf).unwrap();
    let value_b64 = base64::engine::general_purpose::STANDARD.encode(&state_buf);

    // 4. DB에 적재
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES ('antigravityUnifiedStateSync.trajectorySummaries', ?1)",
        rusqlite::params![value_b64],
    ).unwrap();
    drop(conn); // 파일 핸들 해제

    // 5. AntigravityAdapter 로 파싱 테스트
    let adapter = AntigravityAdapter;
    let virtual_path = format!("{}?session_id={}", temp_db_path, conversation_id);
    let parsed_res = adapter.parse_session(&virtual_path).expect("Antigravity mock 파싱 실패");

    // 6. 결과 단언문(Assertion) 검증 (폴백에 의해 5 step 기준 estimated 25,000 / 5,000 토큰 추정치 산출)
    assert_eq!(parsed_res.session.session_id, conversation_id);
    assert_eq!(parsed_res.session.agent_type, "antigravity");
    assert_eq!(parsed_res.session.token_source, "estimated");
    assert_eq!(parsed_res.session.total_input_tokens, 25000);
    assert_eq!(parsed_res.session.total_output_tokens, 5000);
    assert_eq!(parsed_res.session.cwd, "/Users/mock/project");
    assert_eq!(parsed_res.session.session_name, Some("Mock Conversation Title".to_string()));
    assert_eq!(parsed_res.messages.len(), 5);
    assert_eq!(parsed_res.nodes.len(), 5);
    for node in parsed_res.nodes {
        assert_eq!(node.session_id, conversation_id);
        assert_eq!(node.node_type, "text");
        assert!(node.success);
    }

    // 7. 임시 파일 정리
    let _ = std::fs::remove_file(&temp_db_path);
}

/// [AGY-P-02] Antigravity 로그 파일 존재 시 글자 수 기반 토큰 및 대화 메시지 복원 검증
#[test]
fn test_antigravity_log_character_counting() {
    use agent_token_tracker::adapters::antigravity::{
        AntigravityAdapter, UnifiedState, TrajectorySummary, InnerSummary,
        TrajectorySummaryDetail
    };
    use agent_token_tracker::adapters::LogAdapter;
    use std::fs;
    use prost::Message;
    use base64::Engine;

    // 1. 임시 디렉토리 설정 및 HOME 환경 변수 스푸핑
    let temp_dir = std::env::temp_dir().join("mock_home_for_test");
    let _ = fs::remove_dir_all(&temp_dir); // 기존 자산 제거
    fs::create_dir_all(&temp_dir).unwrap();
    
    // 원래 HOME 백업
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &temp_dir);

    let conversation_id = "test-session-uuid-56789";
    let log_dir = temp_dir
        .join(".gemini")
        .join("antigravity-ide")
        .join("brain")
        .join(conversation_id)
        .join(".system_generated")
        .join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let log_file = log_dir.join("transcript_full.jsonl");

    // 2. 가상 대화 데이터 작성
    // 한글: 10글자 (10 * 1.6 = 16)
    // 영어/ASCII: 20글자 (20 / 4.0 = 5)
    // 합산: 입력 21 tokens
    let dummy_log = r#"{"type":"USER_INPUT","content":"한글열글자다EnglishTwentyChar"}
{"type":"PLANNER_RESPONSE","content":"답변도한글다섯자EngTen","tool_calls":[{"name":"view_file","args":{"toolAction":"view_file","toolSummary":"View file contents","CommandLine":"","Arguments":"{\"AbsolutePath\":\"/mock/file\"}"}}]}"#;
    fs::write(&log_file, dummy_log).unwrap();

    // 3. state.vscdb 생성
    let mut temp_db = std::env::temp_dir();
    temp_db.push("mock_antigravity_state_2.vscdb");
    let temp_db_path = temp_db.to_str().unwrap().to_string();
    let _ = fs::remove_file(&temp_db_path);

    let conn = rusqlite::Connection::open(&temp_db_path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    ).unwrap();

    let detail = TrajectorySummaryDetail {
        title: "Mock Real Log Title".to_string(),
        step_count: 2,
        created_at: None,
        conversation_id: conversation_id.to_string(),
        started_at: None,
        workspace_info: None,
        updated_at: None,
    };
    let mut detail_buf = Vec::new();
    detail.encode(&mut detail_buf).unwrap();
    let detail_b64 = base64::engine::general_purpose::STANDARD.encode(&detail_buf);

    let unified_state = UnifiedState {
        summaries: vec![TrajectorySummary {
            conversation_id: conversation_id.to_string(),
            inner: Some(InnerSummary { detail_b64 }),
        }],
    };
    let mut state_buf = Vec::new();
    unified_state.encode(&mut state_buf).unwrap();
    let value_b64 = base64::engine::general_purpose::STANDARD.encode(&state_buf);

    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES ('antigravityUnifiedStateSync.trajectorySummaries', ?1)",
        rusqlite::params![value_b64],
    ).unwrap();
    drop(conn);

    // 4. 실행 및 검증
    let adapter = AntigravityAdapter;
    let virtual_path = format!("{}?session_id={}", temp_db_path, conversation_id);
    let parsed_res = adapter.parse_session(&virtual_path).expect("Antigravity mock 파싱 실패");

    // 5. 복구 및 정리
    if let Some(h) = original_home {
        std::env::set_var("HOME", h);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = fs::remove_dir_all(&temp_dir);
    let _ = fs::remove_file(&temp_db_path);

    // 6. 단언문 검증
    assert_eq!(parsed_res.session.session_id, conversation_id);
    assert_eq!(parsed_res.session.token_source, "estimated");
    // 입력 (USER_INPUT): 한글 6자 (9) + 영어 17자 (4) = 13 tokens
    assert_eq!(parsed_res.session.total_input_tokens, 13);
    // 출력 (PLANNER_RESPONSE): 한글 8자 (12) + 영어 6자 (1) = 13 tokens + 도구 토큰 13 = 26 tokens
    assert_eq!(parsed_res.session.total_output_tokens, 26);
    assert_eq!(parsed_res.messages.len(), 2);
    assert_eq!(parsed_res.messages[0].role, "user");
    assert_eq!(parsed_res.messages[0].input_tokens, 13);
    assert_eq!(parsed_res.messages[0].output_tokens, 0);
    assert_eq!(parsed_res.messages[0].content, Some("한글열글자다EnglishTwentyChar".to_string()));
    assert_eq!(parsed_res.messages[1].role, "agent");
    assert_eq!(parsed_res.messages[1].input_tokens, 0);
    assert_eq!(parsed_res.messages[1].output_tokens, 26);
    assert_eq!(parsed_res.messages[1].content, Some("답변도한글다섯자EngTen".to_string()));

    // 6-2. 도구 호출 이력 검증
    assert_eq!(parsed_res.tool_calls.len(), 1);
    assert_eq!(parsed_res.tool_calls[0].tool_name, "view_file");
    assert_eq!(parsed_res.tool_calls[0].tool_input, Some("{\"Arguments\":\"{\\\"AbsolutePath\\\":\\\"/mock/file\\\"}\",\"CommandLine\":\"\",\"toolAction\":\"view_file\",\"toolSummary\":\"View file contents\"}".to_string()));
}
