//! SQLite 데이터베이스 연동 및 DDL 마이그레이션 관리 모듈
//!
//! 에이전트 분석 데이터를 안정적으로 적재하기 위해 5대 테이블과 2대 인덱스를
//! 멱등하게(재실행 안전하게) 생성하는 마이그레이션을 구현합니다.

use rusqlite::{Connection, params};
use crate::model::{Session, Message, Node, ToolCall};

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

    Ok(conn)
}

/// 세션 정보를 데이터베이스에 적재합니다. (중복 시 무시하여 멱등성 보장)
pub fn insert_session(conn: &Connection, session: &Session) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO sessions (
            session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id,
            total_input_tokens, total_output_tokens, token_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
            session.token_source
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
                total_input_tokens, total_output_tokens, token_source
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
        )))
    } else {
        Ok(None)
    }
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
    }
}
