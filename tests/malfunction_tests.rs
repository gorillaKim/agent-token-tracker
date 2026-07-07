#[cfg(test)]
mod tests {
    use agent_token_tracker::db::{
        init_db, insert_session, insert_message, insert_tool_call,
        get_session_malfunctions, insert_malfunction_pattern
    };
    use agent_token_tracker::model::{
        Session, Message, ToolCall, MalfunctionRule
    };
    use agent_token_tracker::detect::malfunctions::analyze_and_detect_malfunctions;

    fn setup_temp_db(db_path: &str) -> rusqlite::Connection {
        init_db(db_path).expect("테스트 DB 초기화 실패")
    }

    fn cleanup_temp_db(_db_path: &str) {
        // 인메모리 DB는 명시적 정리가 필요하지 않음
    }

    #[test]
    fn test_malfunction_detection_flow() {
        let db_path = ":memory:";
        let conn = setup_temp_db(db_path);

        // 1. 테스트용 세션 정보 생성 및 적재
        let session_id = "test-sess-123";
        let session = Session::new(
            session_id.to_string(),
            "claude_code".to_string(),
            Some("1.0".to_string()),
            "2026-07-07T09:00:00Z".to_string(),
            None, // unexpected exit (ended_at is None)
            "/workspace".to_string(),
            Some("claude-3-5-sonnet".to_string()),
            15000, // input tokens (TokenInefficiency 용)
            50,    // output tokens (매우 비효율적)
            0,
            "api".to_string(),
            None,
            None,
        );
        insert_session(&conn, &session).unwrap();

        // 2. 메시지 생성 및 적재 (답변 지연 시뮬레이션: 65초 차이)
        let msg_user = Message::new(
            session_id.to_string(),
            1,
            "user".to_string(),
            1000,
            0,
            0,
            0,
            0.0,
            "2026-07-07T09:00:00Z".to_string(),
            Some("안녕".to_string()),
        );
        let msg_agent = Message::new(
            session_id.to_string(),
            1,
            "agent".to_string(),
            0,
            0,
            0,
            100,
            0.001,
            "2026-07-07T09:01:05Z".to_string(), // 65초 지연
            Some("안녕하세요".to_string()),
        );
        insert_message(&conn, &msg_user).unwrap();
        insert_message(&conn, &msg_agent).unwrap();

        // 3. 도구 호출 생성 및 적재 (핑퐁 루프 시뮬레이션: tool_x -> tool_y -> tool_x -> tool_y)
        let tc1 = ToolCall::new(
            session_id.to_string(),
            "tool_x".to_string(),
            Some("input1".to_string()),
            "h1".to_string(),
            true,
            false,
            false,
            None,
            None,
            "2026-07-07T09:00:10Z".to_string(),
        );
        let tc2 = ToolCall::new(
            session_id.to_string(),
            "tool_y".to_string(),
            Some("input2".to_string()),
            "h2".to_string(),
            true,
            false,
            false,
            None,
            None,
            "2026-07-07T09:00:15Z".to_string(),
        );
        let tc3 = ToolCall::new(
            session_id.to_string(),
            "tool_x".to_string(),
            Some("input3".to_string()),
            "h3".to_string(),
            true,
            false,
            false,
            None,
            None,
            "2026-07-07T09:00:20Z".to_string(),
        );
        let tc4 = ToolCall::new(
            session_id.to_string(),
            "tool_y".to_string(),
            Some("input4".to_string()),
            "h4".to_string(),
            true,
            false,
            false,
            None,
            None,
            "2026-07-07T09:00:25Z".to_string(),
        );
        insert_tool_call(&conn, &tc1).unwrap();
        insert_tool_call(&conn, &tc2).unwrap();
        insert_tool_call(&conn, &tc3).unwrap();
        insert_tool_call(&conn, &tc4).unwrap();

        // 4. 패턴 등록
        // 룰 A: 예상치 못한 종료이면서 60초 이상 지연된 경우 (AND 조건)
        let rule_and = MalfunctionRule::And {
            conditions: vec![
                MalfunctionRule::UnexpectedExit { value: true },
                MalfunctionRule::MaxResponseDelaySec { value: 60 },
            ]
        };
        let rules_json_and = serde_json::to_string(&rule_and).unwrap();
        let pat_id_and = insert_malfunction_pattern(
            &conn,
            "비정상 종료 및 지연 패턴",
            Some("예상치 못한 종료와 동시에 60초 이상 응답이 지연된 패턴"),
            &rules_json_and
        ).unwrap();

        // 룰 B: 2회 이상 핑퐁이 발생한 경우 (동적 핑퐁)
        let rule_pingpong = MalfunctionRule::DynamicPingPong { cycles_threshold: 2 };
        let rules_json_pingpong = serde_json::to_string(&rule_pingpong).unwrap();
        let pat_id_pingpong = insert_malfunction_pattern(
            &conn,
            "핑퐁 감지 패턴",
            Some("동적 핑퐁이 2회 이상 검출된 패턴"),
            &rules_json_pingpong
        ).unwrap();

        // 5. 오작동 분석 매칭 가동
        let detected = analyze_and_detect_malfunctions(&conn, session_id).unwrap();

        // 6. 결과 검증
        assert_eq!(detected.len(), 2, "2개의 오작동 패턴이 모두 검출되어야 합니다.");
        
        let detected_ids: Vec<i64> = detected.iter().map(|(id, _)| *id).collect();
        assert!(detected_ids.contains(&pat_id_and));
        assert!(detected_ids.contains(&pat_id_pingpong));

        // DB에 감지 내역이 잘 적재되었는지 검증
        let detections = get_session_malfunctions(&conn, session_id).unwrap();
        assert_eq!(detections.len(), 2);
        
        cleanup_temp_db(db_path);
    }

