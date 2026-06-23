//! SQLite 데이터베이스 연동 및 DDL 마이그레이션 관리 모듈
//!
//! 에이전트 분석 데이터를 안정적으로 적재하기 위해 5대 테이블과 2대 인덱스를
//! 멱등하게(재실행 안전하게) 생성하는 마이그레이션을 구현합니다.

use rusqlite::{Connection, params};
use crate::model::{Session, Message, Node, ToolCall, Pricing, SessionReport, AgentReport, ToolReport};

/// 데이터베이스 커넥션을 초기화하고 필요한 스키마 테이블 및 인덱스를 생성합니다.
pub fn init_db(db_path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(db_path)?;

    // SQLite 외래 키 제약 조건 활성화
    conn.pragma_update(None, "foreign_keys", "ON")?;

    // WAL(Write-Ahead Logging) 모드 활성화 및 busy_timeout 설정 (graceful degrade 정책 준수)
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", &3000)?; // 3초 대기

    // 1. sessions 테이블 생성
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            agent_type TEXT NOT NULL,
            agent_version TEXT,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            cwd TEXT NOT NULL,
            model_id TEXT,
            total_input_tokens INTEGER NOT NULL DEFAULT 0,
            total_output_tokens INTEGER NOT NULL DEFAULT 0,
            token_source TEXT NOT NULL DEFAULT 'unavailable'
        );",
        [],
    )?;

    // 멱등적으로 컬럼 추가 (ALTER TABLE 에러 무시)
    conn.execute("ALTER TABLE sessions ADD COLUMN session_name TEXT;", []).ok();
    conn.execute("ALTER TABLE sessions ADD COLUMN parent_session_id TEXT;", []).ok();

    // 2. messages 테이블 생성
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
            turn_index INTEGER NOT NULL,
            role TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0.0,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    // 3. nodes 테이블 생성
    conn.execute(
        "CREATE TABLE IF NOT EXISTS nodes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
            node_type TEXT NOT NULL,
            success INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    // 4. tool_calls 테이블 생성
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tool_calls (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
            tool_name TEXT NOT NULL,
            tool_input TEXT,
            input_hash TEXT NOT NULL,
            success INTEGER NOT NULL DEFAULT 1,
            is_loop_suspect INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    // 5. pricing 테이블 생성
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pricing (
            model_id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            input_cost_per_million REAL NOT NULL,
            output_cost_per_million REAL NOT NULL,
            cached_input_cost_per_million REAL NOT NULL,
            updated_at TEXT NOT NULL
        );",
        [],
    )?;

    // 6. 인덱스 생성
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tool_loop ON tool_calls(session_id, tool_name, input_hash);",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_msg_session ON messages(session_id, turn_index);",
        [],
    )?;

    // 7. pricing 테이블 기본 단가 시딩 (멱등)
    conn.execute(
        "INSERT OR IGNORE INTO pricing (model_id, provider, input_cost_per_million, output_cost_per_million, cached_input_cost_per_million, updated_at)
         VALUES 
         ('claude-3-5-sonnet', 'anthropic', 3.0, 15.0, 0.3, datetime('now')),
         ('claude-3-opus', 'anthropic', 15.0, 75.0, 1.5, datetime('now')),
         ('claude-3-haiku', 'anthropic', 0.25, 1.25, 0.03, datetime('now')),
         ('gpt-4o', 'openai', 5.0, 15.0, 2.5, datetime('now'));",
        [],
    )?;

    Ok(conn)
}

/// 세션 정보를 데이터베이스에 적재합니다. (중복 시 무시하여 멱등성 보장)
pub fn insert_session(conn: &Connection, session: &Session) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO sessions (
            session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id,
            total_input_tokens, total_output_tokens, token_source, session_name, parent_session_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            session.session_id,
            session.agent_type,
            session.agent_version,
            session.started_at,
            session.ended_at,
            session.cwd,
            session.model_id,
            session.total_input_tokens,
            session.total_output_tokens,
            session.token_source,
            session.session_name,
            session.parent_session_id
        ],
    )?;
    Ok(())
}

