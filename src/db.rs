//! SQLite 데이터베이스 연동 및 DDL 마이그레이션 관리 모듈
//!
//! 에이전트 분석 데이터를 안정적으로 적재하기 위해 5대 테이블과 2대 인덱스를
//! 멱등하게(재실행 안전하게) 생성하는 마이그레이션을 구현합니다.

use rusqlite::{Connection, params};
use crate::model::{Session, Message, Node, ToolCall, Pricing, SessionReport, AgentReport, ToolReport, McpServerReport, McpToolDetailReport};

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
            created_at TEXT NOT NULL,
            content TEXT
        );",
        [],
    )?;

    // 멱등적으로 컬럼 추가 (ALTER TABLE 에러 무시)
    conn.execute("ALTER TABLE messages ADD COLUMN content TEXT;", []).ok();

    // SQLite FTS5 확장 기능 선택적 활성화
    #[cfg(feature = "fts")]
    {
        // FTS5 가상 테이블 생성 (messages 테이블을 외부 콘텐츠로 하는 가상 테이블)
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS msg_fts USING fts5(
                session_id UNINDEXED,
                role,
                content,
                content='messages',
                content_rowid='id'
            );",
            [],
        )?;

        // FTS5 동기화 트리거 생성
        // INSERT 트리거
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS trg_msg_insert AFTER INSERT ON messages BEGIN
                INSERT INTO msg_fts(rowid, session_id, role, content)
                VALUES (new.id, new.session_id, new.role, new.content);
            END;",
            [],
        )?;

        // DELETE 트리거
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS trg_msg_delete AFTER DELETE ON messages BEGIN
                INSERT INTO msg_fts(msg_fts, rowid, session_id, role, content)
                VALUES ('delete', old.id, old.session_id, old.role, old.content);
            END;",
            [],
        )?;

        // UPDATE 트리거
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS trg_msg_update AFTER UPDATE ON messages BEGIN
                INSERT INTO msg_fts(msg_fts, rowid, session_id, role, content)
                VALUES ('delete', old.id, old.session_id, old.role, old.content);
                INSERT INTO msg_fts(rowid, session_id, role, content)
                VALUES (new.id, new.session_id, new.role, new.content);
            END;",
            [],
        )?;
    }

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
            is_mcp INTEGER NOT NULL DEFAULT 0,
            mcp_server TEXT,
            mcp_tool TEXT,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    // 멱등적으로 컬럼 추가 (ALTER TABLE 에러 무시)
    conn.execute("ALTER TABLE tool_calls ADD COLUMN is_mcp INTEGER NOT NULL DEFAULT 0;", []).ok();
    conn.execute("ALTER TABLE tool_calls ADD COLUMN mcp_server TEXT;", []).ok();
    conn.execute("ALTER TABLE tool_calls ADD COLUMN mcp_tool TEXT;", []).ok();

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
            output_tokens, cost_usd, created_at, content
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            msg.session_id,
            msg.turn_index,
            msg.role,
            msg.input_tokens,
            msg.cache_read_input_tokens,
            msg.output_tokens,
            msg.cost_usd,
            msg.created_at,
            msg.content
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
        "INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, is_mcp, mcp_server, mcp_tool, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            tc.session_id,
            tc.tool_name,
            tc.tool_input,
            tc.input_hash,
            if tc.success { 1 } else { 0 },
            if tc.is_loop_suspect { 1 } else { 0 },
            if tc.is_mcp { 1 } else { 0 },
            tc.mcp_server,
            tc.mcp_tool,
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

