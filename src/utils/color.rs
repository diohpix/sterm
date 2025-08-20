use alacritty_terminal::vte::ansi::{self, NamedColor};
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub fn from_hex(hex: &str) -> Result<Self> {
        let hex = hex.trim_start_matches('#');

        if hex.len() != 6 {
            return Err(anyhow::anyhow!("Invalid hex color format: {}", hex));
        }

        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;

        Ok(Self::rgb(r, g, b))
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn to_slint_color(&self) -> slint::Color {
        slint::Color::from_rgb_u8(self.r, self.g, self.b)
    }

    pub fn from_slint_color(color: slint::Color) -> Self {
        Self::new(color.red(), color.green(), color.blue(), color.alpha())
    }

    pub fn blend(&self, other: &Color, alpha: f32) -> Self {
        let alpha = alpha.clamp(0.0, 1.0);
        let inv_alpha = 1.0 - alpha;

        Self::new(
            ((self.r as f32 * inv_alpha) + (other.r as f32 * alpha)) as u8,
            ((self.g as f32 * inv_alpha) + (other.g as f32 * alpha)) as u8,
            ((self.b as f32 * inv_alpha) + (other.b as f32 * alpha)) as u8,
            ((self.a as f32 * inv_alpha) + (other.a as f32 * alpha)) as u8,
        )
    }
}

pub struct ColorTheme {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection: Color,
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,
    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_magenta: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self::dark_theme()
    }
}

impl ColorTheme {
    pub fn dark_theme() -> Self {
        Self {
            background: Color::from_hex("#1e1e1e").unwrap(),
            foreground: Color::from_hex("#ffffff").unwrap(),
            cursor: Color::from_hex("#ffffff").unwrap(),
            selection: Color::from_hex("#404040").unwrap(),
            black: Color::from_hex("#000000").unwrap(),
            red: Color::from_hex("#cd0000").unwrap(),
            green: Color::from_hex("#00cd00").unwrap(),
            yellow: Color::from_hex("#cdcd00").unwrap(),
            blue: Color::from_hex("#0000ee").unwrap(),
            magenta: Color::from_hex("#cd00cd").unwrap(),
            cyan: Color::from_hex("#00cdcd").unwrap(),
            white: Color::from_hex("#e5e5e5").unwrap(),
            bright_black: Color::from_hex("#7f7f7f").unwrap(),
            bright_red: Color::from_hex("#ff0000").unwrap(),
            bright_green: Color::from_hex("#00ff00").unwrap(),
            bright_yellow: Color::from_hex("#ffff00").unwrap(),
            bright_blue: Color::from_hex("#5c5cff").unwrap(),
            bright_magenta: Color::from_hex("#ff00ff").unwrap(),
            bright_cyan: Color::from_hex("#00ffff").unwrap(),
            bright_white: Color::from_hex("#ffffff").unwrap(),
        }
    }

    pub fn light_theme() -> Self {
        Self {
            background: Color::from_hex("#ffffff").unwrap(),
            foreground: Color::from_hex("#000000").unwrap(),
            cursor: Color::from_hex("#000000").unwrap(),
            selection: Color::from_hex("#b5d5ff").unwrap(),
            black: Color::from_hex("#000000").unwrap(),
            red: Color::from_hex("#cd0000").unwrap(),
            green: Color::from_hex("#00cd00").unwrap(),
            yellow: Color::from_hex("#cdcd00").unwrap(),
            blue: Color::from_hex("#0000ee").unwrap(),
            magenta: Color::from_hex("#cd00cd").unwrap(),
            cyan: Color::from_hex("#00cdcd").unwrap(),
            white: Color::from_hex("#e5e5e5").unwrap(),
            bright_black: Color::from_hex("#7f7f7f").unwrap(),
            bright_red: Color::from_hex("#ff0000").unwrap(),
            bright_green: Color::from_hex("#00ff00").unwrap(),
            bright_yellow: Color::from_hex("#ffff00").unwrap(),
            bright_blue: Color::from_hex("#5c5cff").unwrap(),
            bright_magenta: Color::from_hex("#ff00ff").unwrap(),
            bright_cyan: Color::from_hex("#00ffff").unwrap(),
            bright_white: Color::from_hex("#ffffff").unwrap(),
        }
    }

