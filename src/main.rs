//! Agent Token Tracker CLI 진입점
//!
//! 에이전트들의 활동 로그 및 토큰 사용량을 분석하고 시각화하는 도구입니다.

pub mod model;
pub mod db;
pub mod pricing;
pub mod adapters;
pub mod detect;
pub mod tui;
pub mod cross_check;

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use adapters::LogAdapter;
use adapters::claude_code::ClaudeCodeAdapter;
use adapters::codex::CodexAdapter;
use adapters::antigravity::AntigravityAdapter;

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

        #[arg(short, long, help = "파일 변경을 감시하여 실시간으로 증분 적재합니다.")]
        watch: bool,

        #[arg(long, help = "단일 파일에 대해 훅 트리거 모드로 멱등 적재합니다 (전체 스캔 스킵).")]
        hook: bool,
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
    #[command(about = "Ratatui 기반 TUI 라이브 뷰를 실행합니다.")]
    Tui,
    #[command(about = "FTS5를 통해 에이전트 대화 본문을 검색하고 컨텍스트를 출력합니다.")]
    Search {
        #[arg(help = "검색할 쿼리 문자열 (SQLite FTS5 매치 문법 지원)")]
        query: String,
    },
    #[command(about = "ATK ↔ ccusage 교차검증 하니스: ccusage JSON과 ATK DB 토큰 합계를 대조합니다.")]
    CrossCheck {
        #[arg(long, help = "ccusage session --json 출력 파일 경로 (미지정 시 stdin에서 읽음)")]
        ccusage_file: Option<String>,

        #[arg(long, help = "조회 시작일 필터 (예: 2026-06-23)")]
        since: Option<String>,

        #[arg(long, default_value = "5.0", help = "output 토큰 허용오차율 (%) [기본: 5.0]")]
        output_tolerance: f64,

        #[arg(long, default_value = "10.0", help = "비용 허용오차율 (%) [기본: 10.0]")]
        cost_tolerance: f64,

        #[arg(long, help = "결과를 JSON 형식으로 출력합니다.")]
        json: bool,
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
        Commands::Scan { path, agent, watch, hook } => {
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
            let db_write_lock = Arc::new(Mutex::new(()));

            if *hook {
                // 단일 훅 트리거 최적화 경로: 전체 수집을 스킵하고 단일 지정 경로 즉시 멱등 적재
                let file_path = Path::new(path);
                println!("[Hook Trigger] 단일 파일에 대한 즉시 멱등 적재를 구동합니다: {}", file_path.display());
                match process_single_file(
                    file_path,
                    agent.as_deref(),
                    &pricing_map_shared,
                    &db_path,
                    &db_write_lock,
                    true, // force_update
                ) {
                    Ok(_) => println!("[Hook Trigger] 성공적으로 적재 완료되었습니다."),
                    Err(err) => {
                        eprintln!("[Hook Trigger] 적재 실패: {}", err);
                        std::process::exit(1);
                    }
                }
            } else {
                // 일반 스캔 모드 또는 파일 감시 모드
                let mut files = Vec::new();
                if let Err(err) = collect_files(Path::new(path), &mut files) {
                    eprintln!("파일 목록 수집 중 오류 발생: {}", err);
                    std::process::exit(1);
                }

                let files_total = files.len();
                println!("총 {}개의 대상 파일을 발견했습니다.", files_total);

                let result_accumulator = Arc::new(Mutex::new(ScanResult::default()));
                let db_path_clone = db_path.clone();

                // Rayon을 활용한 병렬 스캔 처리
                files.par_iter().for_each(|file_path| {
                    let accumulator = Arc::clone(&result_accumulator);
                    let pricing_cache = Arc::clone(&pricing_map_shared);
                    let db_write_lock = Arc::clone(&db_write_lock);

                    match process_single_file(
                        file_path,
                        agent.as_deref(),
                        &pricing_cache,
                        &db_path_clone,
                        &db_write_lock,
                        false, // force_update (일반 최초 스캔은 기존 데이터 중복 시 스킵)
                    ) {
                        Ok(_) => {
                            let mut res = accumulator.lock().unwrap();
                            res.sessions_inserted += 1;
                            res.sessions_scanned += 1;
                        }
                        Err(err) => {
                            let mut res = accumulator.lock().unwrap();
                            let err_str = err.to_string();
                            if err_str.contains("already_exists") {
                                res.sessions_skipped += 1;
                                *res.skip_reasons.entry("already_exists".to_string()).or_insert(0) += 1;
                            } else if err_str.contains("database is locked") || err_str.contains("db_locked") || err_str.contains("busy") {
                                res.sessions_skipped += 1;
                                res.warnings.push(format!("DB 잠금 스킵 [{}]: {}", file_path.display(), err));
                                *res.skip_reasons.entry("db_locked".to_string()).or_insert(0) += 1;
                            } else if err_str.contains("permission denied") || err_str.contains("PermissionDenied") || err_str.contains("permission_denied") {
                                res.sessions_skipped += 1;
                                res.warnings.push(format!("권한 오류 스킵 [{}]: {}", file_path.display(), err));
                                *res.skip_reasons.entry("permission_denied".to_string()).or_insert(0) += 1;
                            } else {
                                res.sessions_failed += 1;
                                res.warnings.push(format!("파일 파싱 실패 [{}]: {}", file_path.display(), err));
                                *res.skip_reasons.entry("parse_error".to_string()).or_insert(0) += 1;
                            }
                            res.sessions_scanned += 1;
                        }
                    }
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

                // 파일 감시 모드가 활성화된 경우
                if *watch {
                    use notify::{Watcher, RecursiveMode};
                    use std::sync::mpsc::channel;

                    let (tx, rx) = channel();
                    let mut watcher = match notify::recommended_watcher(move |res| {
                        if let Ok(event) = res {
                            let _ = tx.send(event);
                        }
                    }) {
                        Ok(w) => w,
                        Err(err) => {
                            eprintln!("파일 감시자 생성 실패: {}", err);
                            std::process::exit(1);
                        }
                    };

                    let watch_path = Path::new(path);
                    let target_dir = if watch_path.is_file() {
                        watch_path.parent().unwrap_or(watch_path)
                    } else {
                        watch_path
                    };

                    if let Err(err) = watcher.watch(target_dir, RecursiveMode::Recursive) {
                        eprintln!("파일 감시 시작 실패 [{}]: {}", target_dir.display(), err);
                        std::process::exit(1);
                    }

                    println!("\n[Watch] 파일 감시 모드가 활성화되었습니다 (감시 경로: {}).", target_dir.display());
                    println!("[Watch] 파일 변경 감지 시 500ms 디바운스 후 실시간 멱등 증분 적재를 실행합니다. (Q/Ctrl+C로 종료)");

                    let mut last_event_time = Instant::now();
                    let mut pending_files = std::collections::HashSet::new();

                    loop {
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(event) => {
                                for p in event.paths {
                                    if p.is_file() {
                                        pending_files.insert(p);
                                    }
                                }
                                last_event_time = Instant::now();
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                if !pending_files.is_empty() && last_event_time.elapsed() >= Duration::from_millis(500) {
                                    println!("\n[Watch] 변경 사항 감지! 증분 적재를 시작합니다...");
                                    for file in pending_files.drain() {
                                        let path_str = file.to_str().unwrap_or("");
                                        let is_vscdb = path_str.contains("state.vscdb");
                                        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
                                        
                                        if ext != "jsonl" && !is_vscdb {
                                            continue; // 유효 파일만
                                        }

                                        if is_vscdb {
                                            // state.vscdb 변경 시 세션 ID 목록을 새로 뽑아 가상 경로로 변형해 순차 처리
                                            match adapters::antigravity::get_vscdb_session_ids(path_str) {
                                                Ok(ids) => {
                                                    for id in ids {
                                                        let virtual_path_str = format!("{}?session_id={}", path_str, id);
                                                        let virtual_path = PathBuf::from(virtual_path_str);
                                                        match process_single_file(
                                                            &virtual_path,
                                                            agent.as_deref(),
                                                            &pricing_map_shared,
                                                            &db_path,
                                                            &db_write_lock,
                                                            true, // force_update
                                                        ) {
                                                            Ok(_) => println!("  - 성공적으로 적재(vscdb 세션): {}", id),
                                                            Err(err) => eprintln!("  - 적재 실패 [vscdb 세션 {}]: {}", id, err),
                                                        }
                                                    }
                                                }
                                                Err(err) => eprintln!("  - vscdb 세션 ID 목록 갱신 실패: {:?}", err),
                                            }
                                        } else {
                                            match process_single_file(
                                                &file,
                                                agent.as_deref(),
                                                &pricing_map_shared,
                                                &db_path,
                                                &db_write_lock,
                                                true, // force_update
                                            ) {
                                                Ok(_) => println!("  - 성공적으로 적재: {}", file.display()),
                                                Err(err) => eprintln!("  - 적재 실패 [{}]: {}", file.display(), err),
                                            }
                                        }
                                    }
                                    println!("[Watch] 증분 적재가 완료되었습니다. 대기 중...");
                                }
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                eprintln!("감시 채널이 끊어졌습니다.");
                                break;
                            }
                        }
                    }
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
        Commands::Tui => {
            if let Err(err) = tui::run_tui(&db_path) {
                eprintln!("TUI 실행 오류: {}", err);
                std::process::exit(1);
            }
        }
        Commands::Search { query } => {
            #[cfg(feature = "fts")]
            {
                let conn = match db::init_db(&db_path) {
                    Ok(c) => c,
                    Err(err) => {
                        eprintln!("데이터베이스 연결 실패: {}", err);
                        std::process::exit(1);
                    }
                };

                match db::search_messages(&conn, query) {
                    Ok(results) => {
                        println!("\n================================================ 대화 본문 검색 결과 ================================================");
                        println!("검색 쿼리: '{}'", query);
                        println!("검색된 메시지 수: {} 건", results.len());
                        println!("=====================================================================================================================");

                        for r in &results {
                            let total_tokens = r.total_input_tokens + r.total_output_tokens;
                            println!("\n[세션 ID] {}", r.session_id);
                            println!("  - 모델 ID: {} | 시작 시간: {}", r.model_id.as_deref().unwrap_or("unknown"), r.started_at);
                            println!("  - 전체 토큰: {} (In: {}, Out: {}) | 세션 비용: ${:.6}", format_number(total_tokens), format_number(r.total_input_tokens), format_number(r.total_output_tokens), r.cost_usd);
                            println!("  - 역할: {} (Turn Index: {})", r.role, r.turn_index);
                            println!("  - 메시지 내용:");
                            for line in r.content.lines() {
                                println!("    > {}", line);
                            }
                            println!("---------------------------------------------------------------------------------------------------------------------");
                        }
                    }
                    Err(err) => {
                        eprintln!("대화 검색 실패: {}", err);
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(feature = "fts"))]
            {
                let _ = query;
                eprintln!("에러: 대화 검색 기능(FTS5)은 빌드 시 컴파일되지 않았습니다.");
                eprintln!("도움을 받으려면 '--features fts' 플래그를 사용하여 빌드하여 주십시오.");
                eprintln!("예시: cargo run --features fts -- search \"<query>\"");
                std::process::exit(1);
            }
        }
        Commands::CrossCheck { ccusage_file, since, output_tolerance, cost_tolerance, json } => {
            // 1. ccusage JSON 읽기 (파일 또는 stdin)
            let raw = if let Some(path) = ccusage_file {
                match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("ccusage 파일 읽기 실패: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // stdin에서 ccusage JSON 읽기
                use std::io::Read;
                let mut buf = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                    eprintln!("stdin 읽기 실패: {}", e);
                    std::process::exit(1);
                }
                buf
            };

            // 2. ccusage JSON 파싱
            let ccusage_sessions = match cross_check::parse_ccusage_json(&raw) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("ccusage JSON 파싱 실패: {}", e);
                    eprintln!("힌트: `npx ccusage session --json` 출력을 파이프로 전달하거나 --ccusage-file 옵션을 사용하세요.");
                    std::process::exit(1);
                }
            };

            // 3. ATK DB에서 세션 집계 조회
            let conn = match db::init_db(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("DB 연결 실패: {}", e);
                    std::process::exit(1);
                }
            };

            let atk_sessions = match cross_check::get_atk_session_agg(&conn, since.as_deref()) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("ATK DB 조회 실패: {}", e);
                    std::process::exit(1);
                }
            };

            // 4. 교차검증 수행
            let tolerance = cross_check::Tolerance {
                output_token_rate: *output_tolerance / 100.0,
                cost_rate: *cost_tolerance / 100.0,
            };
            let report = cross_check::cross_check(&atk_sessions, &ccusage_sessions, &tolerance);

            // 5. 결과 출력
            if *json {
                match serde_json::to_string_pretty(&report) {
                    Ok(s) => println!("{}", s),
                    Err(e) => eprintln!("JSON 직렬화 실패: {}", e),
                }
            } else {
                cross_check::print_report(&report, &tolerance);
            }

            // 6. 허용오차 초과 시 비정상 종료 코드
            if !report.output_within_tolerance || !report.cost_within_tolerance {
                std::process::exit(2);
            }
        }
    }
}

