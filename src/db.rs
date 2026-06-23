//! SQLite 데이터베이스 연동 및 DDL 마이그레이션 관리 모듈
//!
//! 에이전트 분석 데이터를 안정적으로 적재하기 위해 5대 테이블과 2대 인덱스를
//! 멱등하게(재실행 안전하게) 생성하는 마이그레이션을 구현합니다.

use rusqlite::Connection;

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
