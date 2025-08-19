use anyhow::Result;
use alacritty_terminal::{
    event::{Event, EventListener, Notify, WindowSize},
    event_loop::{EventLoop, Msg, Notifier},
    grid::{Dimensions, Grid},
    index::{Column, Line, Point},
    selection::SelectionRange,
    sync::FairMutex,
    term::{Term, Config as TermConfig, test::TermSize, TermMode, cell::Cell},
    tty::{self, Options as TtyOptions, Shell},
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use tokio::sync::Mutex;

use crate::config::Config;
use crate::utils::color::{ColorTheme, Color};

static SESSION_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub type SessionId = usize;
pub type UIUpdateCallback = Box<dyn Fn(SessionId, String) + Send + Sync>;

/// Renderable terminal content (from tterm/mterm)
#[derive(Clone)]
pub struct RenderableContent {
    pub grid: Grid<Cell>,
    pub selectable_range: Option<SelectionRange>,
    pub cursor: Cell,
    pub terminal_mode: TermMode,
    pub terminal_size: TerminalSize,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

impl Default for RenderableContent {
    fn default() -> Self {
        Self {
            grid: Grid::new(0, 0, 0),
            selectable_range: None,
            cursor: Cell::default(),
            terminal_mode: TermMode::empty(),
            terminal_size: TerminalSize::default(),
            cursor_line: 0,
            cursor_col: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(String),
    TitleChanged(String),
    Bell,
    Exit,
}

/// Colored text segment for rendering
#[derive(Debug, Clone)]
pub struct ColoredTextSegment {
    pub text: String,
    pub fg_color: Color,
    pub bg_color: Color,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
}

/// Extracted terminal content with color information
#[derive(Debug, Clone)]
pub struct ColoredTerminalContent {
    pub segments: Vec<ColoredTextSegment>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub total_lines: usize,
    pub total_cols: usize,
}

// EventProxy - PTY 이벤트를 수신하여 UI로 전달
#[derive(Clone)]
pub struct EventProxy {
    sender: mpsc::Sender<Event>,
}

impl EventProxy {
    pub fn new() -> (Self, mpsc::Receiver<Event>) {
        let (sender, receiver) = mpsc::channel();
        (Self { sender }, receiver)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        if let Err(_) = self.sender.send(event) {
            log::warn!("Failed to send PTY event: receiver may have been dropped");
        }
    }
}

// tterm 스타일의 TerminalSize
#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cell_width: u16,
    pub cell_height: u16,
    pub num_cols: u16,
    pub num_lines: u16,
    pub layout_width: f32,
    pub layout_height: f32,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cell_width: 8,
            cell_height: 16,
            num_cols: 120, // 더 넓게
            num_lines: 40, // 더 높게  
            layout_width: 960.0,
            layout_height: 640.0,
        }
    }
}

impl From<TerminalSize> for WindowSize {
    fn from(size: TerminalSize) -> Self {
        Self {
            num_lines: size.num_lines,
            num_cols: size.num_cols,
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        }
    }
}

// tterm 스타일의 TerminalBackend 
pub struct TerminalSession {
    pub id: SessionId,
    pub title: String,
    pub term: Arc<FairMutex<Term<EventProxy>>>,
    pub notifier: Notifier,
    pub size: TerminalSize,
    pub content: Arc<Mutex<String>>, // UI 렌더링용 콘텐츠
    pub ui_callback: Option<Arc<UIUpdateCallback>>,
    pub is_running: Arc<Mutex<bool>>,
    pub last_content: RenderableContent,
}

impl TerminalSession {
    pub fn new(
        id: SessionId, 
        shell: &str, 
        pty_event_proxy_sender: mpsc::Sender<(SessionId, Event)>
    ) -> Result<Self> {
        log::info!("Creating new terminal session {} with shell: {}", id, shell);
        
        // PTY 설정 - tterm 방식
        let pty_config = TtyOptions {
            shell: Some(Shell::new(shell.to_string(), vec!["-i".to_string(), "-l".to_string()])),
            working_directory: None,
            env: std::collections::HashMap::new(),
            ..TtyOptions::default()
        };
        
        // Terminal 설정
        let term_config = TermConfig::default();
        let terminal_size = TerminalSize::default();
        
        // EventProxy 생성
        let (event_proxy, event_receiver) = EventProxy::new();
        
        // PTY 생성 (tterm 방식)
        let pty = tty::new(&pty_config, terminal_size.into(), id as u64)?;
        
        // Terminal 생성
        let term_size = TermSize::new(terminal_size.num_cols as usize, terminal_size.num_lines as usize);
        let mut term = Term::new(term_config, &term_size, event_proxy.clone());
        
        // Initial content 생성 (tterm/mterm 방식)
        let initial_content = RenderableContent {
            grid: term.grid().clone(),
            selectable_range: None,
            terminal_mode: *term.mode(),
            terminal_size,
            cursor: term.grid_mut().cursor_cell().clone(),
            cursor_line: 0,
            cursor_col: 0,
        };
        
        let term = Arc::new(FairMutex::new(term));
        
        // EventLoop 생성 및 시작
        let pty_event_loop = EventLoop::new(
            term.clone(),
            event_proxy,
            pty,
            false, // hold
            false, // ref_test
        )?;
        
        let notifier = Notifier(pty_event_loop.channel());
        
        // EventLoop를 백그라운드에서 실행
        let _pty_event_loop_handle = pty_event_loop.spawn();
        
        let content = Arc::new(Mutex::new(String::new()));
        let is_running = Arc::new(Mutex::new(true));
        
        let session = Self {
            id,
            title: format!("Terminal {}", id + 1),
            term,
            notifier,
            size: terminal_size,
            content: content.clone(),
            ui_callback: None,
            is_running: is_running.clone(),
            last_content: initial_content,
        };
        
        // PTY 이벤트 구독 스레드 시작 (tterm 방식) - 이벤트 로깅만
        let _pty_event_subscription = std::thread::Builder::new()
            .name(format!("pty_event_subscription_{}", id))
            .spawn(move || loop {
                if let Ok(event) = event_receiver.recv() {
                    log::debug!("PTY event received for session {}: {:?}", id, event);
                    if let Err(e) = pty_event_proxy_sender.send((id, event.clone())) {
                        log::warn!("pty_event_subscription_{}: Failed to send PtyEvent: {}. Receiver may have been dropped.", id, e);
                        break; // Exit the loop if receiver is gone
                    }
                    if let Event::Exit = event {
                        break;
                    }
                }
            })?;

        // 간단한 로그만 출력
        log::debug!("Terminal session {} setup complete, waiting for PTY data", id);
        
        // 초기 프롬프트 출력을 위해 newline 전송
        session.notifier.notify(b"\n");

        log::info!("Terminal session {} created successfully", id);
        Ok(session)
    }
    
    // UI 콜백 설정
    pub fn set_ui_callback(&mut self, callback: Arc<UIUpdateCallback>) {
        self.ui_callback = Some(callback);
    }
    
    /// Sync terminal state and return renderable content (from tterm/mterm)
    pub fn sync(&mut self) -> &RenderableContent {
        let term = self.term.clone();
        let mut terminal = term.lock();
        let selectable_range = match &terminal.selection {
            Some(s) => s.to_range(&terminal),
            None => None,
        };

        let cursor = terminal.grid_mut().cursor_cell().clone();
        let grid_ref = terminal.grid();
        let point: Point = grid_ref.cursor.point;
        self.last_content.grid = grid_ref.clone();
        self.last_content.selectable_range = selectable_range;
        self.last_content.cursor = cursor.clone();
        self.last_content.terminal_mode = *terminal.mode();
        self.last_content.terminal_size = self.size;
        self.last_content.cursor_line = point.line.0 as usize;
        self.last_content.cursor_col = point.column.0 as usize;
        &self.last_content
    }
    
    /// Extract text from terminal grid
    pub fn extract_terminal_text(&mut self) -> String {
        let content = self.sync();
        let grid = &content.grid;
        let mut result = String::new();
        
        // Grid를 순회해서 텍스트 추출 (alacritty 방식)
        for indexed in grid.display_iter() {
            let cell = indexed.cell;
            let ch = cell.c;
            
            // 줄바꿈 처리
            if indexed.point.column.0 == 0 && indexed.point.line.0 > 0 {
                result.push('\n');
            }
            
            // 문자 추가 (공백이 아닌 경우만)
            if ch != ' ' || result.chars().last() != Some(' ') {
                result.push(ch);
            }
        }
        
        // 끝의 빈 줄들 제거
        result.trim_end().to_string()
    }
    
    /// Extract text with color information from terminal grid
    pub fn extract_colored_terminal_content(&mut self) -> ColoredTerminalContent {
        let session_id = self.id; // Copy id first to avoid borrow issues
        let content = self.sync();
        let grid = &content.grid;
        let theme = ColorTheme::default();
        let mut segments = Vec::new();
        
        log::debug!("Starting color extraction for session {}", session_id);
        
        // display_iter()를 사용해서 실제 색상 정보 추출하되 줄별로 정리
        let mut current_line = 0;
        let mut line_text = String::new();
        let mut line_segments = Vec::new();
        let mut current_segment_text = String::new();
        let mut current_fg = theme.foreground;
        let mut current_bg = theme.background;
        let mut segment_start_col = 0;
        
        for indexed in grid.display_iter() {
            let cell = indexed.cell;
            let ch = cell.c;
            let line_num = indexed.point.line.0 as usize;
            let _col_num = indexed.point.column.0 as usize;
            
            // Skip wide char spacers
            if cell.flags.contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            
            // Get actual colors from indexed cell
            let mut fg_color = theme.convert_ansi_color(&indexed.fg);
            let mut bg_color = theme.convert_ansi_color(&indexed.bg);
            
            // Apply cell flags
            if cell.flags.contains(alacritty_terminal::term::cell::Flags::INVERSE) {
                std::mem::swap(&mut fg_color, &mut bg_color);
            }
            if cell.flags.intersects(alacritty_terminal::term::cell::Flags::DIM | alacritty_terminal::term::cell::Flags::DIM_BOLD) {
                fg_color = Color {
                    r: ((fg_color.r as f32) * 0.7) as u8,
                    g: ((fg_color.g as f32) * 0.7) as u8,
                    b: ((fg_color.b as f32) * 0.7) as u8,
                    a: fg_color.a,
                };
            }
            
            // 새 줄이 시작되면 이전 줄 처리
            if line_num != current_line {
                // 이전 줄의 마지막 세그먼트 추가
                if !current_segment_text.is_empty() {
                    line_segments.push(ColoredTextSegment {
                        text: current_segment_text.clone(),
                        fg_color: current_fg,
                        bg_color: current_bg,
                        line: current_line,
                        start_col: segment_start_col,
                        end_col: segment_start_col + current_segment_text.chars().count(),
                    });
                }
                
                // 줄별 세그먼트들을 메인 리스트에 추가
                segments.extend(line_segments.clone());
                
                // 새 줄 초기화
                current_line = line_num;
                line_text.clear();
                line_segments.clear();
                current_segment_text.clear();
                segment_start_col = 0;
                current_fg = fg_color;
                current_bg = bg_color;
            }
            
            // 색상이 변경되면 새 세그먼트 시작
            let colors_changed = fg_color.r != current_fg.r || fg_color.g != current_fg.g || fg_color.b != current_fg.b ||
                               bg_color.r != current_bg.r || bg_color.g != current_bg.g || bg_color.b != current_bg.b;
            
            if colors_changed && !current_segment_text.is_empty() {
                // 현재 세그먼트 저장
                line_segments.push(ColoredTextSegment {
                    text: current_segment_text.clone(),
                    fg_color: current_fg,
                    bg_color: current_bg,
                    line: current_line,
                    start_col: segment_start_col,
                    end_col: segment_start_col + current_segment_text.chars().count(),
                });
                
                // 새 세그먼트 시작
                segment_start_col += current_segment_text.chars().count();
                current_segment_text.clear();
                current_fg = fg_color;
                current_bg = bg_color;
            }
            
            // 문자 추가
            current_segment_text.push(ch);
            line_text.push(ch);
        }
        
        // 마지막 세그먼트 처리
        if !current_segment_text.is_empty() {
            let text_len = current_segment_text.chars().count();
            line_segments.push(ColoredTextSegment {
                text: current_segment_text,
                fg_color: current_fg,
                bg_color: current_bg,
                line: current_line,
                start_col: segment_start_col,
                end_col: segment_start_col + text_len,
            });
        }
        segments.extend(line_segments);
        
        log::debug!("Color extraction completed for session {}. Total segments: {}", session_id, segments.len());
        
        // 생성된 세그먼트들을 자세히 디버그 출력
        for (i, seg) in segments.iter().enumerate() {
            let text_preview = if seg.text.len() > 30 {
                format!("{}...", &seg.text[..30])
            } else {
                seg.text.clone()
            };
            let text_escaped = text_preview.replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
            log::debug!("🎨 Generated Segment[{}]: text='{}' (len={}) fg=rgb({},{},{}) bg=rgb({},{},{}) line={} start_col={}", 
                i, 
                text_escaped,
                seg.text.len(),
                seg.fg_color.r, seg.fg_color.g, seg.fg_color.b,
                seg.bg_color.r, seg.bg_color.g, seg.bg_color.b,
                seg.line, 
                seg.start_col
            );
        }
        
        ColoredTerminalContent {
            segments,
            cursor_line: content.cursor_line,
            cursor_col: content.cursor_col,
            total_lines: grid.screen_lines(),
            total_cols: grid.columns(),
        }
    }
    
    // tterm 방식의 write - Notifier 사용
    pub fn write(&self, data: &str) -> Result<()> {
        log::debug!("Writing to PTY (session {}): {:?}", self.id, data);
        self.notifier.notify(data.as_bytes().to_vec());
        Ok(())
    }

    // tterm 방식의 resize
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        log::info!("Resizing session {} to {}x{}", self.id, cols, rows);
        
        // 터미널 크기 업데이트
        self.size.num_cols = cols;
        self.size.num_lines = rows;
        self.size.layout_width = cols as f32 * self.size.cell_width as f32;
        self.size.layout_height = rows as f32 * self.size.cell_height as f32;
        
        // PTY에 리사이즈 알림
        let window_size: WindowSize = self.size.into();
        self.notifier.0.send(Msg::Resize(window_size))?;
        
        // Term에도 리사이즈 알림
        let mut term = self.term.lock();
        term.resize(TermSize::new(cols as usize, rows as usize));
        
        Ok(())
    }

    pub async fn get_content(&self) -> String {
        self.content.lock().await.clone()
    }

    pub async fn is_alive(&self) -> bool {
        let running = self.is_running.lock().await;
        *running
    }



    pub async fn stop(&self) {
        log::info!("Stopping terminal session {}", self.id);
        let mut running = self.is_running.lock().await;
        *running = false;
        
        // PTY에 종료 신호 전송
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

pub struct TerminalManager {
    config: Config,
    sessions: HashMap<SessionId, TerminalSession>,
    active_session: Option<SessionId>,
    ui_callback: Option<Arc<UIUpdateCallback>>,
    pty_event_sender: mpsc::Sender<(SessionId, Event)>,
    pty_event_receiver: Option<mpsc::Receiver<(SessionId, Event)>>,
}

impl TerminalManager {
    pub fn new(config: Config) -> Result<Self> {
        let (pty_event_sender, pty_event_receiver) = mpsc::channel();
        Ok(Self {
            config,
            sessions: HashMap::new(),
            active_session: None,
            ui_callback: None,
            pty_event_sender,
            pty_event_receiver: Some(pty_event_receiver),
        })
    }
    
    pub fn set_ui_update_callback(&mut self, callback: UIUpdateCallback) {
        self.ui_callback = Some(Arc::new(callback));
    }
    
    pub fn take_pty_event_receiver(&mut self) -> Option<mpsc::Receiver<(SessionId, Event)>> {
        self.pty_event_receiver.take()
    }
    
    pub async fn process_pty_event(&mut self, session_id: SessionId, event: Event) {
        match event {
            Event::PtyWrite(data) => {
                let text = String::from_utf8_lossy(data.as_bytes());
                log::debug!("PTY output for session {}: {:?}", session_id, text);
                
                // 해당 세션의 콘텐츠 업데이트
                if let Some(session) = self.sessions.get(&session_id) {
                    let mut content_guard = session.content.lock().await;
                    content_guard.push_str(&text);
                    
                    // 스크롤백 관리
                    if content_guard.len() > 50000 {
                        let split_pos = content_guard.len() - 40000;
                        if let Some(newline_pos) = content_guard[split_pos..].find('\n') {
                            content_guard.drain(0..split_pos + newline_pos + 1);
                        }
                    }
                    
                    // UI 업데이트 콜백 호출
                    if let Some(callback) = &self.ui_callback {
                        callback(session_id, content_guard.clone());
                    }
                }
            }
            Event::Title(title) => {
                log::debug!("Terminal title changed for session {}: {}", session_id, title);
            }
            Event::Exit => {
                log::info!("Terminal session {} exited", session_id);
                if let Some(session) = self.sessions.get(&session_id) {
                    let mut running_guard = session.is_running.lock().await;
                    *running_guard = false;
                }
            }
            _ => {
                // 다른 이벤트들은 무시
            }
        }
    }
    
    pub fn process_pty_event_sync(&self, session_id: SessionId, event: Event) {
        match event {
            Event::PtyWrite(data) => {
                let text = String::from_utf8_lossy(data.as_bytes());
                log::debug!("PTY output for session {} (sync): {:?}", session_id, text);
                
                // 해당 세션의 콘텐츠 업데이트
                if let Some(session) = self.sessions.get(&session_id) {
                    if let Ok(mut content_guard) = session.content.try_lock() {
                        content_guard.push_str(&text);
                        
                        // 스크롤백 관리
                        if content_guard.len() > 50000 {
                            let split_pos = content_guard.len() - 40000;
                            if let Some(newline_pos) = content_guard[split_pos..].find('\n') {
                                content_guard.drain(0..split_pos + newline_pos + 1);
                            }
                        }
                        
                        // UI 업데이트 콜백 호출
                        if let Some(callback) = &self.ui_callback {
                            callback(session_id, content_guard.clone());
                        }
                    }
                }
            }
            Event::Title(title) => {
                log::debug!("Terminal title changed for session {}: {}", session_id, title);
            }
            Event::Exit => {
                log::info!("Terminal session {} exited", session_id);
                if let Some(session) = self.sessions.get(&session_id) {
                    if let Ok(mut running_guard) = session.is_running.try_lock() {
                        *running_guard = false;
                    }
                }
            }
            _ => {
                // 다른 이벤트들은 무시
            }
        }
    }
    
    pub fn update_session_content_and_get(&self, session_id: SessionId, text: &str) -> Option<String> {
        if let Some(session) = self.sessions.get(&session_id) {
            if let Ok(mut content_guard) = session.content.try_lock() {
                content_guard.push_str(text);
                
                // 스크롤백 관리
                if content_guard.len() > 50000 {
                    let split_pos = content_guard.len() - 40000;
                    if let Some(newline_pos) = content_guard[split_pos..].find('\n') {
                        content_guard.drain(0..split_pos + newline_pos + 1);
                    }
                }
                
                return Some(content_guard.clone());
            }
        }
        None
    }
    
    /// Extract terminal text from session (for UI updates)
    pub fn extract_session_terminal_text(&mut self, session_id: SessionId) -> Option<String> {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            Some(session.extract_terminal_text())
        } else {
            None
        }
    }
    
    pub fn extract_session_colored_content(&mut self, session_id: SessionId) -> Option<ColoredTerminalContent> {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            Some(session.extract_colored_terminal_content())
        } else {
            None
        }
    }

    pub fn create_new_session(&mut self) -> Result<SessionId> {
        let session_id = SESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        let mut session = TerminalSession::new(
            session_id, 
            &self.config.terminal.shell,
            self.pty_event_sender.clone()
        )?;
        
        // UI 콜백 설정
        if let Some(callback) = &self.ui_callback {
            session.set_ui_callback(callback.clone());
        }
        
        self.sessions.insert(session_id, session);
        
        if self.active_session.is_none() {
            self.active_session = Some(session_id);
        }

        log::info!("Created new terminal session: {}", session_id);
        Ok(session_id)
    }

    pub fn get_session(&self, session_id: SessionId) -> Option<&TerminalSession> {
        self.sessions.get(&session_id)
    }

    pub fn get_session_mut(&mut self, session_id: SessionId) -> Option<&mut TerminalSession> {
        self.sessions.get_mut(&session_id)
    }

    pub fn get_active_session(&self) -> Option<&TerminalSession> {
        self.active_session
            .and_then(|id| self.sessions.get(&id))
    }

    pub fn set_active_session(&mut self, session_id: SessionId) -> Result<()> {
        if self.sessions.contains_key(&session_id) {
            self.active_session = Some(session_id);
            log::info!("Set active session: {}", session_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session {} not found", session_id))
        }
    }

    pub async fn close_session(&mut self, session_id: SessionId) -> Result<()> {
        if let Some(session) = self.sessions.remove(&session_id) {
            session.stop().await;
            
            // 세션이 현재 활성 세션인 경우 다른 세션으로 전환
            if self.active_session == Some(session_id) {
                self.active_session = self.sessions.keys().next().copied();
            }
            log::info!("Closed terminal session: {}", session_id);
        }
        Ok(())
    }

    pub fn get_all_sessions(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }

    pub fn write_to_session(&self, session_id: SessionId, data: &str) -> Result<()> {
        if let Some(session) = self.sessions.get(&session_id) {
            session.write(data)?;
        } else {
            log::warn!("Session {} not found for write", session_id);
        }
        Ok(())
    }

    pub fn resize_session(&mut self, session_id: SessionId, cols: u16, rows: u16) -> Result<()> {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.resize(cols, rows)?;
        } else {
            log::warn!("Session {} not found for resize", session_id);
        }
        Ok(())
    }

    pub async fn get_terminal_content(&self, session_id: SessionId) -> Option<String> {
        if let Some(session) = self.sessions.get(&session_id) {
            Some(session.get_content().await)
        } else {
            None
        }
    }
    


    pub async fn cleanup_dead_sessions(&mut self) {
        let mut dead_sessions = Vec::new();
        
        for (id, session) in &self.sessions {
            if !session.is_alive().await {
                dead_sessions.push(*id);
            }
        }
        
        for id in dead_sessions {
            let _ = self.close_session(id).await;
        }
    }
}