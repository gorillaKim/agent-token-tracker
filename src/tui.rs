//! Ratatui 기반 TUI 라이브 뷰 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 TUI 인터페이스를 설계하고 구현했습니다.
//! crossterm 및 ratatui를 활용해 실시간으로 에이전트 세션 목록 및 토큰 비용 요약,
//! 루프 오작동 감지 시그널을 관측할 수 있습니다.

use std::io;
use std::time::{Duration, Instant};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Tabs},
    Terminal,
};

/// 에이전트 TUI 상태를 관리하는 구조체
struct AppState {
    db_path: String,
    current_tab: usize,
    
    // 세션 정보 상태
    sessions: Vec<crate::model::Session>,
    session_list_state: ListState,
    selected_session_anomalies: Option<crate::detect::loops::LoopDetectionResult>,
    
    // 에이전트 요약 통계 상태
    agent_reports: Vec<crate::model::AgentReport>,
    
    // 도구 호출 요약 통계 상태
    tool_reports: Vec<crate::model::ToolReport>,
    
    // 선택된 세션의 상세 메시지 및 도구 호출 캐시
    selected_session_msgs: Vec<crate::model::Message>,
    selected_session_tcs: Vec<crate::model::ToolCall>,
}

impl AppState {
    fn new(db_path: &str) -> Self {
        let mut session_list_state = ListState::default();
        session_list_state.select(Some(0));
        Self {
            db_path: db_path.to_string(),
            current_tab: 0,
            sessions: Vec::new(),
            session_list_state,
            selected_session_anomalies: None,
            agent_reports: Vec::new(),
            tool_reports: Vec::new(),
            selected_session_msgs: Vec::new(),
            selected_session_tcs: Vec::new(),
        }
    }

    /// 데이터베이스에서 최신 데이터를 다시 조회하여 상태를 동기화합니다.
    fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let _ = conn.pragma_update(None, "busy_timeout", &3000);

        // 1. 전체 세션 목록 로드
        self.sessions = crate::db::get_all_sessions(&conn)?;
        // 시간 내림차순 정렬 (최신 세션이 위로 오도록)
        self.sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // 2. 에이전트별 요약 리포트 로드
        self.agent_reports = crate::db::get_agent_report(&conn, None, None, None)?;

        // 3. 도구별 요약 리포트 로드
        self.tool_reports = crate::db::get_tool_report(&conn, None, None, None)?;

        // 4. 선택된 세션의 상세 정보 갱신
        let selected_idx = self.session_list_state.selected().unwrap_or(0);
        if self.sessions.is_empty() {
            self.session_list_state.select(None);
            self.selected_session_anomalies = None;
            self.selected_session_msgs.clear();
            self.selected_session_tcs.clear();
        } else {
            let idx = selected_idx.min(self.sessions.len() - 1);
            self.session_list_state.select(Some(idx));
            
            let sess = &self.sessions[idx];
            self.selected_session_msgs = crate::db::get_messages_by_session(&conn, &sess.session_id)?;
            self.selected_session_tcs = crate::db::get_tool_calls_by_session(&conn, &sess.session_id)?;

            // 루프/오작동 분석기 연동
            let config = crate::detect::loops::DetectorConfig::default();
            let report = crate::detect::loops::detect_session_anomalies(
                sess,
                &self.selected_session_msgs,
                &self.selected_session_tcs,
                &config,
            );
            self.selected_session_anomalies = Some(report);
        }