    #[test]
    fn test_malfunction_dedup_and_idempotence() {
        let db_path = ":memory:";
        let conn = setup_temp_db(db_path);

        let session_id = "test-sess-dedup";
        let session = Session::new(
            session_id.to_string(),
            "claude_code".to_string(),
            Some("1.0".to_string()),
            "2026-07-07T09:00:00Z".to_string(),
            None,
            "/workspace".to_string(),
            Some("claude-3-5-sonnet".to_string()),
            100,
            100,
            0,
            "api".to_string(),
            None,
            None,
        );
        insert_session(&conn, &session).unwrap();

        let pat_id = insert_malfunction_pattern(
            &conn,
            "테스트 패턴",
            None,
            "{\"type\":\"unexpected_exit\",\"value\":true}"
        ).unwrap();

        // 중복 감지 이력 등록 시도 (insert_malfunction_detection 대신 DB에 직접 쓰거나 이관된 기능 호출)
        let _id1 = agent_token_tracker::db::insert_malfunction_detection(&conn, session_id, pat_id, "근거 1").unwrap();
        let _id2 = agent_token_tracker::db::insert_malfunction_detection(&conn, session_id, pat_id, "근거 2").unwrap();

        // UNIQUE 제약 조건(INSERT OR IGNORE)에 의해 두 번째 입력은 무시되어야 하며,
        // 결과적으로 get_session_malfunctions 반환 건수는 1건이어야 한다.
        let detections = get_session_malfunctions(&conn, session_id).unwrap();
        assert_eq!(detections.len(), 1, "중복 감지는 무시되어 단 1건만 존재해야 합니다.");
        assert_eq!(detections[0].evidence, "근거 1", "최초로 인입된 근거 1이 보존되어야 합니다.");
    }

