/// Korean IME (Input Method Editor) implementation
/// Based on tterm's Korean input handler
use std::collections::HashMap;

/// State for Korean character composition for a single terminal session
#[derive(Debug, Clone)]
pub struct KoreanInputState {
    pub chosung: Option<char>,    // 초성 (initial consonant)
    pub jungsung: Option<char>,   // 중성 (medial vowel)
    pub jongsung: Option<char>,   // 종성 (final consonant)
    pub is_composing: bool,       // Whether we're currently composing a character
}

impl KoreanInputState {
    pub fn new() -> Self {
        Self {
            chosung: None,
            jungsung: None,
            jongsung: None,
            is_composing: false,
        }
    }

    pub fn reset(&mut self) {
        self.chosung = None;
        self.jungsung = None;
        self.jongsung = None;
        self.is_composing = false;
    }

    /// Get the currently composed character if it exists (only for complete composition)
    pub fn get_current_char(&self) -> Option<char> {
        if let Some(cho) = self.chosung {
            let cho_idx = get_chosung_index(cho)?;
            
            // 중성이 없으면 불완전한 조합이므로 None 반환
            let jung_idx = self.jungsung.and_then(get_jungsung_index)?;
            let jong_idx = self.jongsung.and_then(get_jongsung_index).unwrap_or(0);
            
            Some(compose_korean(cho_idx, jung_idx, jong_idx))
        } else {
            None
        }
    }
    
    /// Get character for UI display - shows individual jamo for incomplete compositions
    pub fn get_display_char(&self) -> Option<char> {
        if let Some(cho) = self.chosung {
            if let Some(_jung) = self.jungsung {
                // Complete composition - use get_current_char
                self.get_current_char()
            } else {
                // Only consonant - show the consonant itself
                Some(cho)
            }
        } else {
            None
        }
    }

    /// Handle backspace during composition
    pub fn handle_backspace(&mut self) {
        if self.jongsung.is_some() {
            self.jongsung = None;
        } else if self.jungsung.is_some() {
            self.jungsung = None;
        } else if self.chosung.is_some() {
            self.chosung = None;
            self.is_composing = false;
        }
    }
}

/// Manager for Korean IME states across multiple terminals
pub struct KoreanIME {
    pub terminal_states: HashMap<usize, KoreanInputState>, // SessionId -> KoreanInputState
}

impl KoreanIME {
    pub fn new() -> Self {
        Self {
            terminal_states: HashMap::new(),
        }
    }

    /// Get or create Korean input state for a terminal
    pub fn get_or_create_state(&mut self, terminal_id: usize) -> &mut KoreanInputState {
        self.terminal_states.entry(terminal_id).or_insert_with(KoreanInputState::new)
    }

    /// Clean up state for a closed terminal
    pub fn remove_terminal(&mut self, terminal_id: usize) {
        self.terminal_states.remove(&terminal_id);
    }

