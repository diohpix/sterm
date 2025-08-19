use anyhow::Result;

#[derive(Debug, Clone)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "Monaco".to_string(),
            size: 14.0,
            bold: false,
            italic: false,
        }
    }
}

impl FontConfig {
    pub fn new(family: String, size: f32) -> Self {
        Self {
            family,
            size,
            bold: false,
            italic: false,
        }
    }

    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    pub fn calculate_char_dimensions(&self) -> (f32, f32) {
        // 고정 폭 폰트의 문자 크기 계산
        // 실제 구현에서는 폰트 렌더링 라이브러리를 사용해야 함
        let char_width = self.size * 0.6; // 근사치
        let char_height = self.size * 1.2; // 근사치
        (char_width, char_height)
    }

    pub fn is_monospace(&self) -> bool {
        // 일반적인 고정 폭 폰트들
        matches!(
            self.family.to_lowercase().as_str(),
            "monaco" | "consolas" | "courier" | "courier new" 
            | "menlo" | "source code pro" | "fira code" 
            | "hack" | "inconsolata" | "jetbrains mono"
        )
    }
}

pub struct FontManager;

impl FontManager {
    pub fn get_available_fonts() -> Vec<String> {
        // macOS에서 사용 가능한 고정 폭 폰트들
        vec![
            "Monaco".to_string(),
            "Menlo".to_string(),
            "SF Mono".to_string(),
            "Courier".to_string(),
            "Courier New".to_string(),
            "Consolas".to_string(),
            "Source Code Pro".to_string(),
            "Fira Code".to_string(),
            "JetBrains Mono".to_string(),
            "Hack".to_string(),
            "Inconsolata".to_string(),
        ]
    }

    pub fn is_font_available(font_name: &str) -> bool {
        Self::get_available_fonts()
            .iter()
            .any(|f| f.eq_ignore_ascii_case(font_name))
    }

    pub fn get_default_font() -> FontConfig {
        // macOS에서 기본 터미널 폰트
        if Self::is_font_available("Monaco") {
            FontConfig::new("Monaco".to_string(), 14.0)
        } else if Self::is_font_available("Menlo") {
            FontConfig::new("Menlo".to_string(), 14.0)
        } else {
            FontConfig::new("Courier".to_string(), 14.0)
        }
    }

    pub fn validate_font_size(size: f32) -> Result<f32> {
        if size < 6.0 || size > 72.0 {
            Err(anyhow::anyhow!(
                "Font size must be between 6 and 72 pixels, got: {}",
                size
            ))
        } else {
            Ok(size)
        }
    }

    pub fn scale_font_size(current_size: f32, scale_factor: f32) -> f32 {
        let new_size = current_size * scale_factor;
        Self::validate_font_size(new_size).unwrap_or(current_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone)]
pub struct TextAttributes {
    pub weight: FontWeight,
    pub style: FontStyle,
    pub underline: bool,
    pub strikethrough: bool,
}

impl Default for TextAttributes {
    fn default() -> Self {
        Self {
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
            underline: false,
            strikethrough: false,
        }
    }
}

impl TextAttributes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }

    pub fn with_strikethrough(mut self, strikethrough: bool) -> Self {
        self.strikethrough = strikethrough;
        self
    }

    pub fn is_bold(&self) -> bool {
        matches!(self.weight, FontWeight::Bold)
    }

    pub fn is_italic(&self) -> bool {
        matches!(self.style, FontStyle::Italic)
    }
}
