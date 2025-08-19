use anyhow::Result;
use std::path::PathBuf;

// 플랫폼별 모듈은 인라인으로 정의

pub struct Platform;

impl Platform {
    /// 현재 플랫폼의 이름을 반환합니다.
    pub fn name() -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "linux") {
            "Linux"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else {
            "Unknown"
        }
    }

    /// 기본 셸을 반환합니다.
    pub fn default_shell() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
    }

    /// 시스템 설정 디렉토리를 반환합니다.
    pub fn config_dir() -> Result<PathBuf> {
        if let Some(config_dir) = dirs::config_dir() {
            Ok(config_dir.join("sterm"))
        } else {
            Err(anyhow::anyhow!("Could not find config directory"))
        }
    }

    /// 애플리케이션 데이터 디렉토리를 반환합니다.
    pub fn data_dir() -> Result<PathBuf> {
        if let Some(data_dir) = dirs::data_dir() {
            Ok(data_dir.join("sterm"))
        } else {
            Err(anyhow::anyhow!("Could not find data directory"))
        }
    }

    /// 캐시 디렉토리를 반환합니다.
    pub fn cache_dir() -> Result<PathBuf> {
        if let Some(cache_dir) = dirs::cache_dir() {
            Ok(cache_dir.join("sterm"))
        } else {
            Err(anyhow::anyhow!("Could not find cache directory"))
        }
    }

    /// 클립보드에 텍스트를 복사합니다.
    pub fn copy_to_clipboard(text: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        return macos::copy_to_clipboard(text);
        
        #[cfg(not(target_os = "macos"))]
        {
            // 다른 플랫폼에서는 기본 구현
            log::warn!("Clipboard functionality not implemented for this platform");
            Err(anyhow::anyhow!("Clipboard not supported"))
        }
    }

    /// 클립보드에서 텍스트를 가져옵니다.
    pub fn paste_from_clipboard() -> Result<String> {
        #[cfg(target_os = "macos")]
        return macos::paste_from_clipboard();
        
        #[cfg(not(target_os = "macos"))]
        {
            // 다른 플랫폼에서는 기본 구현
            log::warn!("Clipboard functionality not implemented for this platform");
            Err(anyhow::anyhow!("Clipboard not supported"))
        }
    }

    /// 시스템 알림을 표시합니다.
    pub fn show_notification(title: &str, message: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        return macos::show_notification(title, message);
        
        #[cfg(not(target_os = "macos"))]
        {
            log::info!("Notification: {} - {}", title, message);
            Ok(())
        }
    }

    /// 시스템 테마를 확인합니다 (다크/라이트).
    pub fn is_dark_mode() -> bool {
        #[cfg(target_os = "macos")]
        return macos::is_dark_mode();
        
        #[cfg(not(target_os = "macos"))]
        false
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Result;
    use std::process::Command;

    pub fn copy_to_clipboard(text: &str) -> Result<()> {
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        
        child.wait()?;
        Ok(())
    }

    pub fn paste_from_clipboard() -> Result<String> {
        let output = Command::new("pbpaste").output()?;
        
        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?)
        } else {
            Err(anyhow::anyhow!("Failed to read from clipboard"))
        }
    }

    pub fn show_notification(title: &str, message: &str) -> Result<()> {
        Command::new("osascript")
            .arg("-e")
            .arg(&format!(
                r#"display notification "{}" with title "{}""#,
                message, title
            ))
            .spawn()?;
        
        Ok(())
    }

    pub fn is_dark_mode() -> bool {
        let output = Command::new("defaults")
            .args(&["read", "-g", "AppleInterfaceStyle"])
            .output();
        
        if let Ok(output) = output {
            if output.status.success() {
                let style = String::from_utf8_lossy(&output.stdout);
                return style.trim() == "Dark";
            }
        }
        
        false
    }
}

/// 시스템 정보를 가져오는 구조체
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub hostname: String,
    pub username: String,
}

impl SystemInfo {
    pub fn new() -> Result<Self> {
        Ok(Self {
            os_name: Platform::name().to_string(),
            os_version: Self::get_os_version()?,
            kernel_version: Self::get_kernel_version()?,
            hostname: Self::get_hostname()?,
            username: Self::get_username()?,
        })
    }

    fn get_os_version() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("sw_vers")
                .arg("-productVersion")
                .output()?;
            
            if output.status.success() {
                Ok(String::from_utf8(output.stdout)?.trim().to_string())
            } else {
                Ok("Unknown".to_string())
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        Ok("Unknown".to_string())
    }

    fn get_kernel_version() -> Result<String> {
        let output = std::process::Command::new("uname")
            .arg("-r")
            .output()?;
        
        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        } else {
            Ok("Unknown".to_string())
        }
    }

    fn get_hostname() -> Result<String> {
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .or_else(|_| {
                let output = std::process::Command::new("hostname").output()?;
                if output.status.success() {
                    Ok(String::from_utf8(output.stdout)?.trim().to_string())
                } else {
                    Err(anyhow::anyhow!("Failed to get hostname"))
                }
            })
    }

    fn get_username() -> Result<String> {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .map_err(|_| anyhow::anyhow!("Failed to get username"))
    }
}
