//! 오작동 감지 룰 엔진 및 세션 매칭 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

use rusqlite::{Connection, params};
use crate::db;
use crate::model::{Session, Message, Node, ToolCall, MalfunctionRule, CohortMetric, CohortKey};
use regex::Regex;
use chrono::DateTime;
use std::collections::HashMap;

/// 세션 분석 메트릭 데이터 컨텍스트
pub struct SessionMalfunctionContext {
    pub session: Session,
    pub provider: Option<String>,
    pub messages: Vec<Message>,
    pub tool_calls: Vec<ToolCall>,
    pub nodes: Vec<Node>,
    pub max_delay_sec: u64,
    pub consecutive_tool_failures: usize,
    pub tool_consecutive_failures_map: HashMap<String, usize>,
    pub mcp_tool_counts: HashMap<String, HashMap<String, usize>>,
    pub mcp_server_counts: HashMap<String, usize>,
    pub error_texts: Vec<String>,
    pub max_ping_pong_cycles: usize,
    pub max_cyclic_loop_cycles: usize,
    pub max_repeated_calls: usize,
    pub token_inefficiency_ratio: f64,
    pub total_cost_usd: f64,
    pub max_tool_calls_per_turn: usize,
    pub total_turn_count: usize,
    pub user_denied_count: usize,
    pub user_interrupt_count: usize,
    pub max_edits_single_file: usize,
    pub subagent_anomaly_count: usize,
}