    /// Process input text and return (completed_chars, is_composing, current_composition)
    pub fn process_input(&mut self, terminal_id: usize, input_text: &str) -> (String, bool, Option<char>) {
        let state = self.get_or_create_state(terminal_id);
        let mut result = String::new();
        
        // ANSI escape sequence 처리 (방향키 등)
        if input_text.starts_with("\x1b[") {
            if state.is_composing {
                // 조합 중이면 조합만 완료하고 escape sequence 무효화
                if let Some(composed) = state.get_current_char() {
                    result.push(composed);
                }
                state.reset();
                // escape sequence는 전송하지 않음
            } else {
                // 조합 중이 아니면 정상적으로 전송
                result.push_str(input_text);
            }
            
            let current_composition = if state.is_composing {
                state.get_display_char()
            } else {
                None
            };
            
            return (result, state.is_composing, current_composition);
        }

        for ch in input_text.chars() {
            // macOS 방향키 처리
            if matches!(ch, '\u{f700}' | '\u{f701}' | '\u{f702}' | '\u{f703}') {
                if state.is_composing {
                    // 조합 중: 불완전한 조합 상태인지 확인
                    let is_incomplete = state.chosung.is_some() && state.jungsung.is_none();
                    
                    if is_incomplete {
                        // 불완전한 상태: 자음만 출력
                        if let Some(cho) = state.chosung {
                            result.push(cho);
                        }
                    } else {
                        // 완전한 조합: 조합된 문자 출력
                        if let Some(composed) = state.get_current_char() {
                            result.push(composed);
                        }
                    }
                    state.reset();
                    // 방향키는 무효화 (전송하지 않음)
                    eprintln!("💫 macOS Arrow key during composition - completed composition, arrow key ignored");
                } else {
                    // 조합 중이 아닐 때: ANSI escape sequence로 변환하여 전송
                    let ansi_seq = match ch {
                        '\u{f700}' => "\u{1b}OA", // Up Arrow
                        '\u{f701}' => "\u{1b}OB", // Down Arrow  
                        '\u{f702}' => "\u{1b}OD", // Left Arrow
                        '\u{f703}' => "\u{1b}OC", // Right Arrow
                        _ => unreachable!(),
                    };
                    result.push_str(ansi_seq);
                    eprintln!("💫 macOS Arrow key - converted to ANSI: {:?} -> {:?}", ch, ansi_seq);
                }
            } else if is_korean_jamo(ch) {
                let completed = Self::process_korean_char(state, ch);
                result.push_str(&completed);
            } else {
                // Non-Korean character
                if state.is_composing {
                    // 불완전한 조합 상태 (중성이 없음)인지 확인
                    let is_incomplete = state.chosung.is_some() && state.jungsung.is_none();
                    
                    if is_incomplete {
                        // 불완전한 상태: 자음만 출력
                        if let Some(cho) = state.chosung {
                            result.push(cho);
                        }
                    } else {
                        // 완전한 조합: 조합된 문자 출력
                        if let Some(composed) = state.get_current_char() {
                            result.push(composed);
                        }
                    }
                    state.reset();
                    
                    // 엔터키는 조합만 완료하고 무효화, ESC는 조합 완료 후 전송
                    if ch == '\r' || ch == '\n' {
                        // 엔터키는 조합만 완료하고 전송하지 않음
                    } else if ch == '\u{1b}' {
                        // ESC는 조합 완료 후 전송 (vi 모드 등을 위해)
                        result.push(ch);
                    } else if ch == ' ' {
                        // 스페이스는 조합 완료 후 함께 전송
                        result.push(ch);
                    } else if !ch.is_control() {
                        // 스페이스가 아닌 일반 문자는 함께 전송
                        result.push(ch);
                    } else {
                        // 제어 문자는 정상 처리
                        result.push(ch);
                    }
                } else {
                    // 조합 중이 아닐 때: 정상적으로 문자 입력
                    result.push(ch);
                }
            }
        }

        let current_composition = if state.is_composing {
            state.get_display_char()
        } else {
            None
        };

        log::debug!("KoreanIME::process_input result: {:?}, is_composing: {}, current_composition: {:?}", 
                   result, state.is_composing, current_composition);

        (result, state.is_composing, current_composition)
    }

    /// Finalize any pending composition for a terminal
    pub fn finalize_composition(&mut self, terminal_id: usize) -> Option<char> {
        if let Some(state) = self.terminal_states.get_mut(&terminal_id) {
            if state.is_composing {
                let completed = state.get_current_char();
                state.reset();
                return completed;
            }
        }
        None
    }

    /// Handle backspace for a terminal
    pub fn handle_backspace(&mut self, terminal_id: usize) -> bool {
        if let Some(state) = self.terminal_states.get_mut(&terminal_id) {
            if state.is_composing {
                state.handle_backspace();
                return true; // Consumed by IME
            }
        }
        false // Not consumed, should be sent to terminal
    }

