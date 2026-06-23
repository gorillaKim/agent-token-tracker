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

fn main() {
    let cli = Cli::parse();
    let db_path = cli.database.unwrap_or_else(|| "atk.db".to_string());

    println!("데이터베이스 파일: {}", db_path);

    // 데이터베이스 초기화 및 테이블/인덱스 마이그레이션 실행
    match db::init_db(&db_path) {
        Ok(_) => println!("데이터베이스 초기화 및 마이그레이션이 완료되었습니다."),
        Err(err) => {
            eprintln!("데이터베이스 초기화 실패: {}", err);
            std::process::exit(1);
        }
    }

    match &cli.command {
        Commands::Scan { path, agent } => {
            println!("스캔을 진행합니다. 대상 경로: {}", path);
            if let Some(agent_type) = agent {
                println!("필터링할 에이전트 타입: {}", agent_type);
            }
            // TODO: 스캔 수행 로직 호출
        }
        Commands::Report { session_id, detail } => {
            println!("리포트를 출력합니다.");
            if let Some(sid) = session_id {
                println!("세션 ID 필터: {}", sid);
            }
            println!("상세 모드: {}", detail);
            // TODO: 리포트 출력 로직 호출
        }
        Commands::Loops { threshold } => {
            println!("루프 탐지를 시작합니다.");
            if let Some(t) = threshold {
                println!("설정된 임계치: {}", t);
            }
            // TODO: 루프 탐지 로직 호출
        }
    }
}