impl SessionMalfunctionContext {
    /// 데이터베이스로부터 세션 데이터를 로드하고 메트릭을 집계합니다.
    pub fn load(conn: &Connection, session_id: &str) -> Result<Self, rusqlite::Error> {
        let session = match db::get_session(conn, session_id)? {
            Some(s) => s,
            None => return Err(rusqlite::Error::QueryReturnedNoRows),
        };

        // 1. 제공사(provider) 파악
        let mut provider = None;
        if let Some(ref model_id) = session.model_id {
            if let Ok(pricings) = db::get_all_pricings(conn) {
                if let Some(pricing) = pricings.get(model_id) {
                    provider = Some(pricing.provider.clone());
                }
            }
        }

        let messages = db::get_messages_by_session(conn, session_id)?;
        let tool_calls = db::get_tool_calls_by_session(conn, session_id)?;
        let nodes = db::get_nodes_by_session(conn, session_id)?;

        // 2. 최대 답변 지연 시간 계산 (턴 단위 시간 차이)
        let mut max_delay_sec = 0;
        let mut sorted_msgs = messages.clone();
        // created_at 기준으로 정렬
        sorted_msgs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        for i in 1..sorted_msgs.len() {
            let prev = &sorted_msgs[i - 1];
            let curr = &sorted_msgs[i];
            
            // user 메시지 직후 agent/assistant 메시지의 시간차를 계산
            if prev.role == "user" && (curr.role == "agent" || curr.role == "assistant") {
                if let (Ok(p_time), Ok(c_time)) = (
                    DateTime::parse_from_rfc3339(&prev.created_at),
                    DateTime::parse_from_rfc3339(&curr.created_at),
                ) {
                    let diff = c_time.signed_duration_since(p_time).num_seconds();
                    if diff > 0 && (diff as u64) > max_delay_sec {
                        max_delay_sec = diff as u64;
                    }
                }
            }
        }

        // 3. 도구 연속 실패 횟수 및 각 도구별 최대 연속 실패 계산
        let mut consecutive_tool_failures = 0;
        let mut max_consecutive_tool_failures = 0;
        let mut tool_consecutive_failures_map: HashMap<String, usize> = HashMap::new();
        let mut current_tool_consecutive_failures: HashMap<String, usize> = HashMap::new();

        for tc in &tool_calls {
            if !tc.success {
                consecutive_tool_failures += 1;
                if consecutive_tool_failures > max_consecutive_tool_failures {
                    max_consecutive_tool_failures = consecutive_tool_failures;
                }

                let entry = current_tool_consecutive_failures.entry(tc.tool_name.clone()).or_insert(0);
                *entry += 1;
                let max_entry = tool_consecutive_failures_map.entry(tc.tool_name.clone()).or_insert(0);
                if *entry > *max_entry {
                    *max_entry = *entry;
                }
            } else {
                consecutive_tool_failures = 0;
                current_tool_consecutive_failures.insert(tc.tool_name.clone(), 0);
            }
        }

        // 4. MCP 서버/도구 호출 횟수 집계 및 에러 텍스트 수집
        let mut mcp_tool_counts: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut mcp_server_counts: HashMap<String, usize> = HashMap::new();
        let mut error_texts = Vec::new();
        let mut user_denied_count = 0;

        for tc in &tool_calls {
            if tc.is_mcp {
                if let Some(ref server) = tc.mcp_server {
                    *mcp_server_counts.entry(server.clone()).or_insert(0) += 1;
                    if let Some(ref tool) = tc.mcp_tool {
                        let tool_map = mcp_tool_counts.entry(server.clone()).or_insert_with(HashMap::new);
                        *tool_map.entry(tool.clone()).or_insert(0) += 1;
                    }
                }
            }

            // 에러/실패 텍스트 수집
            if !tc.success {
                if let Some(ref input) = tc.tool_input {
                    error_texts.push(input.clone());
                }
                
                // 사용자 거절 징후 감지 (tool_input 혹은 임의의 결과 필드)
                if let Some(ref input) = tc.tool_input {
                    let input_lower = input.to_lowercase();
                    if input_lower.contains("user denied") || input_lower.contains("cancelled") || input_lower.contains("rejected") {
                        user_denied_count += 1;
                    }
                }
            }
        }

        // 메시지 본문에서도 텍스트 수집
        for msg in &messages {
            if let Some(ref content) = msg.content {
                error_texts.push(content.clone());
            }
        }

        // 5. 동적 핑퐁 루프 계산
        let max_ping_pong_cycles = calculate_ping_pong(&tool_calls);

        // 6. 다중 순환 루프 계산 (3개 이상 도구 순환)
        let max_cyclic_loop_cycles = calculate_cyclic_loops(&tool_calls, 3);

        // 7. 동적 반복 호출 계산 (동일 도구 & 동일 인자 해시)
        let max_repeated_calls = calculate_repeated_calls(&tool_calls);

        // 8. 토큰 효율성 계산 (출력 / 입력)
        let token_inefficiency_ratio = if session.total_input_tokens > 0 {
            (session.total_output_tokens as f64) / (session.total_input_tokens as f64)
        } else {
            1.0
        };

        // 9. 세션 총 비용 계산
        let total_cost_usd: f64 = messages.iter().map(|m| m.cost_usd).sum();

        // 10. 한 턴 내부 도구 호출 최대값 계산
        let mut max_tool_calls_per_turn = 0;
        let mut turn_tool_counts: HashMap<u64, usize> = HashMap::new();
        for tc in &tool_calls {
            if let Ok(tc_time) = DateTime::parse_from_rfc3339(&tc.created_at) {
                let mut best_turn = 0;
                let mut min_diff = i64::MAX;
                for msg in &messages {
                    if let Ok(msg_time) = DateTime::parse_from_rfc3339(&msg.created_at) {
                        let diff = (tc_time.signed_duration_since(msg_time).num_milliseconds()).abs();
                        if diff < min_diff {
                            min_diff = diff;
                            best_turn = msg.turn_index;
                        }
                    }
                }
                *turn_tool_counts.entry(best_turn).or_insert(0) += 1;
            }
        }
        for &count in turn_tool_counts.values() {
            if count > max_tool_calls_per_turn {
                max_tool_calls_per_turn = count;
            }
        }

        // 11. 총 턴 수 계산
        let total_turn_count = messages.iter().map(|m| m.turn_index).max().unwrap_or(0) as usize;

        // 12. 자식 세션 오작동 감지 횟수 계산
        let mut subagent_anomaly_count = 0;
        let mut stmt = conn.prepare(
            "SELECT COUNT(DISTINCT md.session_id)
             FROM sessions s
             JOIN malfunction_detections md ON md.session_id = s.session_id
             WHERE s.parent_session_id = ?1",
        )?;
        if let Ok(count) = stmt.query_row(params![session_id], |row| row.get::<_, usize>(0)) {
            subagent_anomaly_count = count;
        }

        let mut user_interrupt_count = 0;
        if session.token_source != "estimated" {
            for msg in &messages {
                if msg.role == "user" {
                    if let Some(ref content) = msg.content {
                        let content_lower = content.to_lowercase();
                        if content_lower.contains("interrupted by user") {
                            user_interrupt_count += 1;
                        }
                    }
                }
            }
        }

        let mut file_counts: HashMap<String, usize> = HashMap::new();
        for tc in &tool_calls {
            if ["Edit", "Write", "NotebookEdit"].contains(&tc.tool_name.as_str()) {
                if let Some(ref input) = tc.tool_input {
                    if let Some(path) = extract_file_path(input) {
                        *file_counts.entry(path).or_insert(0) += 1;
                    }
                }
            }
        }
        let max_edits_single_file = file_counts.values().copied().max().unwrap_or(0);

        Ok(Self {
            session,
            provider,
            messages,
            tool_calls,
            nodes,
            max_delay_sec,
            consecutive_tool_failures: max_consecutive_tool_failures,
            tool_consecutive_failures_map,
            mcp_tool_counts,
            mcp_server_counts,
            error_texts,
            max_ping_pong_cycles,
            max_cyclic_loop_cycles,
            max_repeated_calls,
            token_inefficiency_ratio,
            total_cost_usd,
            max_tool_calls_per_turn,
            total_turn_count,
            user_denied_count,
            user_interrupt_count,
            max_edits_single_file,
            subagent_anomaly_count,
        })
    }

