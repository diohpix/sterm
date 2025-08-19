mod config;
mod terminal;
mod ui;
mod utils;

use anyhow::Result;
use log::info;
use slint::ComponentHandle;
use std::sync::Arc;
use tokio::sync::Mutex;

slint::include_modules!();

use crate::config::Config;
use crate::terminal::TerminalManager;
use crate::ui::UIManager;

#[tokio::main]
async fn main() -> Result<()> {
    // 로깅 초기화
    env_logger::init();
    info!("STerm starting...");

    // 설정 로드
    let config = Config::load().await?;
    info!("Configuration loaded");

    // 터미널 매니저 생성
    let terminal_manager = Arc::new(Mutex::new(TerminalManager::new(config.clone())?));
    info!("Terminal manager created");

    // UI 생성
    let main_window = MainWindow::new()?;
    let mut ui_manager = UIManager::new(main_window.as_weak(), terminal_manager.clone())?;
    info!("UI manager created");

    // UI 이벤트 핸들러 설정
    ui_manager.setup_event_handlers().await?;
    info!("Event handlers setup complete");

    // 첫 번째 터미널 세션 시작
    {
        let mut tm = terminal_manager.lock().await;
        tm.create_new_session()?;
    }
    info!("Initial terminal session created");

    // UI 실행
    info!("Starting UI event loop");
    main_window.run()?;

    info!("STerm shutting down...");
    Ok(())
}