/// 메시지 정보를 데이터베이스에 적재합니다.
pub fn insert_message(conn: &Connection, msg: &Message) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO messages (
            session_id, turn_index, role, input_tokens, cache_read_input_tokens,
            output_tokens, cost_usd, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            msg.session_id,
            msg.turn_index,
            msg.role,
            msg.input_tokens,
            msg.cache_read_input_tokens,
            msg.output_tokens,
            msg.cost_usd,
            msg.created_at
        ],
    )?;
    Ok(())
}

/// 노드 정보를 데이터베이스에 적재합니다.
pub fn insert_node(conn: &Connection, node: &Node) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO nodes (session_id, node_type, success, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            node.session_id,
            node.node_type,
            if node.success { 1 } else { 0 },
            node.created_at
        ],
    )?;
    Ok(())
}

/// 도구 호출 정보를 데이터베이스에 적재합니다.
pub fn insert_tool_call(conn: &Connection, tc: &ToolCall) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            tc.session_id,
            tc.tool_name,
            tc.tool_input,
            tc.input_hash,
            if tc.success { 1 } else { 0 },
            if tc.is_loop_suspect { 1 } else { 0 },
            tc.created_at
        ],
    )?;
    Ok(())
}

/// 특정 세션 정보를 조회합니다.
pub fn get_session(conn: &Connection, session_id: &str) -> Result<Option<Session>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id,
                total_input_tokens, total_output_tokens, token_source, session_name, parent_session_id
         FROM sessions WHERE session_id = ?1",
    )?;

    let mut rows = stmt.query(params![session_id])?;

    if let Some(row) = rows.next()? {
        Ok(Some(Session::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
        )))
    } else {
        Ok(None)
    }
}

/// 적재된 모든 세션 정보를 조회합니다.
pub fn get_all_sessions(conn: &Connection) -> Result<Vec<Session>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id,
                total_input_tokens, total_output_tokens, token_source, session_name, parent_session_id
         FROM sessions",
    )?;

    let sess_iter = stmt.query_map([], |row| {
        Ok(Session::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
        ))
    })?;

    let mut sessions = Vec::new();
    for sess in sess_iter {
        sessions.push(sess?);
    }
    Ok(sessions)
}

/// 특정 세션의 메시지 리스트를 턴 인덱스 오름차순으로 조회합니다.
pub fn get_messages_by_session(conn: &Connection, session_id: &str) -> Result<Vec<Message>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, turn_index, role, input_tokens, cache_read_input_tokens,
                output_tokens, cost_usd, created_at
         FROM messages WHERE session_id = ?1 ORDER BY turn_index ASC",
    )?;

    let msg_iter = stmt.query_map(params![session_id], |row| {
        let mut msg = Message::new(
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
        );
        msg.id = Some(row.get(0)?);
        Ok(msg)
    })?;

    let mut messages = Vec::new();
    for msg in msg_iter {
        messages.push(msg?);
    }
    Ok(messages)
}

/// 특정 세션의 노드 리스트를 ID 순으로 조회합니다.
pub fn get_nodes_by_session(conn: &Connection, session_id: &str) -> Result<Vec<Node>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, node_type, success, created_at
         FROM nodes WHERE session_id = ?1 ORDER BY id ASC",
    )?;

    let node_iter = stmt.query_map(params![session_id], |row| {
        let success_val: i32 = row.get(3)?;
        let mut node = Node::new(
            row.get(1)?,
            row.get(2)?,
            success_val == 1,
            row.get(4)?,
        );
        node.id = Some(row.get(0)?);
        Ok(node)
    })?;

    let mut nodes = Vec::new();
    for node in node_iter {
        nodes.push(node?);
    }
    Ok(nodes)
}

/// 특정 세션의 도구 호출 기록을 ID 순으로 조회합니다.
pub fn get_tool_calls_by_session(conn: &Connection, session_id: &str) -> Result<Vec<ToolCall>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at
         FROM tool_calls WHERE session_id = ?1 ORDER BY id ASC",
    )?;

    let tc_iter = stmt.query_map(params![session_id], |row| {
        let success_val: i32 = row.get(5)?;
        let loop_suspect_val: i32 = row.get(6)?;
        let mut tc = ToolCall::new(
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            success_val == 1,
            loop_suspect_val == 1,
            row.get(7)?,
        );
        tc.id = Some(row.get(0)?);
        Ok(tc)
    })?;

    let mut tool_calls = Vec::new();
    for tc in tc_iter {
        tool_calls.push(tc?);
    }
    Ok(tool_calls)
}