        Ok(())
    }

    /// 목록 탐색 시 아래 방향으로 인덱스를 이동시킵니다.
    fn next(&mut self) {
        if self.current_tab == 0 && !self.sessions.is_empty() {
            let i = match self.session_list_state.selected() {
                Some(i) => {
                    if i >= self.sessions.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            self.session_list_state.select(Some(i));
            let _ = self.refresh(); // 세션이 바뀌면 상세 정보도 즉시 갱신
        }
    }

    /// 목록 탐색 시 위 방향으로 인덱스를 이동시킵니다.
    fn previous(&mut self) {
        if self.current_tab == 0 && !self.sessions.is_empty() {
            let i = match self.session_list_state.selected() {
                Some(i) => {
                    if i == 0 {
                        self.sessions.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            self.session_list_state.select(Some(i));
            let _ = self.refresh();
        }
    }
}

/// TUI 메인 이벤트 루프 구동 함수
pub fn run_tui(db_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. raw 모드 활성화 및 대체 화면 진입
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. 패닉 발생 시 raw 모드 정상 해제를 보장하는 패닉 훅 등록 (패닉 시 터미널 복구)
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, crossterm::cursor::Show);
        default_hook(panic_info);
    }));

    // 3. 상태 인스턴스 초기화 및 최초 조회
    let mut app = AppState::new(db_path);
    if let Err(err) = app.refresh() {
        // 데이터베이스 파일이 아예 존재하지 않는 경우 등 대비
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        return Err(format!("데이터 로드 실패: {}", err).into());
    }

    // 4. 타이머 틱 레이트 설정 (1초마다 자동 새로고침)
    let tick_rate = Duration::from_secs(1);
    let mut last_tick = Instant::now();

    loop {
        // UI 렌더링 호출
        terminal.draw(|f| draw_ui(f, &mut app))?;

        // 5. 키보드 이벤트 폴링 및 타임아웃 틱 설정
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        let _ = app.refresh();
                    }
                    KeyCode::Char('1') | KeyCode::F(1) => app.current_tab = 0,
                    KeyCode::Char('2') | KeyCode::F(2) => app.current_tab = 1,
                    KeyCode::Char('3') | KeyCode::F(3) => app.current_tab = 2,
                    KeyCode::Down => app.next(),
                    KeyCode::Up => app.previous(),
                    _ => {}
                }
            }
        }

        // 6. 주기적 틱 리프레시 수행
        if last_tick.elapsed() >= tick_rate {
            let _ = app.refresh();
            last_tick = Instant::now();
        }
    }

    // 7. 터미널 복구 후 종료
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    Ok(())
}

/// UI 전체 화면 렌더링 함수
fn draw_ui(f: &mut ratatui::Frame, app: &mut AppState) {
    // vertical 레이아웃 구분
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 탭 바
            Constraint::Min(0),    // 메인 내용 영역
            Constraint::Length(1), // 하단 단축키 가이드
        ])
        .split(f.size());

    // 1. 탭 바 렌더링 (한국어 탭)
    let tab_titles = vec!["[F1] 세션 목록 & 상세", "[F2] 에이전트별 요약", "[F3] 도구별 통계"];
    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).title(" Agent Token Tracker Live 관측 "))
        .select(app.current_tab)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // 2. 현재 선택된 탭 내용 렌더링
    match app.current_tab {
        0 => draw_session_tab(f, app, chunks[1]),
        1 => draw_agent_tab(f, app, chunks[1]),
        2 => draw_tool_tab(f, app, chunks[1]),
        _ => {}
    }

    // 3. 하단 도움말/단축키 바 렌더링
    let help_text = " Q: 종료 | R: 새로고침 | 1~3(F1~F3): 탭 전환 | ▲/▼: 세션 목록 이동 ";
    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(help_paragraph, chunks[2]);
}

