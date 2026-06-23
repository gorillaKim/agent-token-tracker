//! Agent Token Tracker CLI 진입점
//!
//! 에이전트들의 활동 로그 및 토큰 사용량을 분석하고 시각화하는 도구입니다.

pub mod model;
pub mod db;
pub mod pricing;
pub mod adapters;
pub mod detect;
pub mod tui;

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use adapters::{LogAdapter, NormalizedSession};
use adapters::claude_code::ClaudeCodeAdapter;

#[derive(Parser)]
#[command(name = "agent-token-tracker")]
#[command(author = "gorillaKim")]
#[command(version = "0.1.0")]
#[command(about = "에이전트 토큰 사용량 및 활동 분석 관측 도구", long_about = None)]
struct Cli {
    #[arg(short, long, help = "사용할 SQLite 데이터베이스 파일 경로")]
    database: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "에이전트 로그 파일을 스캔하여 데이터베이스에 적재합니다.")]
    Scan {
        #[arg(short, long, help = "스캔할 대상 디렉토리 또는 파일 경로")]
        path: String,
        
        #[arg(short, long, help = "특정 에이전트 타입 필터 (예: codex, antigravity)")]
        agent: Option<String>,
    },
    #[command(about = "적재된 에이전트 세션의 토큰 사용량 및 비용 리포트를 출력합니다.")]
    Report {
        #[arg(short, long, help = "특정 세션 ID 필터 (session 차원 전용)")]
        session_id: Option<String>,

        #[arg(short, long, help = "롤업 기준 차원 (session, agent, tool) [기본값: session]")]
        dimension: Option<String>,

        #[arg(long, help = "정렬 기준 (cost, tokens, count)")]
        sort: Option<String>,

        #[arg(short, long, help = "출력할 최대 행 수")]
        limit: Option<usize>,

        #[arg(long, help = "조회 시작일 필터 (예: 2026-06-23)")]
        since: Option<String>,
    },
    #[command(about = "에이전트의 무한 루프 및 오작동 의심 세션을 탐지합니다.")]
    Loops {
        #[arg(short, long, help = "특정 세션 ID 필터")]
        session_id: Option<String>,

        #[arg(short, long, help = "특정 에이전트 타입 필터 (예: claude_code)")]
        agent: Option<String>,

        #[arg(long, help = "특정 이상 징후 시그널 종류 필터 (repeated_call, repeated_failure, token_inflation, ping_pong)")]
        signal: Option<String>,

        #[arg(long, help = "정렬 기준 (session_id, agent_type, started_at) [기본값: started_at]")]
        sort: Option<String>,

        #[arg(long, help = "동일 호출 반복 횟수 임계치 [기본값: 3]")]
        max_calls: Option<usize>,

        #[arg(long, help = "연속 실패 횟수 임계치 [기본값: 3]")]
        max_failures: Option<usize>,

        #[arg(long, help = "무진전 토큰 급증 임계치 [기본값: 50000]")]
        inflation: Option<u64>,

        #[arg(long, help = "핑퐁 반복 주기 임계치 [기본값: 3]")]
        ping_pong: Option<usize>,
    },
}

/// 스캔 결과를 요약 보고하기 위한 구조체 (이슈 #683 정책 준수)
#[derive(Debug, Default)]
pub struct ScanResult {
    pub files_total: usize,
    pub sessions_scanned: usize,
    pub sessions_inserted: usize,
    pub sessions_skipped: usize,
    pub sessions_failed: usize,
    pub skip_reasons: HashMap<String, usize>,
    pub warnings: Vec<String>,
}

/// 루프 탐지 아스키 리포트 출력을 위한 임시 행 구조체
#[derive(Debug)]
struct LoopRow {
    session_id: String,
    agent_type: String,
    started_at: String,
    signal_type: String,
    description: String,
    evidence: String,
}