    /// Process a single Korean character
    fn process_korean_char(state: &mut KoreanInputState, ch: char) -> String {
        let mut result = String::new();

        if is_consonant(ch) {
            if state.chosung.is_none() {
                // First consonant - set as chosung
                state.chosung = Some(ch);
                state.is_composing = true;
            } else if state.jungsung.is_some() && state.jongsung.is_none() {
                // Have chosung + jungsung, this becomes jongsung
                state.jongsung = Some(ch);
            } else if let Some(existing_jong) = state.jongsung {
                // Try to combine with existing jongsung
                if let Some(combined) = combine_consonants(existing_jong, ch) {
                    state.jongsung = Some(combined);
                } else {
                    // Can't combine - complete current and start new
                    if let Some(completed) = state.get_current_char() {
                        result.push(completed);
                    }
                    state.reset();
                    state.chosung = Some(ch);
                    state.is_composing = true;
                }
            } else {
                // Already have chosung but no jungsung - complete and start new
                if let Some(completed) = state.get_current_char() {
                    result.push(completed);
                }
                state.reset();
                state.chosung = Some(ch);
                state.is_composing = true;
            }
        } else if is_vowel(ch) {
            if state.chosung.is_some() && state.jungsung.is_none() {
                // Have chosung, this becomes jungsung
                state.jungsung = Some(ch);
            } else if let Some(existing_jung) = state.jungsung {
                // Try to combine with existing jungsung
                if let Some(jong) = state.jongsung {
                    // Have jongsung - need to move it to new syllable
                    let cho_idx = get_chosung_index(state.chosung.unwrap()).unwrap();
                    let jung_idx = get_jungsung_index(existing_jung).unwrap();
                    let completed = compose_korean(cho_idx, jung_idx, 0);
                    result.push(completed);

                    // Start new syllable
                    state.reset();
                    state.chosung = Some(jong);
                    state.jungsung = Some(ch);
                    state.is_composing = true;
                } else if let Some(combined) = combine_vowels(existing_jung, ch) {
                    state.jungsung = Some(combined);
                } else {
                    // Can't combine - complete current
                    if let Some(completed) = state.get_current_char() {
                        result.push(completed);
                    }
                    state.reset();
                    result.push(ch);
                }
            } else {
                // No chosung - can't start with vowel
                result.push(ch);
            }
        }

        result
    }
}

/// Check if character is a Korean jamo (consonant or vowel)
pub fn is_korean_jamo(ch: char) -> bool {
    is_consonant(ch) || is_vowel(ch)
}

/// Check if character is a consonant
pub fn is_consonant(ch: char) -> bool {
    matches!(ch, 
        'ㄱ' | 'ㄲ' | 'ㄴ' | 'ㄷ' | 'ㄸ' | 'ㄹ' | 'ㅁ' | 'ㅂ' | 'ㅃ' | 
        'ㅅ' | 'ㅆ' | 'ㅇ' | 'ㅈ' | 'ㅉ' | 'ㅊ' | 'ㅋ' | 'ㅌ' | 'ㅍ' | 'ㅎ'
    )
}

/// Check if character is a vowel
pub fn is_vowel(ch: char) -> bool {
    matches!(ch,
        'ㅏ' | 'ㅐ' | 'ㅑ' | 'ㅒ' | 'ㅓ' | 'ㅔ' | 'ㅕ' | 'ㅖ' | 'ㅗ' | 'ㅘ' | 
        'ㅙ' | 'ㅚ' | 'ㅛ' | 'ㅜ' | 'ㅝ' | 'ㅞ' | 'ㅟ' | 'ㅠ' | 'ㅡ' | 'ㅢ' | 'ㅣ'
    )
}

/// Get chosung (initial consonant) index
pub fn get_chosung_index(ch: char) -> Option<usize> {
    let chosungs = [
        'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 
        'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'
    ];
    chosungs.iter().position(|&c| c == ch)
}

/// Get jungsung (medial vowel) index
pub fn get_jungsung_index(ch: char) -> Option<usize> {
    let jungsungs = [
        'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 
        'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ', 'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ'
    ];
    jungsungs.iter().position(|&c| c == ch)
}