/// 데이터베이스의 모든 모델별 단가 정보를 조회해 HashMap 형태로 반환합니다.
pub fn get_all_pricings(conn: &Connection) -> Result<std::collections::HashMap<String, Pricing>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT model_id, provider, input_cost_per_million, output_cost_per_million, cached_input_cost_per_million, updated_at
         FROM pricing",
    )?;

    let pricing_iter = stmt.query_map([], |row| {
        Ok(Pricing::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    })?;

    let mut pricings = std::collections::HashMap::new();
    for pricing in pricing_iter {
        let p = pricing?;
        pricings.insert(p.model_id.clone(), p);
    }
    Ok(pricings)
}

/// 세션별 토큰 및 비용 정보를 조회하여 집계 리포트 목록을 반환합니다.
pub fn get_session_report(
    conn: &Connection,
    session_id: Option<&str>,
    since: Option<&str>,
    sort: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<SessionReport>, rusqlite::Error> {
    let mut query = "
        SELECT s.session_id, s.agent_type, s.model_id, s.total_input_tokens, s.total_output_tokens,
               COALESCE(mc.session_cost, 0.0) as total_cost, s.started_at
        FROM sessions s
        LEFT JOIN (
            SELECT session_id, SUM(cost_usd) as session_cost
            FROM messages
            GROUP BY session_id
        ) mc ON s.session_id = mc.session_id
        WHERE 1=1
    ".to_string();

    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(sid) = session_id {
        query.push_str(" AND s.session_id = ? ");
        params.push(rusqlite::types::Value::Text(sid.to_string()));
    }

    if let Some(date_str) = since {
        query.push_str(" AND s.started_at >= ? ");
        params.push(rusqlite::types::Value::Text(date_str.to_string()));
    }

    query.push_str(" GROUP BY s.session_id ");

    match sort {
        Some("cost") => query.push_str(" ORDER BY total_cost DESC "),
        Some("tokens") => query.push_str(" ORDER BY (s.total_input_tokens + s.total_output_tokens) DESC "),
        _ => query.push_str(" ORDER BY s.started_at DESC "),
    }

    if let Some(l) = limit {
        query.push_str(" LIMIT ? ");
        params.push(rusqlite::types::Value::Integer(l as i64));
    }

    let mut stmt = conn.prepare(&query)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let report_iter = stmt.query_map(&params_ref[..], |row| {
        Ok(SessionReport::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
        ))
    })?;

    let mut list = Vec::new();
    for item in report_iter {
        list.push(item?);
    }
    Ok(list)
}

/// 에이전트별 세션 수, 토큰 수 및 비용 정보를 조회하여 집계 리포트 목록을 반환합니다.
pub fn get_agent_report(
    conn: &Connection,
    since: Option<&str>,
    sort: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<AgentReport>, rusqlite::Error> {
    let mut query = "
        SELECT s.agent_type, COUNT(DISTINCT s.session_id) as session_count,
               SUM(s.total_input_tokens) as total_input, SUM(s.total_output_tokens) as total_output,
               COALESCE(SUM(mc.session_cost), 0.0) as total_cost
        FROM sessions s
        LEFT JOIN (
            SELECT session_id, SUM(cost_usd) as session_cost
            FROM messages
            GROUP BY session_id
        ) mc ON s.session_id = mc.session_id
        WHERE 1=1
    ".to_string();

    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(date_str) = since {
        query.push_str(" AND s.started_at >= ? ");
        params.push(rusqlite::types::Value::Text(date_str.to_string()));
    }

    query.push_str(" GROUP BY s.agent_type ");

    match sort {
        Some("cost") => query.push_str(" ORDER BY total_cost DESC "),
        Some("tokens") => query.push_str(" ORDER BY (total_input + total_output) DESC "),
        _ => query.push_str(" ORDER BY total_cost DESC "),
    }

    if let Some(l) = limit {
        query.push_str(" LIMIT ? ");
        params.push(rusqlite::types::Value::Integer(l as i64));
    }

    let mut stmt = conn.prepare(&query)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let report_iter = stmt.query_map(&params_ref[..], |row| {
        Ok(AgentReport::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
        ))
    })?;

    let mut list = Vec::new();
    for item in report_iter {
        list.push(item?);
    }
    Ok(list)
}