    pub fn get_ansi_color(&self, index: u8) -> Color {
        match index {
            0 => self.black,
            1 => self.red,
            2 => self.green,
            3 => self.yellow,
            4 => self.blue,
            5 => self.magenta,
            6 => self.cyan,
            7 => self.white,
            8 => self.bright_black,
            9 => self.bright_red,
            10 => self.bright_green,
            11 => self.bright_yellow,
            12 => self.bright_blue,
            13 => self.bright_magenta,
            14 => self.bright_cyan,
            15 => self.bright_white,
            _ => self.foreground,
        }
    }

    /// Convert alacritty's Color to our Color
    pub fn convert_ansi_color(&self, color: &ansi::Color) -> Color {
        match color {
            ansi::Color::Named(named_color) => self.get_named_color(named_color),
            ansi::Color::Spec(rgb) => Color::rgb(rgb.r, rgb.g, rgb.b),
            ansi::Color::Indexed(indexed_color) => self.get_indexed_color(*indexed_color),
        }
    }

    fn get_named_color(&self, named_color: &NamedColor) -> Color {
        match named_color {
            NamedColor::Foreground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Black => self.black,
            NamedColor::Red => self.red,
            NamedColor::Green => self.green,
            NamedColor::Yellow => self.yellow,
            NamedColor::Blue => self.blue,
            NamedColor::Magenta => self.magenta,
            NamedColor::Cyan => self.cyan,
            NamedColor::White => self.white,
            NamedColor::BrightBlack => self.bright_black,
            NamedColor::BrightRed => self.bright_red,
            NamedColor::BrightGreen => self.bright_green,
            NamedColor::BrightYellow => self.bright_yellow,
            NamedColor::BrightBlue => self.bright_blue,
            NamedColor::BrightMagenta => self.bright_magenta,
            NamedColor::BrightCyan => self.bright_cyan,
            NamedColor::BrightWhite => self.bright_white,
            NamedColor::BrightForeground => self.bright_white,
            NamedColor::DimForeground => self.foreground.blend(&self.background, 0.5),
            NamedColor::DimBlack => self.black.blend(&self.background, 0.5),
            NamedColor::DimRed => self.red.blend(&self.background, 0.5),
            NamedColor::DimGreen => self.green.blend(&self.background, 0.5),
            NamedColor::DimYellow => self.yellow.blend(&self.background, 0.5),
            NamedColor::DimBlue => self.blue.blend(&self.background, 0.5),
            NamedColor::DimMagenta => self.magenta.blend(&self.background, 0.5),
            NamedColor::DimCyan => self.cyan.blend(&self.background, 0.5),
            NamedColor::DimWhite => self.white.blend(&self.background, 0.5),
            NamedColor::Cursor => self.cursor,
        }
    }

    fn get_indexed_color(&self, index: u8) -> Color {
        match index {
            0..=15 => self.get_ansi_color(index),
            16..=231 => {
                // 216 color cube: 6x6x6
                let index = index - 16;
                let r = (index / 36) % 6;
                let g = (index / 6) % 6;
                let b = index % 6;

                let r = if r == 0 { 0 } else { 55 + r * 40 };
                let g = if g == 0 { 0 } else { 55 + g * 40 };
                let b = if b == 0 { 0 } else { 55 + b * 40 };

                Color::rgb(r, g, b)
            }
            232..=255 => {
                // Grayscale: 24 colors
                let gray = 8 + (index - 232) * 10;
                Color::rgb(gray, gray, gray)
            }
        }
    }
}
