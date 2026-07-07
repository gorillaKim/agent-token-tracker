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
}