/// 도구 호출 횟수, 성공 여부 및 루프 의심 통계를 조회하여 집계 리포트 목록을 반환합니다.
pub fn get_tool_report(
    conn: &Connection,
    since: Option<&str>,
    sort: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<ToolReport>, rusqlite::Error> {
    let mut query = "
        SELECT tc.tool_name, COUNT(*) as call_count, SUM(tc.success) as success_count, SUM(tc.is_loop_suspect) as loop_suspect_count
        FROM tool_calls tc
        LEFT JOIN sessions s ON tc.session_id = s.session_id
        WHERE 1=1
    ".to_string();

    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(date_str) = since {
        query.push_str(" AND s.started_at >= ? ");
        params.push(rusqlite::types::Value::Text(date_str.to_string()));
    }

    query.push_str(" GROUP BY tc.tool_name ");

    match sort {
        Some("count") => query.push_str(" ORDER BY call_count DESC "),
        Some("loop") => query.push_str(" ORDER BY loop_suspect_count DESC "),
        _ => query.push_str(" ORDER BY call_count DESC "),
    }

    if let Some(l) = limit {
        query.push_str(" LIMIT ? ");
        params.push(rusqlite::types::Value::Integer(l as i64));
    }

    let mut stmt = conn.prepare(&query)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let report_iter = stmt.query_map(&params_ref[..], |row| {
        let call_count_val: i64 = row.get(1)?;
        let success_count_val: i64 = row.get(2)?;
        let loop_suspect_count_val: i64 = row.get(3)?;
        Ok(ToolReport::new(
            row.get(0)?,
            call_count_val as u64,
            success_count_val as u64,
            loop_suspect_count_val as u64,
        ))
    })?;

    let mut list = Vec::new();
    for item in report_iter {
        list.push(item?);
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_init_and_crud() {
        // 메모리 내 임시 SQLite DB 사용
        let conn = init_db(":memory:").expect("DB 초기화 실패");

        // 1. Session 데이터 테스트
        let sess = Session::new(
            "sess-uuid-1234".to_string(),
            "codex".to_string(),
            Some("1.0.0".to_string()),
            "2026-06-23T09:00:00Z".to_string(),
            None,
            "/Users/madup/project".to_string(),
            Some("gpt-4o".to_string()),
            1500,
            800,
            "api".to_string(),
            Some("테스트 세션".to_string()),
            Some("parent-uuid-5678".to_string()),
        );

        // INSERT 검증
        insert_session(&conn, &sess).expect("Session 삽입 실패");
        
        // 중복 INSERT 시 멱등성(Ignore) 검증
        insert_session(&conn, &sess).expect("Session 중복 삽입 실패");

        // SELECT 검증
        let fetched_sess = get_session(&conn, "sess-uuid-1234")
            .expect("Session 조회 실패")
            .expect("Session 존재하지 않음");
        assert_eq!(sess, fetched_sess);

        // SELECT ALL 검증
        let all_sessions = get_all_sessions(&conn).expect("전체 Session 조회 실패");
        assert_eq!(all_sessions.len(), 1);
        assert_eq!(all_sessions[0], sess);

        // 2. Message 데이터 테스트
        let msg = Message::new(
            "sess-uuid-1234".to_string(),
            1,
            "user".to_string(),
            100,
            20,
            50,
            0.003,
            "2026-06-23T09:01:00Z".to_string(),
        );
        insert_message(&conn, &msg).expect("Message 삽입 실패");
        
        let fetched_msgs = get_messages_by_session(&conn, "sess-uuid-1234").expect("Message 리스트 조회 실패");
        assert_eq!(fetched_msgs.len(), 1);
        assert_eq!(fetched_msgs[0].turn_index, 1);
        assert_eq!(fetched_msgs[0].role, "user");

        // 3. Node 데이터 테스트
        let node = Node::new(
            "sess-uuid-1234".to_string(),
            "patch".to_string(),
            true,
            "2026-06-23T09:02:00Z".to_string(),
        );
        insert_node(&conn, &node).expect("Node 삽입 실패");

        let fetched_nodes = get_nodes_by_session(&conn, "sess-uuid-1234").expect("Node 리스트 조회 실패");
        assert_eq!(fetched_nodes.len(), 1);
        assert_eq!(fetched_nodes[0].node_type, "patch");
        assert!(fetched_nodes[0].success);

        // 4. ToolCall 데이터 테스트
        let tc = ToolCall::new(
            "sess-uuid-1234".to_string(),
            "view_file".to_string(),
            Some("{\"AbsolutePath\":\"/path/to/file\"}".to_string()),
            "abc123hash".to_string(),
            true,
            false,
            "2026-06-23T09:03:00Z".to_string(),
        );
        insert_tool_call(&conn, &tc).expect("ToolCall 삽입 실패");

        let fetched_tcs = get_tool_calls_by_session(&conn, "sess-uuid-1234").expect("ToolCall 리스트 조회 실패");
        assert_eq!(fetched_tcs.len(), 1);
        assert_eq!(fetched_tcs[0].tool_name, "view_file");
        assert!(fetched_tcs[0].success);

        // 5. Pricing 데이터 테스트 (기본 시드 포함)
        let pricings = get_all_pricings(&conn).expect("Pricing 조회 실패");
        assert!(pricings.contains_key("claude-3-5-sonnet"));
        assert!(pricings.contains_key("gpt-4o"));
        
        let sonnet = pricings.get("claude-3-5-sonnet").unwrap();
        assert_eq!(sonnet.provider, "anthropic");
        assert_eq!(sonnet.input_cost_per_million, 3.0);
        assert_eq!(sonnet.output_cost_per_million, 15.0);
        assert_eq!(sonnet.cached_input_cost_per_million, 0.3);

        // 6. Report 롤업 집계 테스트
        let sess_report = get_session_report(&conn, None, None, None, None).expect("SessionReport 조회 실패");
        assert_eq!(sess_report.len(), 1);
        assert_eq!(sess_report[0].session_id, "sess-uuid-1234");
        assert_eq!(sess_report[0].total_input_tokens, 1500);
        assert_eq!(sess_report[0].total_output_tokens, 800);
        assert_eq!(sess_report[0].total_cost_usd, 0.003);

        let agent_report = get_agent_report(&conn, None, None, None).expect("AgentReport 조회 실패");
        assert_eq!(agent_report.len(), 1);
        assert_eq!(agent_report[0].agent_type, "codex");
        assert_eq!(agent_report[0].session_count, 1);
        assert_eq!(agent_report[0].total_cost_usd, 0.003);

        let tool_report = get_tool_report(&conn, None, None, None).expect("ToolReport 조회 실패");
        assert_eq!(tool_report.len(), 1);
        assert_eq!(tool_report[0].tool_name, "view_file");
        assert_eq!(tool_report[0].call_count, 1);
        assert_eq!(tool_report[0].success_count, 1);

        // 7. Session 삭제 및 CASCADE 연쇄 삭제 검증
        delete_session(&conn, "sess-uuid-1234").expect("Session 삭제 실패");
        let deleted_sess = get_session(&conn, "sess-uuid-1234").expect("Session 조회 실패");
        assert!(deleted_sess.is_none());

        let deleted_msgs = get_messages_by_session(&conn, "sess-uuid-1234").expect("Message 조회 실패");
        assert!(deleted_msgs.is_empty());

        let deleted_nodes = get_nodes_by_session(&conn, "sess-uuid-1234").expect("Node 조회 실패");
        assert!(deleted_nodes.is_empty());

        let deleted_tcs = get_tool_calls_by_session(&conn, "sess-uuid-1234").expect("ToolCall 조회 실패");
        assert!(deleted_tcs.is_empty());
    }
}

/// 특정 세션 ID에 해당하는 세션 정보를 삭제합니다.
/// 외래 키 제약 조건(ON DELETE CASCADE)이 활성화되어 있으므로
/// 관련 메시지, 노드, 도구 호출도 데이터베이스에서 연쇄 삭제됩니다.
pub fn delete_session(conn: &Connection, session_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM sessions WHERE session_id = ?1", params![session_id])?;
    Ok(())
}