/// [F1] 세션 목록 & 상세 탭 렌더링 함수
fn draw_session_tab(f: &mut ratatui::Frame, app: &mut AppState, area: ratatui::layout::Rect) {
    // 좌측(목록) 35%, 우측(상세) 65% 분할
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // 좌측 세션 리스트 데이터 매핑
    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .map(|s| {
            let title = s.session_name.as_deref().unwrap_or("무명 세션");
            // 루프 여부에 따라 색상 강조
            let mut style = Style::default();
            
            // 현재 루프 탐지 정보 매핑해 실시간 검사
            let conn_opt = rusqlite::Connection::open(&app.db_path).ok();
            let mut is_loop = false;
            if let Some(conn) = conn_opt {
                if let Ok(tcs) = crate::db::get_tool_calls_by_session(&conn, &s.session_id) {
                    if tcs.iter().any(|tc| tc.is_loop_suspect) {
                        is_loop = true;
                    }
                }
            }

            let badge = if is_loop {
                style = style.fg(Color::Red);
                "[이상] "
            } else {
                "[정상] "
            };

            let line = Line::from(vec![
                Span::styled(badge, style.add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}.. ", &s.session_id[..s.session_id.len().min(8)]), Style::default().fg(Color::DarkGray)),
                Span::raw(title),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list_title = format!(" 세션 목록 (총 {}건) ", app.sessions.len());
    let list_widget = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list_widget, main_chunks[0], &mut app.session_list_state);

    // 우측 세션 상세 및 루프 탐지 분석 화면 렌더링
    let selected_idx = app.session_list_state.selected();
    if let Some(idx) = selected_idx {
        if idx < app.sessions.len() {
            let sess = &app.sessions[idx];

            let mut text = Vec::new();
            text.push(Line::from(vec![
                Span::styled("세션 ID: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&sess.session_id),
            ]));
            text.push(Line::from(vec![
                Span::styled("세션명: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(sess.session_name.as_deref().unwrap_or("없음")),
            ]));
            text.push(Line::from(vec![
                Span::styled("에이전트 타입: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&sess.agent_type),
            ]));
            text.push(Line::from(vec![
                Span::styled("작업 경로 (CWD): ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&sess.cwd),
            ]));
            text.push(Line::from(vec![
                Span::styled("사용 모델: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(sess.model_id.as_deref().unwrap_or("unknown")),
            ]));
            text.push(Line::from(vec![
                Span::styled("토큰 수치 소스: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&sess.token_source),
            ]));
            text.push(Line::from(vec![
                Span::styled("총 입력 토큰: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format_number(sess.total_input_tokens)),
                Span::styled(" | 총 출력 토큰: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format_number(sess.total_output_tokens)),
            ]));
            
            // 총 비용 계산
            let mut total_cost = 0.0;
            for msg in &app.selected_session_msgs {
                total_cost += msg.cost_usd;
            }
            text.push(Line::from(vec![
                Span::styled("총 연산 비용: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("${:.6} USD", total_cost)),
            ]));
            text.push(Line::from(vec![
                Span::styled("시작 시각: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&sess.started_at),
                Span::styled(" | 종료 시각: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(sess.ended_at.as_deref().unwrap_or("진행중")),
            ]));
            text.push(Line::from(""));

            // 루프/오작동 탐지 리포트 렌더링
            text.push(Line::from(Span::styled(
                "--- [실시간 루프/오작동 이상 탐지 진단] ---",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));

            if let Some(ref report) = app.selected_session_anomalies {
                if report.is_anomaly {
                    text.push(Line::from(Span::styled(
                        "⚠️ 경고: 이 세션에서 오작동/이상 징후 시그널이 감지되었습니다!",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )));
                    for sig in &report.signals {
                        text.push(Line::from(vec![
                            Span::styled(format!("  • [{}] ", sig.signal_type), Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)),
                            Span::raw(&sig.description),
                        ]));
                        text.push(Line::from(Span::styled(
                            format!("    증거 데이터: {}", sig.evidence),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                } else {
                    text.push(Line::from(Span::styled(
                        "✅ 진단 정상: 감지된 루프나 이상 징후 오작동 패턴이 없습니다.",
                        Style::default().fg(Color::Green),
                    )));
                }
            } else {
                text.push(Line::from(Span::raw("진단 보고서를 수집하는 중...")));
            }

            let detail_widget = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL).title(" 세션 상세 정보 및 이상 진단 "))
                .wrap(ratatui::widgets::Wrap { trim: true });

            f.render_widget(detail_widget, main_chunks[1]);
        }
    } else {
        let empty_widget = Paragraph::new("선택된 세션이 없습니다.")
            .block(Block::default().borders(Borders::ALL).title(" 상세 정보 "));
        f.render_widget(empty_widget, main_chunks[1]);
    }
}

/// [F2] 에이전트별 요약 탭 렌더링 함수
fn draw_agent_tab(f: &mut ratatui::Frame, app: &AppState, area: ratatui::layout::Rect) {
    let header_cells = ["에이전트명", "총 세션 수", "총 입력 토큰", "총 출력 토큰", "총 비용 (USD)"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .agent_reports
        .iter()
        .map(|r| {
            let cells = vec![
                Cell::from(r.agent_type.clone()),
                Cell::from(r.session_count.to_string()),
                Cell::from(format_number(r.total_input_tokens)),
                Cell::from(format_number(r.total_output_tokens)),
                Cell::from(format!("${:.6}", r.total_cost_usd)),
            ];
            Row::new(cells).height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" 에이전트 요약 리포트 (실시간 집계) "));

    f.render_widget(table, area);
}

/// [F3] 도구별 통계 탭 렌더링 함수
fn draw_tool_tab(f: &mut ratatui::Frame, app: &AppState, area: ratatui::layout::Rect) {
    let header_cells = ["도구(Tool)명", "총 호출 횟수", "성공 횟수", "루프 의심 횟수", "성공률 (%)"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .tool_reports
        .iter()
        .map(|r| {
            let rate = if r.call_count > 0 {
                (r.success_count as f64) * 100.0 / (r.call_count as f64)
            } else {
                0.0
            };
            let cells = vec![
                Cell::from(r.tool_name.clone()),
                Cell::from(format_number(r.call_count)),
                Cell::from(format_number(r.success_count)),
                Cell::from(format_number(r.loop_suspect_count)),
                Cell::from(format!("{:.1}%", rate)),
            ];
            Row::new(cells).height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" 도구별 호출 및 이상 탐지 리포트 "));

    f.render_widget(table, area);
}

/// 천 단위 마커(콤마)를 추가하는 포맷팅 헬퍼 함수
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;
    for crate_char in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(crate_char);
        count += 1;
    }
    result.chars().rev().collect()
}
