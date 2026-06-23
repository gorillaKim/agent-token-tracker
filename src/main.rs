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
        #[arg(short, long, help = "특정 세션 ID 필터")]
        session_id: Option<String>,

        #[arg(short, long, help = "상세 모드 활성화")]
        detail: bool,
    },
    #[command(about = "에이전트의 무한 루프 및 오작동 의심 세션을 탐지합니다.")]
    Loops {
        #[arg(short, long, help = "탐지 임계치 설정 (단계 수 또는 반복 횟수)")]
        threshold: Option<u64>,
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
                
                // 파일 확장자 판별 (디스패치)
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "jsonl" {
                    return; // jsonl 이외의 파일은 조용히 스킵
                }

                // 1. 어댑터를 통해 파싱 수행
                let adapter = ClaudeCodeAdapter;
                let parsed_session: NormalizedSession = match adapter.parse_session(file_path.to_str().unwrap()) {
                    Ok(sess) => sess,
                    Err(err) => {
                        let mut res = accumulator.lock().unwrap();
                        res.sessions_failed += 1;
                        res.warnings.push(format!("파일 파싱 실패 [{}]: {}", file_path.display(), err));
                        *res.skip_reasons.entry("parse_error".to_string()).or_insert(0) += 1;
                        return;
                    }
                };

                // 2. DB 적재 진행 (스레드별 개별 커넥션 확보)
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

                // 3. 정규화 묶음 데이터 DB 적재
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
        Commands::Report { session_id, detail } => {
            println!("리포트를 출력합니다.");
            if let Some(sid) = session_id {
                println!("세션 ID 필터: {}", sid);
            }
            println!("상세 모드: {}", detail);
        }
        Commands::Loops { threshold } => {
            println!("루프 탐지를 시작합니다.");
            if let Some(t) = threshold {
                println!("설정된 임계치: {}", t);
            }
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
