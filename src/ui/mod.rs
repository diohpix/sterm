use anyhow::Result;
use slint::{Model, ModelRc, VecModel, Weak};
use std::sync::mpsc;
use std::sync::Arc;
// Duration import removed - no longer using timers
use tokio::sync::Mutex;

use crate::terminal::{SessionId, TerminalManager};
use crate::utils::font::FontMetrics;
use crate::utils::korean_ime::KoreanIME;
use crate::{ColorSegment, CursorInfo, MainWindow, TerminalKeyEvent};

/// 터미널로 전달하기에 안전한 키 입력인지 확인하고 필요시 변환  
fn process_and_filter_terminal_input(event: &TerminalKeyEvent, korean_ime: &Arc<Mutex<KoreanIME>>, session_id: SessionId) -> Option<(String, Option<char>)> {
    let input = &event.text.to_string();
    log::debug!("process_and_filter_terminal_input called with: {:?}", input);
    
    if input.is_empty() {
        log::debug!("Filtered: empty input");
        return None;
    }
    
    // 완전히 공백으로만 구성된 문자열 필터링
    
    
    // 먼저 필터링할 문자들을 체크 (단일 문자인지 확인)
    if input.chars().count() == 1 {
        let ch = input.chars().next().unwrap();
        match ch {
            // 나머지 제어 문자들은 필터링
            '\u{00}'..='\u{1f}' | '\u{7f}' => {
                // 허용할 제어 문자들 제외 (ESC 포함)
                if !matches!(ch, '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' | '\u{1b}') {
                    log::debug!("Filtered control character: {:?} (\\u{{{:04x}}})", ch, ch as u32);
                    return None;
                }
            }
            // macOS 특수 키 처리 (방향키 등) - IME에서 처리하도록 이동
            '\u{f700}' | '\u{f701}' | '\u{f702}' | '\u{f703}' => {
                // macOS 방향키들은 IME에서 조합 상태를 확인 후 처리
                // 여기서는 변환하지 않고 그대로 IME로 전달
            }
            // 기타 macOS 특수 키 범위는 필터링
            '\u{f704}'..='\u{f8ff}' => {
                log::debug!("Filtered macOS special key: {:?} (\\u{{{:04x}}})", ch, ch as u32);
                return None;
            }
            _ => {}
        }
    } else {
        // 멀티바이트 문자열의 경우
        
        // macOS 특수 키들이 포함된 경우 필터링
     /*   if input.chars().any(|c| matches!(c, '\u{f700}'..='\u{f8ff}')) {
            log::debug!("Filtered macOS special key sequence: {:?}", input);
            return None;
        }*/
        
        // escape sequence 필터링 (단순 ESC는 제외)
        if input.starts_with('\u{1b}') && input.chars().count() > 1 {
            log::debug!("Filtered escape sequence: {:?}", input);
            return None;
        }
        
        // 대부분 제어 문자로만 구성된 경우 필터링
        if input.chars().all(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' | '\u{1b}')) {
            log::debug!("Filtered control sequence: {:?}", input);
            return None;
        }
    }
    
    // 모든 입력에 대해 한국어 IME 처리
    eprintln!("💫 ENTERING IME PROCESSING with: {:?}", input);
    if let Ok(mut ime) = korean_ime.try_lock() {
        eprintln!("💫 IME LOCK SUCCESS - calling process_input");
        let (completed_text, _is_composing, current_composition) = ime.process_input(session_id, input);
        eprintln!("💫 IME RESULT: completed_text={:?}, composition={:?}", completed_text, current_composition);
        if !completed_text.is_empty() {
            eprintln!("💫 RETURNING NON-EMPTY: {:?}", completed_text);
            Some((completed_text, current_composition))
        } else {
            eprintln!("💫 RETURNING EMPTY STRING");
            Some((String::new(), current_composition))
        }
    } else {
        eprintln!("💫 IME LOCK FAILED - using raw input");
        Some((input.to_string(), None))
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
    korean_ime: Arc<Mutex<KoreanIME>>,
    last_control_key_time: Arc<Mutex<std::time::Instant>>,
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
            korean_ime: Arc::new(Mutex::new(KoreanIME::new())),
            last_control_key_time: Arc::new(Mutex::new(std::time::Instant::now())),
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
            let korean_ime = self.korean_ime.clone();
            let window_weak = self.window.clone();
            let last_control_key_time = self.last_control_key_time.clone();

            window.on_terminal_input(move |event| {
                let terminal_manager = terminal_manager.clone();
                let korean_ime = korean_ime.clone();
                let window_weak = window_weak.clone();
                let last_control_key_time = last_control_key_time.clone();
                
                eprintln!("🔥 BASIC INPUT EVENT: text={:?}, len={}, chars={}", 
                    event.text, event.text.len(), event.text.chars().count());
                    
                // 엔터키 특별 디버깅
                if event.text == "\n" || event.text == "\r" {
                    eprintln!("🔥 ENTER KEY DETECTED! Checking processing path...");
                }
                log::debug!("Received terminal input event: text={:?}, modifiers={{alt:{}, ctrl:{}, meta:{}, shift:{}}}, repeat:{}", 
                    event.text, event.modifiers.alt, event.modifiers.control, event.modifiers.meta, event.modifiers.shift, event.repeat);
                
                // ESC 키 특별 처리 - 빈 텍스트일 때 ESC로 가정
                if event.text.is_empty() && !event.modifiers.alt && !event.modifiers.control && !event.modifiers.meta && !event.modifiers.shift {
                    log::debug!("Empty text event detected - assuming ESC key");
                    // ESC 키 처리
                    if let Ok(tm) = terminal_manager.try_lock() {
                        if let Some(active_session) = tm.get_active_session() {
                            let session_id = active_session.id;
                            
                            // 한글 조합 중인 경우 조합 완료 후 ESC 전송
                            let was_composing = if let Ok(mut ime) = korean_ime.try_lock() {
                                let composing = ime.terminal_states.get(&session_id).map(|state| state.is_composing).unwrap_or(false);
                                if composing {
                                    let (completed_text, _is_composing, current_composition) = ime.process_input(session_id, "\u{1b}");
                                    if !completed_text.is_empty() {
                                        let _ = tm.write_to_session(session_id, &completed_text);
                                    }
                                    
                                    // UI 업데이트
                                    if let Some(window) = window_weak.upgrade() {
                                        // composition_text 직접 업데이트
                                        let composition_str = current_composition.map(|c| c.to_string()).unwrap_or_default();
                                        
                                        // terminal_state 업데이트
                                        let mut terminal_state = window.get_terminal_state();
                                        terminal_state.composition_text = composition_str.into();
                                        window.set_terminal_state(terminal_state);
                                    }
                                }
                                composing
                            } else { false };
                            
                            // ESC 전송 (조합 중이 아니었거나 조합 완료 후)
                            if let Err(e) = tm.write_to_session(session_id, "\u{1b}") {
                                log::error!("Failed to write ESC to session {}: {}", session_id, e);
                            } else {
                                log::debug!("ESC key sent to PTY for session {}", session_id);
                            }
                        }
                    }
                    return;
                }
                
                // Control 키가 눌렸을 때 시간 기록
                if event.modifiers.control {
                    if let Ok(mut last_time) = last_control_key_time.try_lock() {
                        *last_time = std::time::Instant::now();
                    }
                }

                // tterm 스타일: 특수 키 처리 (백스페이스, 엔터, 스페이스 등)
                if event.text == "\u{08}" { // Backspace
                    if let Ok(tm) = terminal_manager.try_lock() {
                        if let Some(active_session) = tm.get_active_session() {
                            let session_id = active_session.id;
                            
                            // 한글 IME에서 백스페이스 처리
                            if let Ok(mut ime) = korean_ime.try_lock() {
                                let consumed = ime.handle_backspace(session_id);
                                
                                // 한글 조합 상태 업데이트
                                let current_composition = ime.terminal_states.get(&session_id)
                                    .and_then(|state| if state.is_composing { state.get_display_char() } else { None });
                                
                                // UI 업데이트
                                if let Some(window) = window_weak.upgrade() {
                                    let mut terminal_state = window.get_terminal_state();
                                    terminal_state.composition_text = current_composition
                                        .map(|c| c.to_string())
                                        .unwrap_or_default()
                                        .into();
                                    window.set_terminal_state(terminal_state);
                                    log::debug!("Korean composition after backspace: {:?}", current_composition);
                                }
                                
                                // IME가 처리하지 않은 경우만 터미널로 전송
                                if !consumed {
                                    if let Err(e) = tm.write_to_session(session_id, "\u{08}") {
                                        log::error!("Failed to write backspace to terminal: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                
                // 엔터키도 일반 IME 경로로 처리하도록 변경
                // (기존 별도 처리 제거)

                // tterm 스타일: modifier 키가 눌렸는데 텍스트가 비어있으면 무시
                if (event.modifiers.control || event.modifiers.alt || event.modifiers.meta) && event.text.is_empty() {
                    log::debug!("Ignoring empty text event with modifier keys: ctrl:{}, alt:{}, meta:{}", 
                        event.modifiers.control, event.modifiers.alt, event.modifiers.meta);
                    return;
                }
                
                // 특수키 및 Modifier 키 조합을 터미널 바이트로 변환
                if let Some(key_bytes) = Self::convert_key_event_to_terminal_bytes(&event) {
                    if let Ok(tm) = terminal_manager.try_lock() {
                        if let Some(active_session) = tm.get_active_session() {
                            let session_id = active_session.id;
                            let bytes_str = String::from_utf8_lossy(&key_bytes);
                            
                            // 특수키를 IME를 통해 처리 (조합 상태 확인 포함)
                            let (filtered_input, current_composition) = match process_and_filter_terminal_input(&TerminalKeyEvent {
                                text: bytes_str.to_string().into(),
                                modifiers: event.modifiers.clone(),
                                repeat: event.repeat,
                            }, &korean_ime, session_id) {
                                Some((processed, composition)) => (processed, composition),
                                None => {
                                    log::debug!("Filtered special key: {:?}", bytes_str);
                                    return;
                                }
                            };
                            
                            // 완성된 텍스트만 터미널로 전송
                            if !filtered_input.is_empty() {
                                if let Err(e) = tm.write_to_session(session_id, &filtered_input) {
                                    log::error!("Failed to write special key to terminal: {}", e);
                                } else {
                                    log::debug!("Sent special key: {:?} -> {}", key_bytes, filtered_input.escape_debug());
                                }
                            }
                            
                            // 조합 중인 글자 UI 업데이트
                            if let Some(window) = window_weak.upgrade() {
                                let mut terminal_state = window.get_terminal_state();
                                terminal_state.composition_text = current_composition
                                    .map(|c| c.to_string())
                                    .unwrap_or_default()
                                    .into();
                                window.set_terminal_state(terminal_state);
                            }
                        }
                    } else {
                        log::warn!("Could not acquire terminal manager lock for key: {:?}", event.text);
                    };
                    return; // 특수키/modifier는 일반 텍스트 처리하지 않음
                }

                // 일반 텍스트 입력 처리
                if let Ok(tm) = terminal_manager.try_lock() {
                    if let Some(active_session) = tm.get_active_session() {
                        let session_id = active_session.id;
                        
                        // Slint의 중복 이벤트 방지: Ctrl 키 직후의 텍스트 이벤트는 무시
                        if !event.modifiers.control && !event.modifiers.alt && !event.modifiers.meta {
                            if let Ok(last_time) = last_control_key_time.try_lock() {
                                let elapsed = last_time.elapsed();
                                if elapsed < std::time::Duration::from_millis(50) && event.text.chars().count() == 1 {
                                    // 최근 50ms 내에 control 키가 눌렸고 단일 문자라면 무시
                                    log::debug!("Ignoring duplicate text event after Ctrl key: {:?}", event.text);
                                    return;
                                }
                            }
                        }
                        
                        // 한글 IME 처리 및 필터링
                        eprintln!("💫 CALLING process_and_filter_terminal_input with: {:?}", event.text);
                        let (filtered_input, current_composition) = match process_and_filter_terminal_input(&event, &korean_ime, session_id) {
                            Some((processed, composition)) => {
                                eprintln!("💫 PROCESSED: {:?} -> {:?}", event.text, processed);
                                (processed, composition)
                            },
                            None => {
                                eprintln!("💫 FILTERED OUT: {:?}", event.text);
                                return;
                            }
                        };

                        // 완성된 텍스트만 터미널로 전송
                        if !filtered_input.is_empty() {
                            eprintln!("💫 WRITING TO PTY: {:?}", filtered_input);
                            if let Err(e) = tm.write_to_session(session_id, &filtered_input) {
                                eprintln!("💫 PTY WRITE ERROR: {}", e);
                            } else {
                                eprintln!("💫 PTY WRITE SUCCESS!");
                            }
                        } else {
                            eprintln!("💫 EMPTY INPUT - NOT WRITING TO PTY");
                        }
                        
                        // 조합 중인 글자 UI 업데이트
                        if let Some(window) = window_weak.upgrade() {
                            let mut terminal_state = window.get_terminal_state();
                            terminal_state.composition_text = current_composition
                                .map(|c| c.to_string())
                                .unwrap_or_default()
                                .into();
                            window.set_terminal_state(terminal_state);
                            log::debug!("Korean composition updated: {:?}", current_composition);
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
    
    /// tterm 스타일: 키 이벤트를 터미널 바이트로 변환 (특수키 + modifier 조합)
    fn convert_key_event_to_terminal_bytes(event: &TerminalKeyEvent) -> Option<Vec<u8>> {
        let text = event.text.as_str();
        log::debug!("convert_key_event_to_terminal_bytes called with: {:?}", text);
        
        // 1. 먼저 특수키들을 처리 (텍스트와 무관한 키들)
        if let Some(special_bytes) = Self::handle_special_keys(text) {
            return Some(special_bytes);
        }
        
        // 2. Ctrl 키 조합 처리 macOS에서는 meta
        if event.modifiers.meta {
            return Self::ctrl_key_to_bytes(text);
        }
        
        // 3. Alt 키 조합 처리 (ESC + 키)  
        if event.modifiers.alt {
            return Self::alt_key_to_bytes(text);
        }
        
        // 4. Meta (Cmd) 키는 보통 애플리케이션 단축키이므로 무시 macOs에서는 ctrl
        if event.modifiers.control {
            return None;
        }
        
        None
    }
    
    /// 특수키들을 터미널 바이트로 변환 (tterm 스타일)
    fn handle_special_keys(text: &str) -> Option<Vec<u8>> {
        log::debug!("handle_special_keys called with: {:?}", text);
        match text {
            // 백스페이스 (\u{08})
            "\u{08}" => Some(vec![0x7F]), // DEL (127)
            
            // Tab
            "\t" => Some(b"\t".to_vec()),
            
            // Enter/Newline - 일반 텍스트 경로로 처리하도록 특수키에서 제외
            // "\n" | "\r" => Some(b"\r".to_vec()), // 주석 처리
            
            // Escape
            "\u{1B}" => Some(b"\x1b".to_vec()),
            
            // 화살표 키들 (ANSI escape sequences)
            // 주의: 이 패턴들은 실제 키보드 입력에서는 잘 안나타나고
            // 보통 Key 이벤트로 처리되지만, 대비해서 넣어둠
            _ if text.starts_with("\u{1B}[") => {
                match text {
                    "\u{1B}[A" => Some(b"\x1b[A".to_vec()), // Up Arrow
                    "\u{1B}[B" => Some(b"\x1b[B".to_vec()), // Down Arrow  
                    "\u{1B}[C" => Some(b"\x1b[C".to_vec()), // Right Arrow
                    "\u{1B}[D" => Some(b"\x1b[D".to_vec()), // Left Arrow
                    "\u{1B}[3~" => Some(b"\x1b[3~".to_vec()), // Delete
                    "\u{1B}[H" => Some(b"\x1b[H".to_vec()), // Home
                    "\u{1B}[F" => Some(b"\x1b[F".to_vec()), // End
                    "\u{1B}[5~" => Some(b"\x1b[5~".to_vec()), // Page Up
                    "\u{1B}[6~" => Some(b"\x1b[6~".to_vec()), // Page Down
                    _ => None,
                }
            }
            
            _ => None,
        }
    }
    
    /// tterm 스타일: 백스페이스 키 처리 (한글 IME 우선)
    fn handle_backspace_key(
        terminal_manager: &Arc<Mutex<TerminalManager>>,
        korean_ime: &Arc<Mutex<KoreanIME>>,
        window_weak: &Weak<MainWindow>
    ) {
        if let Ok(tm) = terminal_manager.try_lock() {
            if let Some(active_session) = tm.get_active_session() {
                let session_id = active_session.id;
                
                // 한글 IME에서 백스페이스 처리
                if let Ok(mut ime) = korean_ime.try_lock() {
                    let consumed = ime.handle_backspace(session_id);
                    
                    // 한글 조합 상태 업데이트
                    let current_composition = ime.terminal_states.get(&session_id)
                        .and_then(|state| if state.is_composing { 
                            state.get_display_char() 
                        } else { 
                            None 
                        });
                    
                    // UI 업데이트 (조합 중인 글자 표시)
                    if let Some(window) = window_weak.upgrade() {
                        let mut terminal_state = window.get_terminal_state();
                        terminal_state.composition_text = current_composition
                            .map(|c| c.to_string())
                            .unwrap_or_default()
                            .into();
                        window.set_terminal_state(terminal_state);
                        log::debug!("Korean composition after backspace: {:?}", current_composition);
                    }
                    
                    // 한글 IME에서 처리했으면 터미널로 백스페이스 보내지 않음
                    if consumed {
                        return;
                    }
                }
                
                // 한글 IME에서 처리하지 않았으면 터미널로 백스페이스 전송
                if let Err(e) = tm.write_to_session(session_id, "\u{7f}") {
                    log::error!("Failed to write backspace to terminal: {}", e);
                }
            }
        }
    }
    
    /// tterm 스타일: 엔터 키 처리 (한글 조합 완료, 엔터는 조합 중이 아닐 때만 전송)
    fn handle_enter_key(
        terminal_manager: &Arc<Mutex<TerminalManager>>,
        korean_ime: &Arc<Mutex<KoreanIME>>,
        window_weak: &Weak<MainWindow>
    ) {
        if let Ok(tm) = terminal_manager.try_lock() {
            if let Some(active_session) = tm.get_active_session() {
                let session_id = active_session.id;
                
                // 한글 조합 중인지 확인
                let was_composing = if let Ok(ime) = korean_ime.try_lock() {
                    ime.terminal_states.get(&session_id)
                        .map(|state| state.is_composing)
                        .unwrap_or(false)
                } else {
                    false
                };
                
                // 한글 조합 완료 처리
                if let Ok(mut ime) = korean_ime.try_lock() {
                    if let Some(state) = ime.terminal_states.get_mut(&session_id) {
                        if state.is_composing {
                            if let Some(completed) = state.get_current_char() {
                                // 조합 중인 글자 완성해서 터미널로 전송
                                if let Err(e) = tm.write_to_session(session_id, &completed.to_string()) {
                                    log::error!("Failed to write completed Korean char to terminal: {}", e);
                                }
                            }
                            state.reset();
                            
                            // UI 업데이트 - 조합 완료로 composition_text 비우기
                            if let Some(window) = window_weak.upgrade() {
                                let mut terminal_state = window.get_terminal_state();
                                terminal_state.composition_text = "".into();
                                window.set_terminal_state(terminal_state);
                                log::debug!("Korean composition completed on Enter (Enter not sent)");
                            }
                        }
                    }
                }
                
                // 조합 중이었으면 엔터 전송하지 않음, 조합 중이 아니었으면 엔터 전송
                if !was_composing {
                    if let Err(e) = tm.write_to_session(session_id, "\r") {
                        log::error!("Failed to write enter to terminal: {}", e);
                    }
                }
            }
        }
    }
    
    /// tterm 스타일: 스페이스 키 처리 (한글 조합 완료 후 스페이스)
    fn handle_space_key(
        terminal_manager: &Arc<Mutex<TerminalManager>>,
        korean_ime: &Arc<Mutex<KoreanIME>>,
        window_weak: &Weak<MainWindow>
    ) {
        if let Ok(tm) = terminal_manager.try_lock() {
            if let Some(active_session) = tm.get_active_session() {
                let session_id = active_session.id;
                
                // 한글 조합 완료 처리
                if let Ok(mut ime) = korean_ime.try_lock() {
                    if let Some(state) = ime.terminal_states.get_mut(&session_id) {
                        if state.is_composing {
                            if let Some(completed) = state.get_current_char() {
                                // 조합 중인 글자 완성해서 터미널로 전송
                                if let Err(e) = tm.write_to_session(session_id, &completed.to_string()) {
                                    log::error!("Failed to write completed Korean char to terminal: {}", e);
                                }
                            }
                            state.reset();
                            
                            // UI 업데이트 - 조합 완료로 composition_text 비우기
                            if let Some(window) = window_weak.upgrade() {
                                let mut terminal_state = window.get_terminal_state();
                                terminal_state.composition_text = "".into();
                                window.set_terminal_state(terminal_state);
                                log::debug!("Korean composition completed on Space");
                            }
                        }
                    }
                }
                
                // 스페이스 전송
                if let Err(e) = tm.write_to_session(session_id, " ") {
                    log::error!("Failed to write space to terminal: {}", e);
                }
            }
        }
    }
    
    /// Convert Ctrl key combinations to terminal control bytes  
    fn ctrl_key_to_bytes(text: &str) -> Option<Vec<u8>> {
        // Slint에서 Ctrl 키 조합 시 텍스트가 비어있을 수 있음을 고려
        if text.is_empty() {
            // Ctrl 키만 눌린 경우 - 일반적인 Ctrl 조합들 처리 불가
            return None;
        }
        
        // Single character Ctrl combinations
        if text.chars().count() == 1 {
            let ch = text.chars().next()?;
            match ch.to_ascii_lowercase() {
                'a' => Some(b"\x01".to_vec()), // Ctrl+A
                'b' => Some(b"\x02".to_vec()), // Ctrl+B  
                'c' => Some(b"\x03".to_vec()), // Ctrl+C (SIGINT)
                'd' => Some(b"\x04".to_vec()), // Ctrl+D (EOF)
                'e' => Some(b"\x05".to_vec()), // Ctrl+E
                'f' => Some(b"\x06".to_vec()), // Ctrl+F
                'g' => Some(b"\x07".to_vec()), // Ctrl+G (Bell)
                'h' => Some(b"\x08".to_vec()), // Ctrl+H (Backspace)
                'i' => Some(b"\x09".to_vec()), // Ctrl+I (Tab)
                'j' => Some(b"\x0a".to_vec()), // Ctrl+J (LF)
                'k' => Some(b"\x0b".to_vec()), // Ctrl+K
                'l' => Some(b"\x0c".to_vec()), // Ctrl+L (Clear screen)
                'm' => Some(b"\x0d".to_vec()), // Ctrl+M (CR)
                'n' => Some(b"\x0e".to_vec()), // Ctrl+N
                'o' => Some(b"\x0f".to_vec()), // Ctrl+O
                'p' => Some(b"\x10".to_vec()), // Ctrl+P
                'q' => Some(b"\x11".to_vec()), // Ctrl+Q (XON)
                'r' => Some(b"\x12".to_vec()), // Ctrl+R
                's' => Some(b"\x13".to_vec()), // Ctrl+S (XOFF)
                't' => Some(b"\x14".to_vec()), // Ctrl+T
                'u' => Some(b"\x15".to_vec()), // Ctrl+U
                'v' => Some(b"\x16".to_vec()), // Ctrl+V
                'w' => Some(b"\x17".to_vec()), // Ctrl+W
                'x' => Some(b"\x18".to_vec()), // Ctrl+X
                'y' => Some(b"\x19".to_vec()), // Ctrl+Y
                'z' => Some(b"\x1a".to_vec()), // Ctrl+Z (SIGTSTP)
                '[' => Some(b"\x1b".to_vec()), // Ctrl+[ (ESC)
                '\\' => Some(b"\x1c".to_vec()), // Ctrl+\
                ']' => Some(b"\x1d".to_vec()), // Ctrl+]
                '^' => Some(b"\x1e".to_vec()), // Ctrl+^
                '_' => Some(b"\x1f".to_vec()), // Ctrl+_
                ' ' => Some(b"\x00".to_vec()), // Ctrl+Space (NUL)
                _ => None,
            }
        } else {
            None
        }
    }
    
    /// Convert Alt/Meta key combinations to escape sequences
    fn alt_key_to_bytes(text: &str) -> Option<Vec<u8>> {
        if !text.is_empty() {
            // Alt+key sends ESC followed by the key
            let mut result = vec![0x1b]; // ESC
            result.extend_from_slice(text.as_bytes());
            Some(result)
        } else {
            None
        }
    }
}

impl Drop for UIManager {
    fn drop(&mut self) {
        log::info!("UIManager dropped");
    }
}