    /// 특정 오작동 규칙을 평가하고 (매칭 여부, 매칭 근거 설명)을 반환합니다.
    pub fn eval_rule(&self, conn: &Connection, rule: &MalfunctionRule) -> (bool, String) {
        match rule {
            MalfunctionRule::TargetAgentTypes { agent_types } => {
                let matched = agent_types.contains(&self.session.agent_type);
                let evidence = if matched {
                    format!("에이전트 타입 매칭: '{}'", self.session.agent_type)
                } else {
                    format!("에이전트 타입 불일치 (실제: '{}')", self.session.agent_type)
                };
                (matched, evidence)
            }
            MalfunctionRule::TargetModelIds { model_ids } => {
                let current_model = self.session.model_id.as_deref().unwrap_or("");
                let matched = model_ids.iter().any(|m| m == current_model);
                let evidence = if matched {
                    format!("모델 ID 매칭: '{}'", current_model)
                } else {
                    format!("모델 ID 불일치 (실제: '{}')", current_model)
                };
                (matched, evidence)
            }
            MalfunctionRule::TargetProviders { providers } => {
                let current_provider = self.provider.as_deref().unwrap_or("");
                let matched = providers.iter().any(|p| p == current_provider);
                let evidence = if matched {
                    format!("제공사 매칭: '{}'", current_provider)
                } else {
                    format!("제공사 불일치 (실제: '{}')", current_provider)
                };
                (matched, evidence)
            }
            MalfunctionRule::UnexpectedExit { value } => {
                let actual_exit = self.session.ended_at.is_none() || self.nodes.iter().any(|n| !n.success);
                let matched = actual_exit == *value;
                let evidence = if matched {
                    format!("예상치 못한 종료 상태 일치: 실제={}", actual_exit)
                } else {
                    format!("예상치 못한 종료 상태 불일치: 실제={}", actual_exit)
                };
                (matched, evidence)
            }
            MalfunctionRule::MaxResponseDelaySec { value } => {
                let matched = self.max_delay_sec >= *value;
                let evidence = if matched {
                    format!("응답 지연 기준 초과: 실제 최대 지연 {}초 (임계치 {}초)", self.max_delay_sec, value)
                } else {
                    format!("응답 지연 기준 미달: 실제 최대 지연 {}초 (임계치 {}초)", self.max_delay_sec, value)
                };
                (matched, evidence)
            }
            MalfunctionRule::ConsecutiveToolFailures { tool_name, count_threshold } => {
                let actual_failures = if let Some(ref name) = tool_name {
                    *self.tool_consecutive_failures_map.get(name).unwrap_or(&0)
                } else {
                    self.consecutive_tool_failures
                };
                let matched = actual_failures >= *count_threshold;
                let evidence = if matched {
                    format!("도구 연속 실패 임계치 초과: 도구={:?}, 실제 연속 실패={}회 (임계치 {}회)", tool_name, actual_failures, count_threshold)
                } else {
                    format!("도구 연속 실패 임계치 미달: 도구={:?}, 실제 연속 실패={}회 (임계치 {}회)", tool_name, actual_failures, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::PluginTriggerLimit { mcp_server, mcp_tool, count_threshold } => {
                let actual_count = if let Some(ref tool) = mcp_tool {
                    self.mcp_tool_counts.get(mcp_server)
                        .and_then(|m| m.get(tool))
                        .copied()
                        .unwrap_or(0)
                } else {
                    *self.mcp_server_counts.get(mcp_server).unwrap_or(&0)
                };
                let matched = actual_count >= *count_threshold;
                let evidence = if matched {
                    format!("MCP 플러그인 호출 임계치 초과: 서버={}, 도구={:?}, 호출={}회 (임계치 {}회)", mcp_server, mcp_tool, actual_count, count_threshold)
                } else {
                    format!("MCP 플러그인 호출 임계치 미달: 서버={}, 도구={:?}, 호출={}회 (임계치 {}회)", mcp_server, mcp_tool, actual_count, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::ErrorMessagePatterns { patterns, is_regex } => {
                let mut matched = false;
                let mut matched_pattern = String::new();
                for pattern in patterns {
                    if *is_regex {
                        if let Ok(re) = Regex::new(pattern) {
                            if self.error_texts.iter().any(|txt| re.is_match(txt)) {
                                matched = true;
                                matched_pattern = pattern.to_string();
                                break;
                            }
                        }
                    } else if self.error_texts.iter().any(|txt| txt.contains(pattern)) {
                        matched = true;
                        matched_pattern = pattern.to_string();
                        break;
                    }
                }
                let evidence = if matched {
                    format!("에러 메시지 패턴 감지: 패턴='{}'", matched_pattern)
                } else {
                    "에러 메시지 패턴 미감지".to_string()
                };
                (matched, evidence)
            }
            MalfunctionRule::DynamicPingPong { cycles_threshold } => {
                let matched = self.max_ping_pong_cycles >= *cycles_threshold;
                let evidence = if matched {
                    format!("임의의 두 도구 간 핑퐁 감지: 실제 왕복 {}회 (임계치 {}회)", self.max_ping_pong_cycles, cycles_threshold)
                } else {
                    format!("임의의 두 도구 간 핑퐁 미달: 실제 왕복 {}회 (임계치 {}회)", self.max_ping_pong_cycles, cycles_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::DynamicCyclicLoop { window_size, cycles_threshold } => {
                let actual_cycles = calculate_cyclic_loops(&self.tool_calls, *window_size);
                let matched = actual_cycles >= *cycles_threshold;
                let evidence = if matched {
                    format!("임의의 다중 도구 순환 루프 감지: 윈도우 크기={}, 실제 순환 {}회 (임계치 {}회)", window_size, actual_cycles, cycles_threshold)
                } else {
                    format!("임의의 다중 도구 순환 루프 미달: 윈도우 크기={}, 실제 순환 {}회 (임계치 {}회)", window_size, actual_cycles, cycles_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::DynamicRepeatedCalls { count_threshold } => {
                let matched = self.max_repeated_calls >= *count_threshold;
                let evidence = if matched {
                    format!("임의의 동일 도구 연속 반복 호출 감지: 실제 연속 {}회 (임계치 {}회)", self.max_repeated_calls, count_threshold)
                } else {
                    format!("임의의 동일 도구 연속 반복 호출 미달: 실제 연속 {}회 (임계치 {}회)", self.max_repeated_calls, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::TokenInefficiency { ratio_threshold } => {
                let is_inefficient = self.token_inefficiency_ratio < *ratio_threshold 
                    && self.session.total_input_tokens > 10_000
                    && self.session.token_source == "api"
                    && self.session.total_output_tokens > 0;
                let matched = is_inefficient;
                let evidence = if matched {
                    format!("토큰 소모 효율성 낮음 감지: 실제 생성 비율 {:.4} (임계치 {:.4}, 누적 입력={}, 누적 출력={})", self.token_inefficiency_ratio, ratio_threshold, self.session.total_input_tokens, self.session.total_output_tokens)
                } else {
                    format!("토큰 소모 효율성 정상: 실제 생성 비율 {:.4} (임계치 {:.4}, 누적 입력={}, 누적 출력={})", self.token_inefficiency_ratio, ratio_threshold, self.session.total_input_tokens, self.session.total_output_tokens)
                };
                (matched, evidence)
            }
            MalfunctionRule::MaxSessionCostUsd { limit_usd } => {
                let matched = self.total_cost_usd >= *limit_usd;
                let evidence = if matched {
                    format!("세션 허용 비용 초과: 실제 비용 ${:.4} (임계치 ${:.4})", self.total_cost_usd, limit_usd)
                } else {
                    format!("세션 허용 비용 정상: 실제 비용 ${:.4} (임계치 ${:.4})", self.total_cost_usd, limit_usd)
                };
                (matched, evidence)
            }
            MalfunctionRule::MaxToolCallsPerTurn { count_threshold } => {
                let matched = self.max_tool_calls_per_turn >= *count_threshold;
                let evidence = if matched {
                    format!("단일 턴 내 과다 도구 호출 감지: 실제 최대 호출 {}회 (임계치 {}회)", self.max_tool_calls_per_turn, count_threshold)
                } else {
                    format!("단일 턴 내 도구 호출 정상: 실제 최대 호출 {}회 (임계치 {}회)", self.max_tool_calls_per_turn, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::MaxTurnCount { count_threshold } => {
                let matched = self.total_turn_count >= *count_threshold;
                let evidence = if matched {
                    format!("세션 대화 턴 수 초과: 실제 턴 수 {}턴 (임계치 {}턴)", self.total_turn_count, count_threshold)
                } else {
                    format!("세션 대화 턴 수 정상: 실제 턴 수 {}턴 (임계치 {}턴)", self.total_turn_count, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::UserInterruptionLimit { count_threshold } => {
                let matched = self.user_interrupt_count >= *count_threshold;
                let evidence = if matched {
                    format!("사용자 실행 중단 횟수 초과: 실제 중단 {}회 (임계치 {}회)", self.user_interrupt_count, count_threshold)
                } else {
                    format!("사용자 실행 중단 횟수 정상: 실제 중단 {}회 (임계치 {}회)", self.user_interrupt_count, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::UserCorrectionSignal { patterns, is_regex, count_threshold } => {
                let mut matched_count = 0;
                if self.session.token_source != "estimated" {
                    for msg in &self.messages {
                        if msg.role == "user" {
                            if let Some(ref content) = msg.content {
                                for pattern in patterns {
                                    let is_matched = if *is_regex {
                                        if let Ok(re) = Regex::new(pattern) {
                                            re.is_match(content)
                                        } else {
                                            false
                                        }
                                    } else {
                                        content.contains(pattern)
                                    };
                                    if is_matched {
                                        matched_count += 1;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                let matched = matched_count >= *count_threshold;
                let evidence = if matched {
                    format!("사용자 정정 신호 감지: 실제 정정 {}회 (임계치 {}회)", matched_count, count_threshold)
                } else {
                    format!("사용자 정정 신호 미달: 실제 정정 {}회 (임계치 {}회)", matched_count, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::FileChurn { min_edits_same_file, tools, require_hash_revisit } => {
                if self.session.token_source == "estimated" {
                    (false, "antigravity 합성 세션 제외".to_string())
                } else {
                    let mut file_history: HashMap<String, Vec<&ToolCall>> = HashMap::new();
                    for tc in &self.tool_calls {
                        if tools.contains(&tc.tool_name) {
                            if let Some(ref input) = tc.tool_input {
                                if let Some(path) = extract_file_path(input) {
                                    file_history.entry(path).or_default().push(tc);
                                }
                            }
                        }
                    }

                    let mut max_edits = 0;
                    let mut has_revisit = false;
                    let mut target_file_matched = String::new();

                    for (path, calls) in file_history {
                        let edit_count = calls.len();
                        if edit_count > max_edits {
                            max_edits = edit_count;
                        }
                        
                        let mut seen_hashes = std::collections::HashSet::new();
                        let mut current_revisit = false;
                        for tc in &calls {
                            if !tc.input_hash.is_empty() {
                                if seen_hashes.contains(&tc.input_hash) {
                                    current_revisit = true;
                                }
                                seen_hashes.insert(tc.input_hash.clone());
                            }
                        }
                        if edit_count >= *min_edits_same_file {
                            if !*require_hash_revisit || current_revisit {
                                has_revisit = true;
                                target_file_matched = path;
                            }
                        }
                    }

                    let matched = max_edits >= *min_edits_same_file && (!*require_hash_revisit || has_revisit);
                    let evidence = if matched {
                        format!("동일 파일 반복 수정(FileChurn) 감지: 파일='{}', 최대 편집 {}회 (임계치 {}회, hash_revisit={})", target_file_matched, max_edits, min_edits_same_file, require_hash_revisit)
                    } else {
                        format!("동일 파일 반복 수정(FileChurn) 미달: 최대 편집 {}회 (임계치 {}회)", max_edits, min_edits_same_file)
                    };
                    (matched, evidence)
                }
            }
            MalfunctionRule::CohortPercentileExceeds { metric, cohort_by, percentile, min_cohort_n } => {
                let cohort_val_opt = match cohort_by {
                    CohortKey::AgentType => Some(self.session.agent_type.clone()),
                    CohortKey::Cwd => Some(self.session.cwd.clone()),
                    CohortKey::ModelId => self.session.model_id.clone(),
                };

                if let Some(cohort_val) = cohort_val_opt {
                    match db::get_cohort_session_metrics(conn, &self.session.session_id, cohort_by.clone(), &cohort_val, metric.clone()) {
                        Ok(mut metrics) => {
                            if metrics.len() < *min_cohort_n {
                                (false, format!("코호트 표본 부족: 현재 {}개 (임계치 {}개)", metrics.len(), min_cohort_n))
                            } else {
                                // 현재 세션의 실제 메트릭 값 계산
                                let current_val = match metric {
                                    CohortMetric::CostUsd => self.total_cost_usd,
                                    CohortMetric::TurnCount => self.total_turn_count as f64,
                                    CohortMetric::ToolCallCount => self.tool_calls.len() as f64,
                                    CohortMetric::ResultTokens => self.tool_calls.iter().map(|tc| tc.result_est_tokens.unwrap_or(0)).sum::<i64>() as f64,
                                    CohortMetric::MaxEditsPerFile => self.max_edits_single_file as f64,
                                };

                                // 백분위 기준값 계산
                                metrics.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                                let len = metrics.len();
                                let index = (len * (*percentile as usize) / 100).min(len - 1);
                                let threshold_val = metrics[index];

                                let matched = current_val > threshold_val;
                                let evidence = if matched {
                                    format!("코호트 이상치 초과 (Key: {:?}, Metric: {:?}): 실제값 {:.4} > p{} 기준값 {:.4} (표본 {}개)", cohort_by, metric, current_val, percentile, threshold_val, len)
                                } else {
                                    format!("코호트 이상치 정상 (Key: {:?}, Metric: {:?}): 실제값 {:.4} <= p{} 기준값 {:.4} (표본 {}개)", cohort_by, metric, current_val, percentile, threshold_val, len)
                                };
                                (matched, evidence)
                            }
                        }
                        Err(e) => (false, format!("코호트 조회 실패: {}", e)),
                    }
                } else {
                    (false, format!("코호트 기준값 누락 (Key: {:?})", cohort_by))
                }
            }
            MalfunctionRule::SubagentAnomalyLimit { count_threshold } => {
                let matched = self.subagent_anomaly_count >= *count_threshold;
                let evidence = if matched {
                    format!("자식 세션 오작동 연쇄 감지: 실제 오작동 자식 세션 수 {}개 (임계치 {}개)", self.subagent_anomaly_count, count_threshold)
                } else {
                    format!("자식 세션 오작동 연쇄 정상: 실제 오작동 자식 세션 수 {}개 (임계치 {}개)", self.subagent_anomaly_count, count_threshold)
                };
                (matched, evidence)
            }
            MalfunctionRule::Sequence { steps } => {
                let mut matched = true;
                let mut evidence_list = Vec::new();
                for (idx, step) in steps.iter().enumerate() {
                    let (step_matched, step_evidence) = self.eval_rule(conn, step);
                    if !step_matched {
                        matched = false;
                        evidence_list.push(format!("단계 {}: 실패 ({})", idx + 1, step_evidence));
                        break;
                    } else {
                        evidence_list.push(format!("단계 {}: 통과 ({})", idx + 1, step_evidence));
                    }
                }
                let evidence = format!("시퀀스 흐름 검증 결과: matched={}, 상세=[{}]", matched, evidence_list.join(" -> "));
                (matched, evidence)
            }
            MalfunctionRule::And { conditions } => {
                let mut matched = true;
                let mut evidence_list = Vec::new();
                for cond in conditions {
                    let (cond_matched, cond_evidence) = self.eval_rule(conn, cond);
                    evidence_list.push(cond_evidence);
                    if !cond_matched {
                        matched = false;
                    }
                }
                let evidence = format!("AND 조합 검증 ({})", evidence_list.join(" AND "));
                (matched, evidence)
            }
            MalfunctionRule::Or { conditions } => {
                let mut matched = false;
                let mut evidence_list = Vec::new();
                for cond in conditions {
                    let (cond_matched, cond_evidence) = self.eval_rule(conn, cond);
                    evidence_list.push(cond_evidence);
                    if cond_matched {
                        matched = true;
                    }
                }
                let evidence = format!("OR 조합 검증 ({})", evidence_list.join(" OR "));
                (matched, evidence)
            }
            MalfunctionRule::Not { condition } => {
                let (cond_matched, cond_evidence) = self.eval_rule(conn, condition);
                let matched = !cond_matched;
                let evidence = format!("NOT 검증: 내부 조건 결과={} ({})", cond_matched, cond_evidence);
                (matched, evidence)
            }
        }
    }
}

/// 특정 세션을 대상으로 데이터베이스에 적재된 모든 오작동 패턴을 불러와 매칭을 수행하고 결과를 적재합니다.
pub fn analyze_and_detect_malfunctions(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<(i64, String)>, rusqlite::Error> {
    let ctx = SessionMalfunctionContext::load(conn, session_id)?;
    let patterns = db::get_malfunction_patterns(conn)?;
    let mut detected = Vec::new();

    for pattern in patterns {
        if let Ok(rule) = serde_json::from_str::<MalfunctionRule>(&pattern.rules_json) {
            let (is_matched, evidence) = ctx.eval_rule(conn, &rule);
            if is_matched {
                db::insert_malfunction_detection(conn, session_id, pattern.id, &evidence)?;
                detected.push((pattern.id, evidence));
            }
        }
    }

    Ok(detected)
}

fn calculate_repeated_calls(tool_calls: &[ToolCall]) -> usize {
    if tool_calls.is_empty() {
        return 0;
    }
    let mut max_count = 1;
    let mut consecutive_count = 1;
    for i in 1..tool_calls.len() {
        let prev = &tool_calls[i - 1];
        let curr = &tool_calls[i];
        if prev.tool_name == curr.tool_name && prev.input_hash == curr.input_hash {
            consecutive_count += 1;
            if consecutive_count > max_count {
                max_count = consecutive_count;
            }
        } else {
            consecutive_count = 1;
        }
    }
    max_count
}

fn calculate_ping_pong(tool_calls: &[ToolCall]) -> usize {
    if tool_calls.len() < 4 {
        return 0;
    }
    let names: Vec<&str> = tool_calls.iter().map(|tc| tc.tool_name.as_str()).collect();
    let mut max_cycles = 0;

    for i in 0..names.len() {
        if i + 3 >= names.len() {
            break;
        }
        let tool_a = names[i];
        let tool_b = names[i + 1];
        if tool_a == tool_b {
            continue;
        }

        let mut cycles = 0;
        let mut idx = i;
        while idx + 1 < names.len() {
            if names[idx] == tool_a && names[idx + 1] == tool_b {
                cycles += 1;
                idx += 2;
            } else {
                break;
            }
        }
        if cycles > max_cycles {
            max_cycles = cycles;
        }
    }
    max_cycles
}

fn calculate_cyclic_loops(tool_calls: &[ToolCall], window_size: usize) -> usize {
    let min_len = window_size * 2;
    if tool_calls.len() < min_len || window_size < 2 {
        return 0;
    }
    let names: Vec<&str> = tool_calls.iter().map(|tc| tc.tool_name.as_str()).collect();
    let mut max_cycles = 0;

    for i in 0..=(names.len() - min_len) {
        let pattern = &names[i..i + window_size];
        let mut unique_tools = pattern.to_vec();
        unique_tools.sort_unstable();
        unique_tools.dedup();
        if unique_tools.len() != window_size {
            continue;
        }

        let mut cycles = 0;
        let mut idx = i;
        while idx + window_size <= names.len() {
            let chunk = &names[idx..idx + window_size];
            if chunk == pattern {
                cycles += 1;
                idx += window_size;
            } else {
                break;
            }
        }
        if cycles > max_cycles {
            max_cycles = cycles;
        }
    }
    max_cycles
}

/// 입력된 JSON 패턴이 유효한 MalfunctionRule 형태인지 파싱을 테스트하고,
/// 최근 생성된 세션 N개에 대해 오탐(False Positive) 여부를 추정 테스트합니다.
pub fn validate_malfunction_pattern(
    conn: &Connection,
    rules_json: &str,
    limit: usize,
) -> Result<(bool, String, f64, bool, Vec<(String, String)>), rusqlite::Error> {
    let rule: MalfunctionRule = match serde_json::from_str(rules_json) {
        Ok(r) => r,
        Err(e) => {
            return Ok((
                false,
                format!("❌ 유효하지 않은 JSON/규칙 형식: {}", e),
                0.0,
                false,
                Vec::new(),
            ));
        }
    };

    let mut stmt = conn.prepare(
        "SELECT session_id FROM sessions ORDER BY started_at DESC LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| row.get::<_, String>(0))?;
    let mut session_ids = Vec::new();
    for r in rows {
        session_ids.push(r?);
    }

    if session_ids.is_empty() {
        return Ok((
            true,
            "✅ 규칙 형식이 유효합니다. (데이터베이스에 테스트할 세션이 존재하지 않아 FP 추정을 스킵합니다)".to_string(),
            0.0,
            false,
            Vec::new(),
        ));
    }

    let total_test_count = session_ids.len();
    let mut matched_samples = Vec::new();

    for sid in &session_ids {
        if let Ok(ctx) = SessionMalfunctionContext::load(conn, sid) {
            let (matched, evidence) = ctx.eval_rule(conn, &rule);
            if matched {
                matched_samples.push((sid.clone(), evidence));
            }
        }
    }

    let match_count = matched_samples.len();
    let match_ratio = (match_count as f64) / (total_test_count as f64);
    let is_fp_suspect = match_ratio >= 0.3;

    let summary_msg = if is_fp_suspect {
        format!(
            "⚠️ 규칙 형식이 유효하나, 최근 테스트한 세션 {}개 중 {}개({:.1}%)가 매칭되어 False Positive(오탐) 확률이 높습니다.",
            total_test_count, match_count, match_ratio * 100.0
        )
    } else {
        format!(
            "✅ 규칙 형식이 유효하며, 최근 테스트한 세션 {}개 중 {}개({:.1}%)만 매칭되어 정상 범위 내로 보입니다.",
            total_test_count, match_count, match_ratio * 100.0
        )
    };

    Ok((true, summary_msg, match_ratio, is_fp_suspect, matched_samples))
}

pub fn extract_file_path(tool_input: &str) -> Option<String> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(tool_input) {
        if let Some(obj) = val.as_object() {
            for key in &["file_path", "path", "target_file", "TargetFile", "targetFile", "absolute_path", "AbsolutePath", "Target"] {
                if let Some(v) = obj.get(*key).and_then(|v| v.as_str()) {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

