//! 에이전트 로그 수집 어댑터 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

pub mod codex;
pub mod antigravity;
pub mod claude_code;
pub mod ingest;

use serde_json::Value;
use std::collections::BTreeMap;
use xxhash_rust::xxh3::xxh3_64;

/// 단일 세션 로그 파싱 결과를 묶어 데이터베이스 적재를 돕는 구조체
#[derive(Debug, Clone)]
pub struct NormalizedSession {
    pub session: crate::model::Session,
    pub messages: Vec<crate::model::Message>,
    pub nodes: Vec<crate::model::Node>,
    pub tool_calls: Vec<crate::model::ToolCall>,
}

/// 어댑터들이 구현해야 하는 공통 트레이트
pub trait LogAdapter {
    /// 지정된 파일/경로에서 세션 데이터를 파싱하고 정규화된 묶음 데이터를 반환합니다.
    fn parse_session(&self, path: &str) -> Result<NormalizedSession, Box<dyn std::error::Error>>;
}

/// tool_input JSON 값을 정규화합니다.
/// 1. 객체의 키를 알파벳 순으로 정렬합니다.
/// 2. 휘발성 필드(toolSummary, toolAction, Reason, reason)를 재귀적으로 제거합니다.
/// 3. 정형화된 컴팩트 JSON 문자열로 직렬화하여 반환합니다.
pub fn normalize_tool_input(input_val: &Value) -> String {
    let normalized_val = normalize_value(input_val);
    serde_json::to_string(&normalized_val).unwrap_or_else(|_| "{}".to_string())
}

/// 재귀적으로 Value 내부를 정규화하는 헬퍼 함수
fn normalize_value(val: &Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut sorted_map = BTreeMap::new();
            for (k, v) in map {
                // 휘발성 필드 스킵
                if k == "toolSummary" || k == "toolAction" || k == "Reason" || k == "reason" {
                    continue;
                }
                // 재귀적 자식 정규화
                sorted_map.insert(k.clone(), normalize_value(v));
            }
            // BTreeMap을 다시 serde_json::Map으로 변환
            let mut new_map = serde_json::Map::new();
            for (k, v) in sorted_map {
                new_map.insert(k, v);
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            let mut new_arr = Vec::new();
            for item in arr {
                new_arr.push(normalize_value(item));
            }
            Value::Array(new_arr)
        }
        _ => val.clone(), // String, Number, Bool, Null 등은 그대로 유지
    }
}

/// 정규화된 tool_input 문자열에 대해 xxh3_64 16진수 해시값을 산출합니다.
pub fn calculate_input_hash(input_val: &Value) -> String {
    let normalized = normalize_tool_input(input_val);
    let hash = xxh3_64(normalized.as_bytes());
    format!("{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_tool_input_sorting_and_filtering() {
        // 1. 키 순서가 다른 경우의 정규화 결과 일치 확인
        let val1 = json!({"path": "a.txt", "content": "hello"});
        let val2 = json!({"content": "hello", "path": "a.txt"});
        assert_eq!(normalize_tool_input(&val1), normalize_tool_input(&val2));
        assert_eq!(calculate_input_hash(&val1), calculate_input_hash(&val2));

        // 2. 휘발성 필드가 포함된 경우의 필터링 확인
        let val_volatile = json!({
            "path": "a.txt",
            "content": "hello",
            "toolSummary": "파일 수정",
            "toolAction": "Editing file",
            "Reason": "임시 테스트 사유",
            "reason": "소문자 사유"
        });
        let val_clean = json!({"content": "hello", "path": "a.txt"});
        assert_eq!(normalize_tool_input(&val_volatile), normalize_tool_input(&val_clean));
        assert_eq!(calculate_input_hash(&val_volatile), calculate_input_hash(&val_clean));

        // 3. 중첩된 객체 내의 휘발성 필드 제거 및 정렬 확인
        let nested_volatile = json!({
            "outer_key": "val",
            "nested_obj": {
                "b": 2,
                "a": 1,
                "toolSummary": "중첩 요약"
            }
        });
        let nested_clean = json!({
            "nested_obj": {
                "a": 1,
                "b": 2
            },
            "outer_key": "val"
        });
        assert_eq!(normalize_tool_input(&nested_volatile), normalize_tool_input(&nested_clean));
    }
}
