//! SQLite 데이터베이스 연동 및 마이그레이션 관리 모듈

use rusqlite::Connection;

/// 데이터베이스 커넥션을 초기화하고 필요한 테이블을 생성합니다.
pub fn init_db(db_path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    
    // TODO: 테이블 생성 및 마이그레이션 로직 추가 예정
    
    Ok(conn)
}