/// started_at 기준 최근 N일 이내(롤링 window)의 세션만 조회합니다.
/// SQLite datetime()으로 양쪽을 정규화해 'T'/space 구분자 차이에 안전합니다.
pub fn get_sessions_within_days(conn: &Connection, days: u32) -> Result<Vec<Session>, rusqlite::Error> {
    let days = days.max(1);
    let sql = format!(
        "SELECT session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id,
                total_input_tokens, total_output_tokens, token_source, session_name, parent_session_id
         FROM sessions
         WHERE datetime(started_at) >= datetime('now', '-{} days')
         ORDER BY datetime(started_at) DESC",
        days
    );
    let mut stmt = conn.prepare(&sql)?;

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

/// started_at 이 "오늘"(사용자 로컬 일자)인 세션만 조회합니다.
///
/// `tz_modifier` 는 `local_tz_sql_modifier()` 가 만드는 SQLite date() 수정자
/// (예: "+540 minutes") 입니다. DB 는 UTC 저장이므로 date(started_at, tz) 로 로컬 일자 버킷팅 후
/// date('now', tz)(로컬 오늘)와 비교합니다. 트레이 헬스 체크처럼 "오늘" 범위만 필요한 곳에서 사용.
pub fn get_sessions_today(conn: &Connection, tz_modifier: &str) -> Result<Vec<Session>, rusqlite::Error> {
    let sql = format!(
        "SELECT session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id,
                total_input_tokens, total_output_tokens, token_source, session_name, parent_session_id
         FROM sessions
         WHERE date(started_at, '{tz}') = date('now', '{tz}')",
        tz = tz_modifier
    );
    let mut stmt = conn.prepare(&sql)?;

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
                output_tokens, cost_usd, created_at, content
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
            row.get(9)?,
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
        "SELECT id, session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, is_mcp, mcp_server, mcp_tool, created_at
         FROM tool_calls WHERE session_id = ?1 ORDER BY id ASC",
    )?;

    let tc_iter = stmt.query_map(params![session_id], |row| {
        let success_val: i32 = row.get(5)?;
        let loop_suspect_val: i32 = row.get(6)?;
        let mcp_val: i32 = row.get(7)?;
        let mut tc = ToolCall::new(
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            success_val == 1,
            loop_suspect_val == 1,
            mcp_val == 1,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
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

/// MCP 서버별 사용량 집계 리포트를 조회합니다.
///
/// 토큰 수치는 세션 기여도 방식으로 집계됩니다.
/// 즉, 해당 MCP 서버를 1회 이상 호출한 세션들의 총 토큰 합계이며,
/// 중복 집계 방지를 위해 DISTINCT 세션 기준으로 먼저 집계한 후 조인합니다.
pub fn get_mcp_server_report(
    conn: &Connection,
    since: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<McpServerReport>, rusqlite::Error> {
    let mut query = "
        SELECT
            tc.mcp_server,
            COUNT(*) AS call_count,
            SUM(tc.success) AS success_count,
            SUM(tc.is_loop_suspect) AS loop_suspect_count,
            COUNT(DISTINCT tc.session_id) AS distinct_sessions,
            COALESCE(SUM(sess_agg.total_input_tokens), 0) AS session_total_input_tokens,
            COALESCE(SUM(sess_agg.total_output_tokens), 0) AS session_total_output_tokens,
            COALESCE(SUM(sess_agg.session_cost), 0.0) AS session_total_cost_usd
        FROM tool_calls tc
        JOIN (
            SELECT DISTINCT s.session_id, s.total_input_tokens, s.total_output_tokens,
                   COALESCE(mc.session_cost, 0.0) AS session_cost
            FROM sessions s
            LEFT JOIN (
                SELECT session_id, SUM(cost_usd) AS session_cost
                FROM messages
                GROUP BY session_id
            ) mc ON s.session_id = mc.session_id
        ) sess_agg ON tc.session_id = sess_agg.session_id
        WHERE tc.is_mcp = 1
          AND tc.mcp_server IS NOT NULL
    ".to_string();

    let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(date_str) = since {
        // 세션 started_at 기준 필터 (tool_call created_at 대신 세션 시작일 기준)
        query.push_str(" AND EXISTS (
            SELECT 1 FROM sessions s2
            WHERE s2.session_id = tc.session_id
              AND s2.started_at >= ?
        ) ");
        params_vec.push(rusqlite::types::Value::Text(date_str.to_string()));
    }

    query.push_str(" GROUP BY tc.mcp_server ORDER BY call_count DESC ");

    if let Some(l) = limit {
        query.push_str(" LIMIT ? ");
        params_vec.push(rusqlite::types::Value::Integer(l as i64));
    }

    let mut stmt = conn.prepare(&query)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let iter = stmt.query_map(&params_ref[..], |row| {
        let call_count: i64 = row.get(1)?;
        let success_count: i64 = row.get(2)?;
        let loop_count: i64 = row.get(3)?;
        let distinct: i64 = row.get(4)?;
        let input_tokens: i64 = row.get(5)?;
        let output_tokens: i64 = row.get(6)?;
        Ok(McpServerReport::new(
            row.get(0)?,
            call_count as u64,
            success_count as u64,
            loop_count as u64,
            distinct as u64,
            input_tokens as u64,
            output_tokens as u64,
            row.get(7)?,
        ))
    })?;

    let mut list = Vec::new();
    for item in iter {
        list.push(item?);
    }
    Ok(list)
}

/// 특정 MCP 서버 내 도구별 상세 사용량 리포트를 조회합니다.
///
/// `mcp_server` 파라미터는 필수이며, 해당 서버 내 각 mcp_tool의
/// 호출 횟수, 성공률, 관련 세션 토큰을 반환합니다.
pub fn get_mcp_tool_report_by_server(
    conn: &Connection,
    mcp_server: &str,
    since: Option<&str>,
    sort: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<McpToolDetailReport>, rusqlite::Error> {
    let mut query = "
        SELECT
            tc.mcp_server,
            tc.mcp_tool,
            COUNT(*) AS call_count,
            SUM(tc.success) AS success_count,
            SUM(tc.is_loop_suspect) AS loop_suspect_count,
            COUNT(DISTINCT tc.session_id) AS distinct_sessions,
            COALESCE(SUM(sess_agg.total_input_tokens), 0) AS session_total_input_tokens,
            COALESCE(SUM(sess_agg.total_output_tokens), 0) AS session_total_output_tokens,
            COALESCE(SUM(sess_agg.session_cost), 0.0) AS session_total_cost_usd
        FROM tool_calls tc
        JOIN (
            SELECT DISTINCT s.session_id, s.total_input_tokens, s.total_output_tokens,
                   COALESCE(mc.session_cost, 0.0) AS session_cost
            FROM sessions s
            LEFT JOIN (
                SELECT session_id, SUM(cost_usd) AS session_cost
                FROM messages
                GROUP BY session_id
            ) mc ON s.session_id = mc.session_id
        ) sess_agg ON tc.session_id = sess_agg.session_id
        WHERE tc.is_mcp = 1
          AND tc.mcp_server = ?
          AND tc.mcp_tool IS NOT NULL
    ".to_string();

    let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();
    params_vec.push(rusqlite::types::Value::Text(mcp_server.to_string()));

    if let Some(date_str) = since {
        query.push_str(" AND EXISTS (
            SELECT 1 FROM sessions s2
            WHERE s2.session_id = tc.session_id
              AND s2.started_at >= ?
        ) ");
        params_vec.push(rusqlite::types::Value::Text(date_str.to_string()));
    }

    query.push_str(" GROUP BY tc.mcp_server, tc.mcp_tool ");

    match sort {
        Some("count") => query.push_str(" ORDER BY call_count DESC "),
        Some("tokens") => query.push_str(" ORDER BY session_total_input_tokens DESC "),
        Some("cost") => query.push_str(" ORDER BY session_total_cost_usd DESC "),
        _ => query.push_str(" ORDER BY call_count DESC "),
    }

    if let Some(l) = limit {
        query.push_str(" LIMIT ? ");
        params_vec.push(rusqlite::types::Value::Integer(l as i64));
    }

    let mut stmt = conn.prepare(&query)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let iter = stmt.query_map(&params_ref[..], |row| {
        let call_count: i64 = row.get(2)?;
        let success_count: i64 = row.get(3)?;
        let loop_count: i64 = row.get(4)?;
        let distinct: i64 = row.get(5)?;
        let input_tokens: i64 = row.get(6)?;
        let output_tokens: i64 = row.get(7)?;
        Ok(McpToolDetailReport::new(
            row.get(0)?,
            row.get(1)?,
            call_count as u64,
            success_count as u64,
            loop_count as u64,
            distinct as u64,
            input_tokens as u64,
            output_tokens as u64,
            row.get(8)?,
        ))
    })?;

    let mut list = Vec::new();
    for item in iter {
        list.push(item?);
    }
    Ok(list)
}

/// 데이터베이스에 기록된 MCP 서버(플러그인) 이름 목록을 반환합니다.
/// 에이전트가 어떤 MCP 서버들이 추적되고 있는지 먼저 확인할 때 사용합니다.
pub fn get_mcp_server_list(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT mcp_server
         FROM tool_calls
         WHERE is_mcp = 1 AND mcp_server IS NOT NULL
         ORDER BY mcp_server ASC",
    )?;

    let iter = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut list = Vec::new();
    for item in iter {
        list.push(item?);
    }
    Ok(list)
}

/// 특정 MCP 서버 내에서 사용된 도구(mcp_tool) 이름 목록을 반환합니다.
pub fn get_mcp_tool_list(conn: &Connection, mcp_server: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT mcp_tool
         FROM tool_calls
         WHERE is_mcp = 1
           AND mcp_server = ?1
           AND mcp_tool IS NOT NULL
         ORDER BY mcp_tool ASC",
    )?;

    let iter = stmt.query_map(params![mcp_server], |row| row.get::<_, String>(0))?;
    let mut list = Vec::new();
    for item in iter {
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
            "/mock/project".to_string(),
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
            Some("안녕하세요".to_string()),
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
            false, // is_mcp
            None,  // mcp_server
            None,  // mcp_tool
            "2026-06-23T09:03:00Z".to_string(),
        );
        insert_tool_call(&conn, &tc).expect("ToolCall 삽입 실패");

        let fetched_tcs = get_tool_calls_by_session(&conn, "sess-uuid-1234").expect("ToolCall 리스트 조회 실패");
        assert_eq!(fetched_tcs.len(), 1);
        assert_eq!(fetched_tcs[0].tool_name, "view_file");
        assert!(fetched_tcs[0].success);
        assert!(!fetched_tcs[0].is_mcp);

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

        // 8. FTS5 검색 및 트리거 동기화 테스트
        #[cfg(feature = "fts")]
        {
            let test_sess = Session::new(
                "sess-fts-test".to_string(),
                "claude_code".to_string(),
                None,
                "2026-06-23T12:00:00Z".to_string(),
                None,
                "/mock/fts".to_string(),
                Some("claude-3-5-sonnet".to_string()),
                100,
                50,
                "api".to_string(),
                None,
                None,
            );
            insert_session(&conn, &test_sess).expect("FTS 테스트 세션 삽입 실패");

            let test_msg1 = Message::new(
                "sess-fts-test".to_string(),
                1,
                "user".to_string(),
                10,
                0,
                0,
                0.0,
                "2026-06-23T12:01:00Z".to_string(),
                Some("중요한 대화 내용입니다. FTS5 검색을 테스트합니다.".to_string()),
            );
            let test_msg2 = Message::new(
                "sess-fts-test".to_string(),
                2,
                "assistant".to_string(),
                0,
                0,
                20,
                0.0003,
                "2026-06-23T12:01:10Z".to_string(),
                Some("어시스턴트의 답변으로 view_file 호출이 완료되었습니다.".to_string()),
            );
            insert_message(&conn, &test_msg1).expect("FTS 테스트 메시지1 삽입 실패");
            insert_message(&conn, &test_msg2).expect("FTS 테스트 메시지2 삽입 실패");

            // FTS5 MATCH 검색 수행
            let search_res = search_messages(&conn, "FTS5").expect("FTS 검색 실패");
            assert_eq!(search_res.len(), 1);
            assert_eq!(search_res[0].session_id, "sess-fts-test");
            assert_eq!(search_res[0].content, "중요한 대화 내용입니다. FTS5 검색을 테스트합니다.");

            let search_res_2 = search_messages(&conn, "view_file").expect("FTS 검색 실패");
            assert_eq!(search_res_2.len(), 1);
            assert_eq!(search_res_2[0].content, "어시스턴트의 답변으로 view_file 호출이 완료되었습니다.");

            // UPDATE에 따른 트리거 동기화 검증
            conn.execute(
                "UPDATE messages SET content = '수정된 메시지 본문입니다.' WHERE id = ?1",
                params![search_res_2[0].id],
            ).expect("메시지 업데이트 실패");

            let search_res_updated = search_messages(&conn, "수정된").expect("FTS 검색 실패");
            assert_eq!(search_res_updated.len(), 1);
            assert_eq!(search_res_updated[0].content, "수정된 메시지 본문입니다.");

            // DELETE에 따른 트리거 동기화 검증
            conn.execute(
                "DELETE FROM messages WHERE id = ?1",
                params![search_res_updated[0].id],
            ).expect("메시지 삭제 실패");

            let search_res_deleted = search_messages(&conn, "수정된").expect("FTS 검색 실패");
            assert!(search_res_deleted.is_empty());
        }
    }
}

/// 특정 세션 ID에 해당하는 세션 정보를 삭제합니다.
/// 외래 키 제약 조건(ON DELETE CASCADE)이 활성화되어 있으므로
/// 관련 메시지, 노드, 도구 호출도 데이터베이스에서 연쇄 삭제됩니다.
pub fn delete_session(conn: &Connection, session_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM sessions WHERE session_id = ?1", params![session_id])?;
    Ok(())
}

#[cfg(feature = "fts")]
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub turn_index: u64,
    pub started_at: String,
    pub model_id: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub cost_usd: f64,
}

#[cfg(feature = "fts")]
pub fn search_messages(conn: &Connection, query: &str) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.session_id, m.role, COALESCE(m.content, ''), m.turn_index,
                s.started_at, s.model_id, s.total_input_tokens, s.total_output_tokens,
                COALESCE((SELECT SUM(cost_usd) FROM messages WHERE session_id = s.session_id), 0.0) as total_cost
         FROM msg_fts f
         JOIN messages m ON f.rowid = m.id
         JOIN sessions s ON m.session_id = s.session_id
         WHERE msg_fts MATCH ?1
         ORDER BY s.started_at DESC, m.turn_index ASC"
    )?;

    let rows = stmt.query_map(params![query], |row| {
        Ok(SearchResult {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            turn_index: row.get(4)?,
            started_at: row.get(5)?,
            model_id: row.get(6)?,
            total_input_tokens: row.get(7)?,
            total_output_tokens: row.get(8)?,
            cost_usd: row.get(9)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

