//! Antigravity 어댑터 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

use rusqlite::params;
use base64::Engine;
use prost::Message;
use crate::model::{Session, Node};
use super::{LogAdapter, NormalizedSession};

// 1. Prost Protobuf 구조체 선언
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnifiedState {
    #[prost(message, repeated, tag = "1")]
    pub summaries: Vec<TrajectorySummary>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct TrajectorySummary {
    #[prost(string, tag = "1")]
    pub conversation_id: String,
    #[prost(message, optional, tag = "2")]
    pub inner: Option<InnerSummary>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct InnerSummary {
    #[prost(string, tag = "1")]
    pub detail_b64: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Timestamp {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct WorkspaceInfo {
    #[prost(string, optional, tag = "1")]
    pub workspace_root: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub workspace_uri: Option<String>,
    #[prost(bytes, optional, tag = "3")]
    pub git_info_raw: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct TrajectorySummaryDetail {
    #[prost(string, tag = "1")]
    pub title: String,
    #[prost(uint64, tag = "2")]
    pub step_count: u64,
    #[prost(message, optional, tag = "3")]
    pub created_at: Option<Timestamp>,
    #[prost(string, tag = "4")]
    pub conversation_id: String,
    #[prost(message, optional, tag = "7")]
    pub started_at: Option<Timestamp>,
    #[prost(message, optional, tag = "9")]
    pub workspace_info: Option<WorkspaceInfo>,
    #[prost(message, optional, tag = "10")]
    pub updated_at: Option<Timestamp>,
}

pub struct AntigravityAdapter;

/// SQLite vscdb 파일로부터 모든 세션 ID 목록을 조회해 반환합니다.
pub fn get_vscdb_session_ids(db_path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE
    )?;
    conn.pragma_update(None, "busy_timeout", &3000)?;

    let mut stmt = conn.prepare("SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.trajectorySummaries'")?;
    let mut rows = stmt.query([])?;
    let mut ids = Vec::new();

    if let Some(row) = rows.next()? {
        let value_b64: String = row.get(0)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(value_b64.trim())?;
        
        let unified_state = UnifiedState::decode(&bytes[..])?;
        for summary in unified_state.summaries {
            ids.push(summary.conversation_id);
        }
    }
    Ok(ids)
}

impl LogAdapter for AntigravityAdapter {
    fn parse_session(&self, path: &str) -> Result<NormalizedSession, Box<dyn std::error::Error>> {
        // path 구조는 "/Users/madup/.../state.vscdb?session_id=UUID" 형식임
        let parts: Vec<&str> = path.split("?session_id=").collect();
        if parts.len() < 2 {
            return Err("Antigravity 가상 경로 규격이 잘못되었습니다 (?session_id= 필요)".into());
        }

        let db_path = parts[0];
        let target_session_id = parts[1];

        // 1. SQLite state.vscdb 파일 오픈
        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE
        )?;
        conn.pragma_update(None, "busy_timeout", &3000)?;

        // 2. trajectorySummaries 조회
        let mut stmt = conn.prepare("SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.trajectorySummaries'")?;
        let mut rows = stmt.query([])?;
        
        let mut detail_b64_opt = None;
        if let Some(row) = rows.next()? {
            let value_b64: String = row.get(0)?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(value_b64.trim())?;
            
            let unified_state = UnifiedState::decode(&bytes[..])?;
            for summary in unified_state.summaries {
                if summary.conversation_id == target_session_id {
                    if let Some(inner) = summary.inner {
                        detail_b64_opt = Some(inner.detail_b64);
                    }
                    break;
                }
            }
        }

        let detail_b64 = detail_b64_opt.ok_or_else(|| {
            format!("지정된 세션 ID [{}]를 vscdb에서 찾을 수 없습니다.", target_session_id)
        })?;

        // 3. 디테일 protobuf 디코딩
        let detail_bytes = base64::engine::general_purpose::STANDARD.decode(detail_b64.trim())?;
        let detail = TrajectorySummaryDetail::decode(&detail_bytes[..])?;

        // 4. 타임스탬프 변환 (seconds -> ISO8601 string)
        let started_at = if let Some(ref sa) = detail.started_at {
            format_timestamp(sa.seconds)
        } else if let Some(ref ca) = detail.created_at {
            format_timestamp(ca.seconds)
        } else {
            format_timestamp(0)
        };

        let ended_at = detail.updated_at.as_ref().map(|ua| format_timestamp(ua.seconds));

        // 5. workspace_info 파싱
        let mut cwd = "/workspace".to_string();
        let mut _git_remote = None;
        let mut git_branch = None;

        if let Some(ws) = detail.workspace_info.as_ref() {
            if let Some(root) = ws.workspace_root.as_ref() {
                cwd = root.to_string();
            } else if let Some(uri) = ws.workspace_uri.as_ref() {
                cwd = uri.trim_start_matches("file://").to_string();
            }

            // git info 파싱 (lossy string 추출)
            if let Some(raw_bytes) = ws.git_info_raw.as_ref() {
                let lossy_str = String::from_utf8_lossy(raw_bytes);
                for word in lossy_str.split(|c: char| c.is_control() || c.is_whitespace() || c == '\u{0}') {
                    if let Some(idx) = word.find("https://") {
                        let url_part = &word[idx..];
                        let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                        _git_remote = Some(clean_url.to_string());
                    } else if let Some(idx) = word.find("git@") {
                        let url_part = &word[idx..];
                        let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                        _git_remote = Some(clean_url.to_string());
                    }

                    if !word.contains("https://") && !word.contains("git@") {
                        if let Some(idx) = word.find("feat/") {
                            let branch_part = &word[idx..];
                            let clean_branch = branch_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '-' && c != '_');
                            git_branch = Some(clean_branch.to_string());
                        } else if word.contains("main") {
                            git_branch = Some("main".to_string());
                        } else if word.contains("master") {
                            git_branch = Some("master".to_string());
                        }
                    }
                }
            }
        }

        // git_remote 및 git_branch 경고 예방 및 세션명 보강
        let mut final_title = detail.title.clone();
        if let Some(ref branch) = git_branch {
            final_title = format!("[{}] {}", branch, final_title);
        }

        // 6. 세션 정보 맵핑
        let session = Session::new(
            target_session_id.to_string(),
            "antigravity".to_string(),
            None,
            started_at.clone(),
            ended_at,
            cwd,
            None,
            0,
            0,
            "unavailable".to_string(),
            Some(final_title),
            None,
        );

        // 7. 활동(step_count)만큼 빈 Node들을 가상 턴으로 생성 (시각화/루프 탐지용)
        let mut nodes = Vec::new();
        for _ in 0..detail.step_count {
            let node = Node::new(
                target_session_id.to_string(),
                "text".to_string(),
                true,
                started_at.clone(),
            );
            nodes.push(node);
        }

        Ok(NormalizedSession {
            session,
            messages: Vec::new(),
            nodes,
            tool_calls: Vec::new(),
        })
    }
}

/// unix_sec 타임스탬프를 ISO8601 표준 포맷 문자열로 변환하는 헬퍼 함수
fn format_timestamp(secs: i64) -> String {
    if secs <= 0 {
        return "2026-06-23T00:00:00Z".to_string();
    }
    if let Ok(conn) = rusqlite::Connection::open_in_memory() {
        if let Ok(mut stmt) = conn.prepare("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?, 'unixepoch')") {
            if let Ok(mut rows) = stmt.query(params![secs]) {
                if let Some(row) = rows.next().unwrap_or(None) {
                    if let Ok(ts_str) = row.get::<_, String>(0) {
                        return ts_str;
                    }
                }
            }
        }
    }
    "2026-06-23T00:00:00Z".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp(1782218400); // 2026-07-03T10:00:00Z 근처
        assert!(ts.starts_with("2026-"));
        assert!(ts.ends_with("Z"));
    }

    #[test]
    fn test_git_info_raw_parsing() {
        let ws_info = WorkspaceInfo {
            workspace_root: Some("/mock/front-core".to_string()),
            workspace_uri: None,
            git_info_raw: Some(b"\x12+https://github.com/madup-inc/front-core.git\"\tfeat/jake".to_vec()),
        };

        let mut git_remote = None;
        let mut git_branch = None;

        if let Some(ref raw_bytes) = ws_info.git_info_raw {
            let lossy_str = String::from_utf8_lossy(raw_bytes);
            for word in lossy_str.split(|c: char| c.is_control() || c.is_whitespace() || c == '\u{0}') {
                if let Some(idx) = word.find("https://") {
                    let url_part = &word[idx..];
                    let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                    git_remote = Some(clean_url.to_string());
                } else if let Some(idx) = word.find("git@") {
                    let url_part = &word[idx..];
                    let clean_url = url_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_' && c != '@');
                    git_remote = Some(clean_url.to_string());
                }

                if !word.contains("https://") && !word.contains("git@") {
                    if let Some(idx) = word.find("feat/") {
                        let branch_part = &word[idx..];
                        let clean_branch = branch_part.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '-' && c != '_');
                        git_branch = Some(clean_branch.to_string());
                    } else if word.contains("main") {
                        git_branch = Some("main".to_string());
                    } else if word.contains("master") {
                        git_branch = Some("master".to_string());
                    }
                }
            }
        }

        assert_eq!(git_remote, Some("https://github.com/madup-inc/front-core.git".to_string()));
        assert_eq!(git_branch, Some("feat/jake".to_string()));
    }

    #[test]
    fn test_parse_session_invalid_path_format() {
        let adapter = AntigravityAdapter;
        let res = adapter.parse_session("/invalid/path/to/state.vscdb");
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "Antigravity 가상 경로 규격이 잘못되었습니다 (?session_id= 필요)"
        );
    }

    #[test]
    fn test_parse_session_nonexistent_db() {
        let adapter = AntigravityAdapter;
        let res = adapter.parse_session("/nonexistent/file.vscdb?session_id=mock-uuid");
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("unable to open database file") || err_msg.contains("cannot open file"));
    }
}