fn main() {
    let cli = Cli::parse();
    let db_path = cli.database.unwrap_or_else(|| "atk.db".to_string());

    println!("데이터베이스 파일: {}", db_path);

    // 최초 DB 마이그레이션 및 멱등 초기화
    match db::init_db(&db_path) {
        Ok(_) => println!("데이터베이스 초기화 및 마이그레이션이 완료되었습니다."),
        Err(err) => {
            eprintln!("데이터베이스 초기화 실패: {}", err);
            std::process::exit(1);
        }
    }

    match &cli.command {
        Commands::Scan { path, agent } => {
            println!("스캔을 시작합니다. 대상 경로: {}", path);
            if let Some(agent_type) = agent {
                println!("필터링할 에이전트 타입: {}", agent_type);
            }

            // 1. 스캔 시작 전 단가 테이블 사전 캐싱 (O(1) 조회를 위해 HashMap 활용)
            let conn_init = match db::init_db(&db_path) {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("데이터베이스 초기화 실패: {}", err);
                    std::process::exit(1);
                }
            };
            let pricing_map = match db::get_all_pricings(&conn_init) {
                Ok(map) => map,
                Err(err) => {
                    eprintln!("단가 정보 조회 실패: {}", err);
                    std::process::exit(1);
                }
            };
            let pricing_map_shared = Arc::new(pricing_map);

            // 파일 목록 수집 (재귀 스캔)
            let mut files = Vec::new();
            if let Err(err) = collect_files(Path::new(path), &mut files) {
                eprintln!("파일 목록 수집 중 오류 발생: {}", err);
                std::process::exit(1);
            }

            let files_total = files.len();
            println!("총 {}개의 대상 파일을 발견했습니다.", files_total);

            // 스레드 안전한 결과 카운트 데이터 구조
            let result_accumulator = Arc::new(Mutex::new(ScanResult::default()));
            let db_path_clone = db_path.clone();

            // Rayon을 활용한 병렬 스캔 처리
            files.par_iter().for_each(|file_path| {
                let accumulator = Arc::clone(&result_accumulator);
                let pricing_cache = Arc::clone(&pricing_map_shared);
                
                // 파일 확장자 판별 (디스패치)
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "jsonl" {
                    return; // jsonl 이외의 파일은 조용히 스킵
                }

                // 1. 어댑터를 통해 파싱 수행
                let adapter = ClaudeCodeAdapter;
                let mut parsed_session: NormalizedSession = match adapter.parse_session(file_path.to_str().unwrap()) {
                    Ok(sess) => sess,
                    Err(err) => {
                        let mut res = accumulator.lock().unwrap();
                        res.sessions_failed += 1;
                        res.warnings.push(format!("파일 파싱 실패 [{}]: {}", file_path.display(), err));
                        *res.skip_reasons.entry("parse_error".to_string()).or_insert(0) += 1;
                        return;
                    }
                };

                // 2. cost_usd 계산 및 messages 채움
                let model_id_opt = parsed_session.session.model_id.as_deref().unwrap_or("unknown");
                let pricing_info = parsed_session.session.model_id.as_ref()
                    .and_then(|m_id| pricing_cache.get(m_id));

                // 미등록 모델 발견 시 경고 수집 및 fallback 정책 수행 (이슈 #683)
                if pricing_info.is_none() && model_id_opt != "unknown" {
                    let mut res = accumulator.lock().unwrap();
                    let warning_msg = format!(
                        "모델 단가 누락 경고: '{}' 모델의 단가 정보가 pricing 테이블에 없습니다. 기본 fallback 단가를 적용합니다.",
                        model_id_opt
                    );
                    if !res.warnings.contains(&warning_msg) {
                        res.warnings.push(warning_msg);
                    }
                }

                for msg in &mut parsed_session.messages {
                    if msg.role == "assistant" {
                        msg.cost_usd = pricing::calculate_cost_usd(
                            pricing_info,
                            msg.input_tokens,
                            msg.cache_read_input_tokens,
                            msg.output_tokens,
                        );
                    }
                }

                // 3. DB 적재 진행 (스레드별 개별 커넥션 확보)
                let conn = match db::init_db(&db_path_clone) {
                    Ok(c) => c,
                    Err(err) => {
                        let mut res = accumulator.lock().unwrap();
                        res.sessions_failed += 1;
                        res.warnings.push(format!("데이터베이스 연결 실패 [{}]: {}", file_path.display(), err));
                        *res.skip_reasons.entry("db_locked".to_string()).or_insert(0) += 1;
                        return;
                    }
                };

                // 멱등성 검사: 이미 존재하는 세션인지 확인
                let exists = match db::get_session(&conn, &parsed_session.session.session_id) {
                    Ok(opt) => opt.is_some(),
                    Err(_) => false,
                };

                if exists {
                    let mut res = accumulator.lock().unwrap();
                    res.sessions_skipped += 1;
                    *res.skip_reasons.entry("already_exists".to_string()).or_insert(0) += 1;
                    return;
                }

                // 4. 정규화 묶음 데이터 DB 적재
                let mut success = true;
                if let Err(err) = db::insert_session(&conn, &parsed_session.session) {
                    success = false;
                    let mut res = accumulator.lock().unwrap();
                    res.warnings.push(format!("세션 DB 적재 실패 [{}]: {}", file_path.display(), err));
                }

                if success {
                    for msg in &parsed_session.messages {
                        if let Err(err) = db::insert_message(&conn, msg) {
                            let mut res = accumulator.lock().unwrap();
                            res.warnings.push(format!("메시지 DB 적재 실패 [{}]: {}", file_path.display(), err));
                        }
                    }
                    for node in &parsed_session.nodes {
                        if let Err(err) = db::insert_node(&conn, node) {
                            let mut res = accumulator.lock().unwrap();
                            res.warnings.push(format!("노드 DB 적재 실패 [{}]: {}", file_path.display(), err));
                        }
                    }
                    for tc in &parsed_session.tool_calls {
                        if let Err(err) = db::insert_tool_call(&conn, tc) {
                            let mut res = accumulator.lock().unwrap();
                            res.warnings.push(format!("도구 호출 DB 적재 실패 [{}]: {}", file_path.display(), err));
                        }
                    }

                    let mut res = accumulator.lock().unwrap();
                    res.sessions_inserted += 1;
                } else {
                    let mut res = accumulator.lock().unwrap();
                    res.sessions_failed += 1;
                    *res.skip_reasons.entry("db_insert_error".to_string()).or_insert(0) += 1;
                }

                let mut res = accumulator.lock().unwrap();
                res.sessions_scanned += 1;
            });

            // 최종 결과 요약 출력
            let final_result = result_accumulator.lock().unwrap();
            println!("\n=== 스캔 결과 요약 ===");
            println!("총 발견된 파일 수: {}개", files_total);
            println!("성공적으로 적재된 세션 수: {}개", final_result.sessions_inserted);
            println!("스킵(중복 등)된 세션 수: {}개", final_result.sessions_skipped);
            println!("실패한 세션 수: {}개", final_result.sessions_failed);
            
            if !final_result.skip_reasons.is_empty() {
                println!("세션 스킵/실패 세부 사유:");
                for (reason, count) in &final_result.skip_reasons {
                    println!("  - {}: {}건", reason, count);
                }
            }

            if !final_result.warnings.is_empty() {
                println!("\n⚠️ 경고 및 오류 이력 (최대 10건):");
                let limit = final_result.warnings.len().min(10);
                for i in 0..limit {
                    println!("  - {}", final_result.warnings[i]);
                }
                if final_result.warnings.len() > 10 {
                    println!("  ... 그 외 {}건의 경고가 더 있습니다.", final_result.warnings.len() - 10);
                }
            }
        }
        Commands::Report {
            session_id,
            dimension,
            sort,
            limit,
            since,
        } => {
            let dim = dimension.as_deref().unwrap_or("session");
            let conn = match db::init_db(&db_path) {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("데이터베이스 연결 실패: {}", err);
                    std::process::exit(1);
                }
            };

            match dim {
                "session" => {
                    let report_list = match db::get_session_report(
                        &conn,
                        session_id.as_deref(),
                        since.as_deref(),
                        sort.as_deref(),
                        *limit,
                    ) {
                        Ok(list) => list,
                        Err(err) => {
                            eprintln!("세션 리포트 조회 실패: {}", err);
                            std::process::exit(1);
                        }
                    };

                    println!("\n============================================= 세션별 토큰/비용 집계 리포트 =============================================");
                    println!("| {:<20} | {:<10} | {:<20} | {:>10} | {:>10} | {:>12} | {:<20} |", 
                             "세션 ID", "에이전트", "모델 ID", "입력 토큰", "출력 토큰", "비용 (USD)", "시작 시간");
                    println!("-------------------------------------------------------------------------------------------------------------------------");
                    
                    let mut sum_input = 0;
                    let mut sum_output = 0;
                    let mut sum_cost = 0.0;

                    for r in &report_list {
                        let model_name = r.model_id.as_deref().unwrap_or("unknown");
                        println!("| {:<20} | {:<10} | {:<20} | {:>10} | {:>10} | ${:>11.6} | {:<20} |",
                                 r.session_id,
                                 r.agent_type,
                                 model_name,
                                 format_number(r.total_input_tokens),
                                 format_number(r.total_output_tokens),
                                 r.total_cost_usd,
                                 r.started_at);
                        sum_input += r.total_input_tokens;
                        sum_output += r.total_output_tokens;
                        sum_cost += r.total_cost_usd;
                    }
                    println!("-------------------------------------------------------------------------------------------------------------------------");
                    println!("| {:<20} | {:<10} | {:<20} | {:>10} | {:>10} | ${:>11.6} | {:<20} |",
                             "합계 (Summary)", "", "",
                             format_number(sum_input),
                             format_number(sum_output),
                             sum_cost,
                             "");
                    println!("=========================================================================================================================");
                }
                "agent" => {
                    let report_list = match db::get_agent_report(
                        &conn,
                        since.as_deref(),
                        sort.as_deref(),
                        *limit,
                    ) {
                        Ok(list) => list,
                        Err(err) => {
                            eprintln!("에이전트 리포트 조회 실패: {}", err);
                            std::process::exit(1);
                        }
                    };

                    println!("\n================================ 에이전트별 집계 리포트 ================================");
                    println!("| {:<10} | {:>10} | {:>12} | {:>12} | {:>14} |", 
                             "에이전트", "총 세션 수", "총 입력 토큰", "총 출력 토큰", "총 비용 (USD)");
                    println!("-----------------------------------------------------------------------------------------");
                    
                    let mut sum_sessions = 0;
                    let mut sum_input = 0;
                    let mut sum_output = 0;
                    let mut sum_cost = 0.0;

                    for r in &report_list {
                        println!("| {:<10} | {:>10} | {:>12} | {:>12} | ${:>13.6} |",
                                 r.agent_type,
                                 r.session_count,
                                 format_number(r.total_input_tokens),
                                 format_number(r.total_output_tokens),
                                 r.total_cost_usd);
                        sum_sessions += r.session_count;
                        sum_input += r.total_input_tokens;
                        sum_output += r.total_output_tokens;
                        sum_cost += r.total_cost_usd;
                    }
                    println!("-----------------------------------------------------------------------------------------");
                    println!("| {:<10} | {:>10} | {:>12} | {:>12} | ${:>13.6} |",
                             "합계",
                             sum_sessions,
                             format_number(sum_input),
                             format_number(sum_output),
                             sum_cost);
                    println!("=========================================================================================");
                }
                "tool" => {
                    let report_list = match db::get_tool_report(
                        &conn,
                        since.as_deref(),
                        sort.as_deref(),
                        *limit,
                    ) {
                        Ok(list) => list,
                        Err(err) => {
                            eprintln!("도구 리포트 조회 실패: {}", err);
                            std::process::exit(1);
                        }
                    };

                    println!("\n=================================== 도구별 호출/루프 집계 리포트 ===================================");
                    println!("| {:<30} | {:>10} | {:>10} | {:>12} | {:>10} |", 
                             "도구명", "총 호출 수", "성공 수", "루프 의심 수", "성공률 (%)");
                    println!("-----------------------------------------------------------------------------------------------------");
                    
                    let mut sum_calls = 0;
                    let mut sum_success = 0;
                    let mut sum_loops = 0;

                    for r in &report_list {
                        let success_rate = if r.call_count > 0 {
                            (r.success_count as f64) * 100.0 / (r.call_count as f64)
                        } else {
                            0.0
                        };

                        println!("| {:<30} | {:>10} | {:>10} | {:>12} | {:>9.1}% |",
                                 r.tool_name,
                                 format_number(r.call_count),
                                 format_number(r.success_count),
                                 format_number(r.loop_suspect_count),
                                 success_rate);
                        sum_calls += r.call_count;
                        sum_success += r.success_count;
                        sum_loops += r.loop_suspect_count;
                    }
                    println!("-----------------------------------------------------------------------------------------------------");
                    let total_rate = if sum_calls > 0 {
                        (sum_success as f64) * 100.0 / (sum_calls as f64)
                    } else {
                        0.0
                    };
                    println!("| {:<30} | {:>10} | {:>10} | {:>12} | {:>9.1}% |",
                             "합계",
                             format_number(sum_calls),
                             format_number(sum_success),
                             format_number(sum_loops),
                             total_rate);
                    println!("=====================================================================================================");
                }
                _ => {
                    eprintln!("잘못된 차원입니다. 지원되는 차원: session, agent, tool");
                    std::process::exit(1);
                }
            }
        }
        Commands::Loops {
            session_id,
            agent,
            signal,
            sort,
            max_calls,
            max_failures,
            inflation,
            ping_pong,
        } => {
            let conn = match db::init_db(&db_path) {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("데이터베이스 연결 실패: {}", err);
                    std::process::exit(1);
                }
            };

            // 1. DetectorConfig 생성
            let mut config = detect::loops::DetectorConfig::default();
            if let Some(mc) = max_calls {
                config.max_repeated_calls = *mc;
            }
            if let Some(mf) = max_failures {
                config.max_repeated_failures = *mf;
            }
            if let Some(inf) = inflation {
                config.token_inflation_threshold = *inf;
            }
            if let Some(pp) = ping_pong {
                config.max_ping_pong_cycles = *pp;
            }

            // 2. 전체 세션 정보 가져오기
            let all_sessions = match db::get_all_sessions(&conn) {
                Ok(list) => list,
                Err(err) => {
                    eprintln!("세션 정보 조회 실패: {}", err);
                    std::process::exit(1);
                }
            };

            let mut rows = Vec::new();
            let mut analyzed_sessions_count = 0;

            for sess in &all_sessions {
                // 세션 ID 필터
                if let Some(sid_filter) = session_id {
                    if sess.session_id != *sid_filter {
                        continue;
                    }
                }

                // 에이전트 필터
                if let Some(agent_filter) = agent {
                    if sess.agent_type != *agent_filter {
                        continue;
                    }
                }

                analyzed_sessions_count += 1;

                // 세션별 메시지 및 도구 호출 조회
                let messages = match db::get_messages_by_session(&conn, &sess.session_id) {
                    Ok(msgs) => msgs,
                    Err(err) => {
                        eprintln!("세션 [{}] 메시지 조회 실패: {}", sess.session_id, err);
                        continue;
                    }
                };

                let tool_calls = match db::get_tool_calls_by_session(&conn, &sess.session_id) {
                    Ok(tcs) => tcs,
                    Err(err) => {
                        eprintln!("세션 [{}] 도구 호출 조회 실패: {}", sess.session_id, err);
                        continue;
                    }
                };

                // 이상 징후 탐지
                let detect_res = detect::loops::detect_session_anomalies(sess, &messages, &tool_calls, &config);

                if detect_res.is_anomaly {
                    for sig in detect_res.signals {
                        // 시그널 타입 필터
                        if let Some(sig_filter) = signal {
                            if sig.signal_type != *sig_filter {
                                continue;
                            }
                        }

                        rows.push(LoopRow {
                            session_id: sess.session_id.clone(),
                            agent_type: sess.agent_type.clone(),
                            started_at: sess.started_at.clone(),
                            signal_type: sig.signal_type.clone(),
                            description: sig.description.clone(),
                            evidence: sig.evidence.clone(),
                        });
                    }
                }
            }

            // 3. 정렬 적용
            let sort_by = sort.as_deref().unwrap_or("started_at");
            match sort_by {
                "session_id" => rows.sort_by(|a, b| a.session_id.cmp(&b.session_id)),
                "agent_type" => rows.sort_by(|a, b| a.agent_type.cmp(&b.agent_type)),
                "started_at" | _ => rows.sort_by(|a, b| a.started_at.cmp(&b.started_at)),
            }

            // 4. 아스키 표 형태로 출력 (한국어 출력)
            println!("\n================================================ 에이전트 루프 및 오작동 탐지 리포트 ================================================");
            println!("| {:<20} | {:<12} | {:<18} | {:<55} | {:<30} |",
                     "세션 ID", "에이전트", "이상 유형", "상세 설명", "증거 정보");
            println!("-------------------------------------------------------------------------------------------------------------------------------------");

            for row in &rows {
                println!("| {:<20} | {:<12} | {:<18} | {:<55} | {:<30} |",
                         row.session_id,
                         row.agent_type,
                         row.signal_type,
                         row.description,
                         row.evidence);
            }
            println!("-------------------------------------------------------------------------------------------------------------------------------------");
            println!("| 총 분석 세션 수: {} 건 | 탐지된 이상 세션 수: {} 건 |",
                     analyzed_sessions_count,
                     rows.iter().map(|r| &r.session_id).collect::<std::collections::HashSet<_>>().len());
            println!("=====================================================================================================================================");
        }
    }
}

/// 경로를 순회하며 파일 목록을 재귀적으로 수집하는 헬퍼 함수
fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let child_path = entry.path();
            collect_files(&child_path, files)?;
        }
    }
    Ok(())
}

/// 천 단위 마커(콤마)를 포함하는 숫자 포맷팅 헬퍼 함수
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }
    result.chars().rev().collect()
}