/// 경로를 순회하며 파일 목록을 재귀적으로 수집하는 헬퍼 함수
fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        let path_str = path.to_str().unwrap_or("");
        if path_str.ends_with("state.vscdb") {
            // state.vscdb 내의 세션 ID 목록을 조회해 가상 경로로 변형해 인입
            match adapters::antigravity::get_vscdb_session_ids(path_str) {
                Ok(ids) => {
                    for id in ids {
                        let virtual_path = format!("{}?session_id={}", path_str, id);
                        files.push(PathBuf::from(virtual_path));
                    }
                }
                Err(err) => {
                    eprintln!("Error loading vscdb session IDs: {:?}", err);
                    files.push(path.to_path_buf());
                }
            }
        } else {
            files.push(path.to_path_buf());
        }
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

/// 단일 파일에 대해 파싱 및 데이터베이스 멱등 적재를 처리합니다.
fn process_single_file(
    file_path: &Path,
    agent_filter: Option<&str>,
    pricing_cache: &Arc<HashMap<String, crate::model::Pricing>>,
    db_path: &str,
    db_write_lock: &Arc<Mutex<()>>,
    force_update: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = file_path.to_str().unwrap_or("");
    let is_vscdb = path_str.contains("state.vscdb");
    
    // vscdb의 가상 경로는 실제 파일 경로가 아닌 state.vscdb?session_id=... 형식임
    let has_vscdb_param = path_str.contains("state.vscdb?session_id=");

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "jsonl" && !is_vscdb && !has_vscdb_param {
        return Ok(());
    }

    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let is_codex = file_name.starts_with("rollout-") || file_name.contains("codex") || agent_filter == Some("codex");
    let is_antigravity = is_vscdb || has_vscdb_param || file_name.contains("antigravity") || agent_filter == Some("antigravity");

    let parsed_res = if is_antigravity {
        let adapter = AntigravityAdapter;
        adapter.parse_session(path_str)
    } else if is_codex {
        let adapter = CodexAdapter;
        adapter.parse_session(path_str)
    } else {
        let adapter = ClaudeCodeAdapter;
        adapter.parse_session(path_str)
    };

    let mut parsed_session = parsed_res?;

    // cost_usd 계산 및 messages 채움
    let model_id_opt = parsed_session.session.model_id.as_deref().unwrap_or("unknown");
    let pricing_info = parsed_session.session.model_id.as_ref()
        .and_then(|m_id| pricing_cache.get(m_id));

    // 미등록 모델 발견 시 경고
    if pricing_info.is_none() && model_id_opt != "unknown" {
        eprintln!(
            "모델 단가 누락 경고: '{}' 모델의 단가 정보가 pricing 테이블에 없습니다. 기본 fallback 단가를 적용합니다.",
            model_id_opt
        );
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

    // DB 적재 진행
    let _write_guard = db_write_lock.lock().unwrap();
    let conn = rusqlite::Connection::open(db_path)?;
    let _ = conn.pragma_update(None, "foreign_keys", "ON");
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "busy_timeout", &5000);

    let exists = match db::get_session(&conn, &parsed_session.session.session_id)? {
        Some(_) => true,
        None => false,
    };

    if exists {
        if force_update {
            // watch / hook 모드 시 멱등성을 위해 기존 데이터 완전 삭제
            db::delete_session(&conn, &parsed_session.session.session_id)?;
        } else {
            // 일반 최초 스캔 시엔 중복 적재 방지를 위해 에러 리턴으로 건너뜀
            return Err("already_exists".into());
        }
    }

    // 정규화 데이터 DB 적재
    db::insert_session(&conn, &parsed_session.session)?;
    for msg in &parsed_session.messages {
        db::insert_message(&conn, msg)?;
    }
    for node in &parsed_session.nodes {
        db::insert_node(&conn, node)?;
    }
    for tc in &parsed_session.tool_calls {
        db::insert_tool_call(&conn, tc)?;
    }

    Ok(())
}

