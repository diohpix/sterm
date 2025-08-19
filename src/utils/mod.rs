use anyhow::Result;
use std::path::PathBuf;

pub mod color;
pub mod font;
pub mod platform;

pub use color::*;
pub use font::*;
pub use platform::*;

/// 애플리케이션 데이터 디렉토리를 반환합니다.
pub fn get_app_data_dir() -> Result<PathBuf> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    
    Ok(home_dir.join(".config").join("sterm"))
}

/// 로그 파일 경로를 반환합니다.
pub fn get_log_file_path() -> Result<PathBuf> {
    let app_data_dir = get_app_data_dir()?;
    Ok(app_data_dir.join("logs").join("sterm.log"))
}

/// 문자열이 유효한 색상 코드인지 확인합니다.
pub fn is_valid_color(color: &str) -> bool {
    color.starts_with('#') && color.len() == 7 && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// 바이트 크기를 사람이 읽기 쉬운 형태로 변환합니다.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{:.0} {}", size, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// 텍스트가 UTF-8인지 확인합니다.
pub fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// 문자열을 안전하게 자릅니다 (UTF-8 경계 고려).
pub fn safe_truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_color() {
        assert!(is_valid_color("#ffffff"));
        assert!(is_valid_color("#000000"));
        assert!(is_valid_color("#123abc"));
        assert!(!is_valid_color("ffffff"));
        assert!(!is_valid_color("#fff"));
        assert!(!is_valid_color("#gggggg"));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_safe_truncate() {
        assert_eq!(safe_truncate("hello", 10), "hello");
        assert_eq!(safe_truncate("hello", 3), "hel");
        assert_eq!(safe_truncate("안녕하세요", 6), "안녕");
    }
}