    #[test]
    fn test_validate_malfunction_pattern_fp() {
        let db_path = ":memory:";
        let conn = setup_temp_db(db_path);

        // 테스트를 위해 세션 하나 등록 (UnexpectedExit 가 true가 되도록 ended_at은 None)
        let session_id = "test-sess-fp";
        let session = Session::new(
            session_id.to_string(),
            "claude_code".to_string(),
            Some("1.0".to_string()),
            "2026-07-07T09:00:00Z".to_string(),
            None,
            "/workspace".to_string(),
            Some("claude-3-5-sonnet".to_string()),
            100,
            100,
            0,
            "api".to_string(),
            None,
            None,
        );
        insert_session(&conn, &session).unwrap();

        // 1. 유효하지 않은 JSON 검증
        let (valid1, msg1, _, _, _) = agent_token_tracker::detect::malfunctions::validate_malfunction_pattern(
            &conn,
            "{invalid_json}",
            30
        ).unwrap();
        assert!(!valid1);
        assert!(msg1.contains("유효하지 않은"));

        // 2. 유효하지만 FP 의심되는 룰 (UnexpectedExit: true 룰은 100% 매칭됨)
        let rules_json = "{\"type\":\"unexpected_exit\",\"value\":true}";
        let (valid2, _msg2, ratio, is_fp, samples) = agent_token_tracker::detect::malfunctions::validate_malfunction_pattern(
            &conn,
            rules_json,
            30
        ).unwrap();
        assert!(valid2);
        assert!(is_fp, "모든 세션이 매칭되므로 FP로 분류되어야 합니다.");
        assert_eq!(ratio, 1.0);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].0, session_id);
    }

    #[test]
    fn test_resolve_session_id_prefix() {
        let db_path = ":memory:";
        let conn = setup_temp_db(db_path);

        let session_id_1 = "session-abcdef-123456";
        let session_id_2 = "session-abcdef-789012";

        let session1 = Session::new(
            session_id_1.to_string(),
            "claude_code".to_string(),
            Some("1.0".to_string()),
            "2026-07-07T09:00:00Z".to_string(),
            None,
            "/workspace".to_string(),
            Some("claude-3-5-sonnet".to_string()),
            100, 100, 0, "api".to_string(), None, None
        );
        let session2 = Session::new(
            session_id_2.to_string(),
            "claude_code".to_string(),
            Some("1.0".to_string()),
            "2026-07-07T09:00:10Z".to_string(), // started_at이 더 최신
            None,
            "/workspace".to_string(),
            Some("claude-3-5-sonnet".to_string()),
            100, 100, 0, "api".to_string(), None, None
        );
        insert_session(&conn, &session1).unwrap();
        insert_session(&conn, &session2).unwrap();

        // 1. Exact match 검증
        let resolved = agent_token_tracker::db::resolve_session_id(&conn, session_id_1).unwrap();
        assert_eq!(resolved, agent_token_tracker::db::ResolvedSession::Single(session_id_1.to_string()));

        // 2. 8자 미만 prefix (None이어야 함)
        let resolved = agent_token_tracker::db::resolve_session_id(&conn, "session").unwrap();
        assert_eq!(resolved, agent_token_tracker::db::ResolvedSession::None);

        // 3. 고유한 단일 prefix (8자 이상)
        let resolved = agent_token_tracker::db::resolve_session_id(&conn, "session-abcdef-7").unwrap();
        assert_eq!(resolved, agent_token_tracker::db::ResolvedSession::Single(session_id_2.to_string()));

        // 4. 다중 매칭 prefix (Multiple 반환되어야 함)
        let resolved = agent_token_tracker::db::resolve_session_id(&conn, "session-abcdef").unwrap();
        match resolved {
            agent_token_tracker::db::ResolvedSession::Multiple(matches) => {
                assert_eq!(matches.len(), 2);
                assert_eq!(matches[0], session_id_2.to_string()); // ORDER BY started_at DESC 에 의해 최신 세션이 0순위
                assert_eq!(matches[1], session_id_1.to_string());
            }
            _ => panic!("Multiple을 반환해야 합니다."),
        }
    }

    #[test]
    fn test_claude_code_adapter_normalization_and_errors() {
        let raw_log = r#"
{"type":"session_start","session_id":"test-claude-sess","agent_type":"claude_code","started_at":"2026-07-07T09:00:00Z","cwd":"/workspace"}
{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"Thinking..."},{"type":"tool_use","id":"call_1","name":"read_file","input":{"path":"src/lib.rs"}}],"usage":{"input_tokens":50,"output_tokens":100}},"timestamp":"2026-07-07T09:00:05Z"}
{"type":"message","message":{"role":"tool","content":[{"type":"tool_result","tool_use_id":"call_1","content":"error: file not found","is_error":true}],"usage":{"input_tokens":10,"output_tokens":20}},"timestamp":"2026-07-07T09:00:10Z"}
{"type":"session_end","timestamp":"2026-07-07T09:00:15Z"}
"#;
        // claude_code 어댑터를 가동해 파싱을 진행
        let parsed = agent_token_tracker::adapters::claude_code::ClaudeCodeAdapter::parse_raw_logs(
            "test-claude-sess",
            raw_log.as_bytes()
        ).unwrap();
        
        // 1. role이 assistant -> agent 로 정규화되었는지 검증
        assert_eq!(parsed.messages.len(), 2);
        assert_eq!(parsed.messages[0].role, "agent");
        
        // 2. tool call success가 is_error=true 에 의해 false로 설정되었는지 검증
        assert_eq!(parsed.tool_calls.len(), 1);
        assert!(!parsed.tool_calls[0].success, "is_error가 true이므로 success=false 여야 합니다.");
    }

    #[test]
    fn test_dismiss_and_false_positive_flow() {
        let db_path = ":memory:";
        let conn = setup_temp_db(db_path);

        // 1. 오작동 패턴 등록
        let pattern_id = insert_malfunction_pattern(
            &conn,
            "test_pattern",
            Some("Description"),
            "{\"type\":\"RepeatedCalls\",\"max_calls\":3,\"tool_name\":null}",
        ).unwrap();

        // 2. 세션 및 감지 이력 등록
        let sess = Session::new(
            "sess-for-dismiss".to_string(),
            "claude_code".to_string(),
            None,
            "/workspace".to_string(),
            None,
            "2026-07-07T12:00:00Z".to_string(),
            None,
            0,
            0,
            0,
            "db".to_string(),
            None,
            None,
        );
        insert_session(&conn, &sess).unwrap();

        agent_token_tracker::db::insert_malfunction_detection(
            &conn,
            "sess-for-dismiss",
            pattern_id,
            "evidence content",
        ).unwrap();

        // 3. 초기 상태 검증 (is_false_positive == false)
        let list_all = agent_token_tracker::db::get_malfunction_detections(&conn, None, None, None, None, None, None).unwrap();
        assert_eq!(list_all.len(), 1);
        assert!(!list_all[0].is_false_positive);
        assert!(!agent_token_tracker::db::is_session_malfunction_dismissed(&conn, "sess-for-dismiss").unwrap());

        // 4. False Positive 마킹 해제
        agent_token_tracker::db::dismiss_session_malfunctions(&conn, "sess-for-dismiss", true).unwrap();

        // 5. 마킹 해제 상태 검증
        assert!(agent_token_tracker::db::is_session_malfunction_dismissed(&conn, "sess-for-dismiss").unwrap());
        let list_fp_only = agent_token_tracker::db::get_malfunction_detections(&conn, None, None, None, Some(true), None, None).unwrap();
        assert_eq!(list_fp_only.len(), 1);
        assert!(list_fp_only[0].is_false_positive);

        let list_active_only = agent_token_tracker::db::get_malfunction_detections(&conn, None, None, None, Some(false), None, None).unwrap();
        assert_eq!(list_active_only.len(), 0);

        // 6. 복원
        agent_token_tracker::db::dismiss_session_malfunctions(&conn, "sess-for-dismiss", false).unwrap();
        assert!(!agent_token_tracker::db::is_session_malfunction_dismissed(&conn, "sess-for-dismiss").unwrap());
    }
}

