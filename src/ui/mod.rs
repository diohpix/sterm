use anyhow::Result;
use slint::{Model, ModelRc, VecModel, Weak};
use std::sync::mpsc;
use std::sync::Arc;
// Duration import removed - no longer using timers
use tokio::sync::Mutex;

use crate::terminal::{SessionId, TerminalManager};
use crate::utils::font::FontMetrics;
use crate::{ColorSegment, CursorInfo, MainWindow, TerminalKeyEvent};

/// 터미널로 전달하기에 안전한 키 입력인지 확인하고 필요시 변환  
fn process_and_filter_terminal_input(event: &TerminalKeyEvent) -> Option<String> {
    let input = &event.text.to_string();
    if input.is_empty() {
        log::debug!("Filtered: empty input");
        return None;
    }
    
    // 완전히 공백으로만 구성된 문자열 필터링
    
    
    // 단일 문자인 경우
    if input.len() == 1 {
        let ch = input.chars().next().unwrap();
        match ch {
            // 일반적인 출력 가능한 ASCII 문자들
            ' '..='~' => Some(input.to_string()),
            // 허용할 제어 문자들
            '\n' | '\r' | '\t' | '\u{08}' => Some(input.to_string()), // Enter, CR, Tab, Backspace
            // Ctrl+L (clear screen) 허용
            '\u{0c}' => {
                log::debug!("Clear screen command detected (Ctrl+L)");
                Some(input.to_string())
            }
            // 나머지 제어 문자들은 필터링
            '\u{00}'..='\u{1f}' | '\u{7f}' => {
                log::debug!("Filtered control character: {:?} (\\u{{{:04x}}})", ch, ch as u32);
                None
            }
            // macOS 특수 키 범위 필터링
            '\u{f700}'..='\u{f8ff}' => {
                log::debug!("Filtered macOS special key: {:?} (\\u{{{:04x}}})", ch, ch as u32);
                None
            }
            // 기타 유니코드 문자들은 허용 (다국어 입력 지원)
            _ => Some(input.to_string()),
        }
    } else {
        // 멀티바이트 문자열의 경우
        
        // macOS 특수 키들이 포함된 경우 필터링
        if input.chars().any(|c| matches!(c, '\u{f700}'..='\u{f8ff}')) {
            log::debug!("Filtered macOS special key sequence: {:?}", input);
            return None;
        }
        
        // escape sequence 필터링
        if input.starts_with('\u{1b}') {
            log::debug!("Filtered escape sequence: {:?}", input);
            return None;
        }
        
        // 대부분 제어 문자로만 구성된 경우 필터링
        if input.chars().all(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}')) {
            log::debug!("Filtered control sequence: {:?}", input);
            return None;
        }
        
        // 일반적인 멀티바이트 문자열 허용 (유니코드, 복합 입력 등)
        Some(input.to_string())
    }
}



// UI 업데이트 메시지 타입
#[derive(Debug, Clone)]
pub enum UIUpdateMessage {
    ColoredTerminalContent {
        session_id: SessionId,
        segments: Vec<crate::terminal::ColoredTextSegment>,
    },
    SessionClosed {
        session_id: SessionId,
    },
}

pub struct UIManager {
    window: Weak<MainWindow>,
    terminal_manager: Arc<Mutex<TerminalManager>>,
    ui_update_sender: mpsc::Sender<UIUpdateMessage>,
    ui_update_receiver: Option<mpsc::Receiver<UIUpdateMessage>>,
}

impl UIManager {
    pub fn new(
        window: Weak<MainWindow>,
        terminal_manager: Arc<Mutex<TerminalManager>>,
    ) -> Result<Self> {
        let (ui_update_sender, ui_update_receiver) = mpsc::channel();
        Ok(Self {
            window,
            terminal_manager,
            ui_update_sender,
            ui_update_receiver: Some(ui_update_receiver),
        })
    }