/// Get jongsung (final consonant) index (0 = no jongsung)
pub fn get_jongsung_index(ch: char) -> Option<usize> {
    let jongsungs = [
        '\0', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ',
        'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ', 'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ',
        'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'
    ];
    jongsungs.iter().position(|&c| c == ch)
}

/// Compose Korean syllable from indices
pub fn compose_korean(chosung_idx: usize, jungsung_idx: usize, jongsung_idx: usize) -> char {
    let code = 0xAC00 + (chosung_idx * 21 + jungsung_idx) * 28 + jongsung_idx;
    char::from_u32(code as u32).unwrap_or('?')
}

/// Try to combine two consonants
pub fn combine_consonants(first: char, second: char) -> Option<char> {
    match (first, second) {
        ('ㄱ', 'ㅅ') => Some('ㄳ'),
        ('ㄴ', 'ㅈ') => Some('ㄵ'),
        ('ㄴ', 'ㅎ') => Some('ㄶ'),
        ('ㄹ', 'ㄱ') => Some('ㄺ'),
        ('ㄹ', 'ㅁ') => Some('ㄻ'),
        ('ㄹ', 'ㅂ') => Some('ㄼ'),
        ('ㄹ', 'ㅅ') => Some('ㄽ'),
        ('ㄹ', 'ㅌ') => Some('ㄾ'),
        ('ㄹ', 'ㅍ') => Some('ㄿ'),
        ('ㄹ', 'ㅎ') => Some('ㅀ'),
        ('ㅂ', 'ㅅ') => Some('ㅄ'),
        _ => None,
    }
}

/// Try to combine two vowels
pub fn combine_vowels(first: char, second: char) -> Option<char> {
    match (first, second) {
        ('ㅗ', 'ㅏ') => Some('ㅘ'),
        ('ㅗ', 'ㅐ') => Some('ㅙ'),
        ('ㅗ', 'ㅣ') => Some('ㅚ'),
        ('ㅜ', 'ㅓ') => Some('ㅝ'),
        ('ㅜ', 'ㅔ') => Some('ㅞ'),
        ('ㅜ', 'ㅣ') => Some('ㅟ'),
        ('ㅡ', 'ㅣ') => Some('ㅢ'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_korean_composition() {
        let mut ime = KoreanIME::new();
        
        // Test "안녕" composition
        let (result, composing, _) = ime.process_input(0, "ㅇ");
        assert_eq!(result, "");
        assert!(composing);
        
        let (result, composing, _) = ime.process_input(0, "ㅏ");
        assert_eq!(result, "");
        assert!(composing);
        
        let (result, composing, _) = ime.process_input(0, "ㄴ");
        assert_eq!(result, "");
        assert!(composing);
        
        let (result, composing, _) = ime.process_input(0, "ㄴ"); // This should complete "안" and start "ㄴ"
        assert_eq!(result, "안");
        assert!(composing);
    }

    #[test]
    fn test_vowel_combination() {
        let mut ime = KoreanIME::new();
        
        // Test "과" composition (ㄱ + ㅗ + ㅏ = ㄱ + ㅘ)
        ime.process_input(0, "ㄱ");
        ime.process_input(0, "ㅗ");
        let (result, composing, current) = ime.process_input(0, "ㅏ");
        
        assert_eq!(result, "");
        assert!(composing);
        assert_eq!(current, Some('과'));
    }

    #[test]
    fn test_consonant_combination() {
        let mut ime = KoreanIME::new();
        
        // Test "갉" composition (ㄱ + ㅏ + ㄱ + ㅅ = ㄱ + ㅏ + ㄳ)
        ime.process_input(0, "ㄱ");
        ime.process_input(0, "ㅏ");
        ime.process_input(0, "ㄱ");
        let (result, composing, current) = ime.process_input(0, "ㅅ");
        
        assert_eq!(result, "");
        assert!(composing);
        assert_eq!(current, Some('갉'));
    }
}
