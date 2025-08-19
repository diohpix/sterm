use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub terminal: TerminalConfig,
    pub ui: UIConfig,
    pub keybindings: KeyBindings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    pub shell: String,
    pub scrollback_lines: usize,
    pub enable_bold: bool,
    pub enable_italic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    pub font_family: String,
    pub font_size: f32,
    pub background_color: String,
    pub foreground_color: String,
    pub cursor_style: CursorStyle,
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CursorStyle {
    Block,
    Underline,
    Beam,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    pub new_tab: String,
    pub close_tab: String,
    pub new_window: String,
    pub copy: String,
    pub paste: String,
    pub find: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            terminal: TerminalConfig {
                shell: Self::default_shell(),
                scrollback_lines: 10000,
                enable_bold: true,
                enable_italic: true,
            },
            ui: UIConfig {
                font_family: "Monaco".to_string(),
                font_size: 14.0,
                background_color: "#1e1e1e".to_string(),
                foreground_color: "#ffffff".to_string(),
                cursor_style: CursorStyle::Block,
                theme: "dark".to_string(),
            },
            keybindings: KeyBindings {
                new_tab: "cmd+t".to_string(),
                close_tab: "cmd+w".to_string(),
                new_window: "cmd+n".to_string(),
                copy: "cmd+c".to_string(),
                paste: "cmd+v".to_string(),
                find: "cmd+f".to_string(),
            },
        }
    }
}

impl Config {
    pub async fn load() -> Result<Self> {
        let config_path = Self::config_file_path()?;
        
        if config_path.exists() {
            let content = fs::read_to_string(&config_path).await?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save().await?;
            Ok(config)
        }
    }

    pub async fn save(&self) -> Result<()> {
        let config_path = Self::config_file_path()?;
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_path, content).await?;
        
        Ok(())
    }

    fn config_file_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        
        Ok(home_dir.join(".config").join("sterm").join("config.toml"))
    }

    fn default_shell() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
    }
}