    /// 색상 세그먼트들을 렌더링 가능한 텍스트로 변환
    fn render_colored_segments(segments: &[crate::terminal::ColoredTextSegment]) -> String {
        // TODO: 실제 색상 렌더링 구현
        // 현재는 텍스트만 연결하여 반환 (색상 정보는 로그에 기록됨)
        let mut result = String::new();
        for segment in segments {
            result.push_str(&segment.text);
        }
        result
    }

    pub async fn setup_event_handlers(&mut self) -> Result<()> {
        let window = self
            .window
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Failed to upgrade window weak reference"))?;

        // 탭 클릭 이벤트 핸들러
        {
            let terminal_manager = self.terminal_manager.clone();
            let window_weak = self.window.clone();

            window.on_tab_clicked(move |tab_id| {
                let terminal_manager = terminal_manager.clone();
                let window_weak = window_weak.clone();

                // 메인 스레드에서 비동기 작업 실행
                slint::invoke_from_event_loop(move || {
                    let terminal_manager = terminal_manager.clone();
                    let window_weak = window_weak.clone();

                    tokio::spawn(async move {
                        let mut tm = terminal_manager.lock().await;
                        if let Err(e) = tm.set_active_session(tab_id as SessionId) {
                            log::error!("Failed to set active session: {}", e);
                            return;
                        }
                        drop(tm);

                        // UI 업데이트는 다시 메인 스레드로
                        slint::invoke_from_event_loop(move || {
                            if let Some(window) = window_weak.upgrade() {
                                window.set_active_tab(tab_id);
                                // 터미널 내용 업데이트는 타이머로 처리됨
                            }
                        })
                        .unwrap_or_else(|e| log::error!("Failed to invoke UI update: {:?}", e));
                    });
                })
                .unwrap_or_else(|e| log::error!("Failed to invoke from event loop: {:?}", e));
            });
        }

        // 새 탭 생성 이벤트 핸들러
        {
            let terminal_manager = self.terminal_manager.clone();
            let window_weak = self.window.clone();

            window.on_new_tab_clicked(move || {
                let terminal_manager = terminal_manager.clone();
                let window_weak = window_weak.clone();

                slint::invoke_from_event_loop(move || {
                    tokio::spawn(async move {
                        let mut tm = terminal_manager.lock().await;
                        match tm.create_new_session() {
                            Ok(session_id) => {
                                // UI 업데이트
                                slint::invoke_from_event_loop(move || {
                                    if let Some(window) = window_weak.upgrade() {
                                        Self::add_tab_to_ui(
                                            &window,
                                            session_id,
                                            &format!("Terminal {}", session_id + 1),
                                        );
                                        window.set_active_tab(session_id as i32);
                                    }
                                })
                                .unwrap_or_else(|e| {
                                    log::error!("Failed to invoke UI update: {:?}", e)
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to create new session: {}", e);
                            }
                        }
                    });
                })
                .unwrap_or_else(|e| log::error!("Failed to invoke from event loop: {:?}", e));
            });
        }

        // 탭 닫기 이벤트 핸들러
        {
            let terminal_manager = self.terminal_manager.clone();
            let window_weak = self.window.clone();

            window.on_close_tab_clicked(move |tab_id| {
                let terminal_manager = terminal_manager.clone();
                let window_weak = window_weak.clone();

                slint::invoke_from_event_loop(move || {
                    tokio::spawn(async move {
                        let mut tm = terminal_manager.lock().await;
                        if let Err(e) = tm.close_session(tab_id as SessionId).await {
                            log::error!("Failed to close session: {}", e);
                            return;
                        }

                        // UI 업데이트
                        slint::invoke_from_event_loop(move || {
                            if let Some(window) = window_weak.upgrade() {
                                Self::remove_tab_from_ui(&window, tab_id as SessionId);
                            }
                        })
                        .unwrap_or_else(|e| log::error!("Failed to invoke UI update: {:?}", e));
                    });
                })
                .unwrap_or_else(|e| log::error!("Failed to invoke from event loop: {:?}", e));
            });
        }

        // 터미널 입력 이벤트 핸들러
        {
            let terminal_manager = self.terminal_manager.clone();

            window.on_terminal_input(move |event| {
                let terminal_manager = terminal_manager.clone();
                log::debug!("Received terminal input event: text={:?}, modifiers={{alt:{}, ctrl:{}, meta:{}, shift:{}}}, repeat:{}", 
                    event.text, event.modifiers.alt, event.modifiers.control, event.modifiers.meta, event.modifiers.shift, event.repeat);

                // 키 입력 필터링 - 안전한 입력만 터미널로 전달
                let filtered_input = match process_and_filter_terminal_input(&event) {
                    Some(processed) => processed,
                    None => {
                        log::debug!("Filtered unsafe terminal input: {:?}", event.text);
                        return;
                    }
                };

                // 별도 스레드 없이 바로 PTY에 쓰기
                if let Ok(tm) = terminal_manager.try_lock() {
                    if let Some(active_session) = tm.get_active_session() {
                        let session_id = active_session.id;
                        if let Err(e) = tm.write_to_session(session_id, &filtered_input) {
                            log::error!("Failed to write to terminal: {}", e);
                        }
                    }
                } else {
                    log::warn!("Could not acquire terminal manager lock for input: {:?}", event.text);
                };
            });
        }



        // 윈도우 리사이즈 이벤트 핸들러
        {
            let terminal_manager = self.terminal_manager.clone();

            window.on_window_resized(move |width, height| {
                let terminal_manager = terminal_manager.clone();

                slint::invoke_from_event_loop(move || {
                    tokio::spawn(async move {
                        let mut tm = terminal_manager.lock().await;
                        if let Some(active_session) = tm.get_active_session() {
                            // 터미널 크기를 문자 단위로 계산 (폰트 크기 기반)
                            let char_width = 8; // 고정 폭 폰트 가정
                            let char_height = 16; // 고정 높이 폰트 가정
                            let cols = (width / char_width) as u16;
                            let rows = (height / char_height) as u16;

                            let session_id = active_session.id;
                            if let Err(e) = tm.resize_session(session_id, cols, rows) {
                                log::error!("Failed to resize terminal: {}", e);
                            }
                        }
                    });
                })
                .unwrap_or_else(|e| log::error!("Failed to invoke from event loop: {:?}", e));
            });
        }

        // 클립보드 복사 이벤트 핸들러
        {
            window.on_copy_selected(move || {
                slint::invoke_from_event_loop(move || {
                    tokio::spawn(async move {
                        // 선택된 텍스트 가져오기 (현재는 플레이스홀더)
                        let selected_text = "Selected terminal text"; // TODO: 실제 선택된 텍스트

                        // 클립보드에 복사
                        match crate::utils::platform::Platform::copy_to_clipboard(selected_text) {
                            Ok(_) => log::info!("Text copied to clipboard"),
                            Err(e) => log::error!("Failed to copy to clipboard: {}", e),
                        }
                    });
                })
                .unwrap_or_else(|e| log::error!("Failed to invoke from event loop: {:?}", e));
            });
        }

        // 클립보드 붙여넣기 이벤트 핸들러
        {
            let terminal_manager = self.terminal_manager.clone();

            window.on_paste_clipboard(move || {
                let terminal_manager = terminal_manager.clone();

                slint::invoke_from_event_loop(move || {
                    tokio::spawn(async move {
                        // 클립보드에서 텍스트 가져오기
                        match crate::utils::platform::Platform::paste_from_clipboard() {
                            Ok(text) => {
                                let mut tm = terminal_manager.lock().await;
                                if let Some(active_session) = tm.get_active_session() {
                                    let session_id = active_session.id;
                                    if let Err(e) = tm.write_to_session(session_id, &text) {
                                        log::error!("Failed to paste text: {}", e);
                                    } else {
                                        log::info!("Pasted text from clipboard");
                                    }
                                }
                            }
                            Err(e) => log::error!("Failed to paste from clipboard: {}", e),
                        }
                    });
                })
                .unwrap_or_else(|e| log::error!("Failed to invoke from event loop: {:?}", e));
            });
        }

        // 초기 탭 설정
        self.setup_initial_tabs(&window).await?;

        // PTY 이벤트 처리 스레드 시작 (tterm 방식)
        self.start_pty_event_processing().await?;

        // UI 업데이트 처리 스레드 시작
        //self.start_ui_update_processing()?;

        Ok(())
    }

    async fn start_pty_event_processing(&self) -> Result<()> {
        let terminal_manager = self.terminal_manager.clone();
        let ui_update_sender = self.ui_update_sender.clone();
        let window_weak = self.window.clone();
        // TerminalManager로부터 이벤트 수신기 가져오기
        let event_receiver = {
            let mut tm = terminal_manager.lock().await;
            tm.take_pty_event_receiver()
        };

        if let Some(receiver) = event_receiver {
            std::thread::Builder::new()
                .name("pty_event_processor".to_string())
                .spawn(move || {
                    log::info!("Starting PTY event processor thread");

                    // 이벤트 처리 루프
                    loop {
                        match receiver.recv() {
                            Ok((session_id, event)) => {
                                log::debug!("Received PTY event for session {}: {:?}", session_id, event);

                                match &event {
                                    // PTY 출력이나 터미널 상태 변경 시 UI 업데이트
                                    alacritty_terminal::event::Event::Wakeup  => {
                                        // Wakeup이나 Title 변경 시에도 터미널 내용 업데이트
                                        if let Ok(mut tm) = terminal_manager.try_lock() {
                                            log::debug!("Terminal content updated on {:?} for session {}:", event, session_id);
                                                                                        // 색상 정보 추출 및 UI로 전송 - 폰트 메트릭 사용
                                            let font_metrics = FontMetrics::default(); // 임시로 기본값 사용
                                            if let Some(colored_content) = tm.extract_session_colored_content(session_id, &font_metrics) {
                                                log::debug!("Color segments for session {} ({}): {} segments", session_id, match &event { alacritty_terminal::event::Event::Wakeup => "Wakeup", alacritty_terminal::event::Event::Title(_) => "Title", _ => "Other" }, colored_content.segments.len());
                                                if colored_content.segments.len() > 0 {
                                                    for (i, segment) in colored_content.segments.iter().take(5).enumerate() {
                                                        log::debug!("  Segment {}: '{}' x={} y={} w={} h={}", i, segment.text.chars().take(20).collect::<String>(), segment.x, segment.y, segment.width, segment.height);
                                                    }
                                                    let cursor_info =  {
                                                        let font_metrics = FontMetrics::default(); // 임시로 기본값 사용
                                                        {
                                                            let cursor_x = font_metrics.padding_x + (colored_content.cursor_col as i32) * font_metrics.char_width;
                                                            let cursor_y = font_metrics.padding_y + (colored_content.cursor_line as i32) * font_metrics.line_height;
                                                            CursorInfo {
                                                                x: cursor_x,
                                                                y: cursor_y,
                                                                width: font_metrics.char_width,
                                                                height: font_metrics.line_height,
                                                                visible: true,
                                                            }
                                                        }
                                                    } ;
                                                    let slint_segments: Vec<ColorSegment> = colored_content.segments.iter().map(|seg| {
                                                        ColorSegment {
                                                            text: seg.text.clone().into(),
                                                            fg_r: seg.fg_color.r as i32,
                                                            fg_g: seg.fg_color.g as i32,
                                                            fg_b: seg.fg_color.b as i32,
                                                            bg_r: seg.bg_color.r as i32,
                                                            bg_g: seg.bg_color.g as i32,
                                                            bg_b: seg.bg_color.b as i32,
                                                            x: seg.x,      // 이미 계산된 절대 X 위치
                                                            y: seg.y,      // 이미 계산된 절대 Y 위치
                                                            width: seg.width,  // 이미 계산된 폭
                                                            height: seg.height, // 이미 계산된 높이
                                                        }
                                                    }).collect();
                                                    let window_weak = window_weak.clone();
                                                    slint::invoke_from_event_loop(move || {
                                                        if let Some(window) = window_weak.upgrade() {
                                                            // 색상 세그먼트 설정
                                                            let model = ModelRc::new(VecModel::from(slint_segments));
                                                            window.set_color_segments(model);
                                                            window.set_cursor_info(cursor_info);

                                                        }
                                                    }).unwrap_or_else(|e|
                                                        log::error!("Failed to invoke colored UI update: {:?}", e));
                                                    }
                                            }
                                        }
                                    }
                                    alacritty_terminal::event::Event::Exit => {
                                        log::info!("Terminal session {} exited", session_id);
                                        // 세션 종료 메시지 전송
                                        if let Err(e) = ui_update_sender.send(UIUpdateMessage::SessionClosed { session_id }) {
                                            log::error!("Failed to send session closed message: {}", e);
                                        }
                                        break;
                                    }
                                    _ => {
                                        // 다른 이벤트들은 무시
                                        println!("other event: {:?}", event);
                                    }
                                }
                            }
                            Err(_) => {
                                log::warn!("PTY event receiver channel closed");
                                break;
                            }
                        }
                    }

                    log::info!("PTY event processor thread ended");
                })?;
        }

        Ok(())
    }

    async fn setup_initial_tabs(&self, window: &MainWindow) -> Result<()> {
        // 초기 탭 데이터 설정
        let initial_tabs = vec![crate::TabInfo {
            title: "Terminal 1".into(),
            active: true,
            id: 0,
        }];

        let tabs_model = VecModel::from(initial_tabs);
        window.set_tabs(ModelRc::new(tabs_model));

        // 초기 터미널 내용 설정 제거 - color_segments 사용
        // window.set_terminal_content("Welcome to STerm!\nInitializing terminal...\n$ ".into());

        Ok(())
    }

    async fn setup_ui_update_callback(&self) -> Result<()> {
        let terminal_manager = self.terminal_manager.clone();
        let window_weak = self.window.clone();

        // 터미널 매니저에 UI 업데이트 콜백 설정
        let mut tm = terminal_manager.lock().await;
        tm.set_ui_update_callback(Box::new(move |session_id: SessionId, content: String| {
            let window_weak = window_weak.clone();

            // UI 업데이트를 메인 스레드에서 실행 - color_segments 우선 사용
            // slint::invoke_from_event_loop(move || {
            //     if let Some(window) = window_weak.upgrade() {
            //         window.set_terminal_content(content.into());
            //         log::debug!("UI updated with terminal content for session {}", session_id);
            //     }
            // }).unwrap_or_else(|e| log::error!("Failed to invoke UI update: {:?}", e));
            log::debug!(
                "Skipping direct terminal content update for session {} (using color_segments)",
                session_id
            );
        }));

        Ok(())
    }

    fn add_tab_to_ui(window: &MainWindow, session_id: SessionId, title: &str) {
        let tabs = window.get_tabs();
        let mut tab_data = Vec::new();

        // 기존 탭들 (비활성화)
        for i in 0..tabs.row_count() {
            if let Some(mut tab) = tabs.row_data(i) {
                tab.active = false;
                tab_data.push(tab);
            }
        }

        // 새 탭 추가 (활성화)
        tab_data.push(crate::TabInfo {
            title: title.into(),
            active: true,
            id: session_id as i32,
        });

        let new_tabs_model = VecModel::from(tab_data);
        window.set_tabs(ModelRc::new(new_tabs_model));
    }

    fn remove_tab_from_ui(window: &MainWindow, session_id: SessionId) {
        let tabs = window.get_tabs();
        let tab_id = session_id as i32;
        let mut tab_data = Vec::new();

        // 해당 탭을 제외한 모든 탭 수집
        for i in 0..tabs.row_count() {
            if let Some(tab) = tabs.row_data(i) {
                if tab.id != tab_id {
                    tab_data.push(tab);
                }
            }
        }

        let new_tabs_model = VecModel::from(tab_data);
        window.set_tabs(ModelRc::new(new_tabs_model));
    }
}

impl Drop for UIManager {
    fn drop(&mut self) {
        log::info!("UIManager dropped");
    }
}
