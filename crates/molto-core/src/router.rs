use crate::languages::{code_to_alpha2, find_lang_by_code, lang_to_alpha2};
use crate::models::{VoiceProfile, VoiceRuleMode};
use ahash::AHashMap;
use whatlang::Detector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharCategory {
    Whitespace,
    AsciiDigit,
    AsciiPunctuation,
    ArabicDigit,
    ArabicPunctuation,
    ArabicLetter,
    OtherLetter,
    OtherNumber,
    OtherPunctuation,
    Other,
}

// 128-byte lookup table for fast ASCII classification
static ASCII_TABLE: [CharCategory; 128] = {
    let mut table = [CharCategory::Other; 128];
    let mut i = 0usize;
    while i < 128 {
        let b = i as u8;
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            table[i] = CharCategory::Whitespace;
        } else if b >= b'0' && b <= b'9' {
            table[i] = CharCategory::AsciiDigit;
        } else if (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z') {
            table[i] = CharCategory::OtherLetter;
        } else if (b >= 33 && b <= 47)
            || (b >= 58 && b <= 64)
            || (b >= 91 && b <= 96)
            || (b >= 123 && b <= 126)
        {
            table[i] = CharCategory::AsciiPunctuation;
        }
        i += 1;
    }
    table
};

#[inline(always)]
fn classify_char(c: char) -> CharCategory {
    let u = c as u32;
    if u < 128 {
        ASCII_TABLE[u as usize]
    } else if (0x0660..=0x0669).contains(&u) || (0x06F0..=0x06F9).contains(&u) {
        CharCategory::ArabicDigit
    } else if u == 0x061F || u == 0x060C || u == 0x061B || u == 0x0640 || u == 0x00AB || u == 0x00BB {
        CharCategory::ArabicPunctuation
    } else if (0x0600..=0x06FF).contains(&u) || (0x0750..=0x077F).contains(&u) || (0x08A0..=0x08FF).contains(&u) {
        CharCategory::ArabicLetter
    } else if c.is_alphabetic() {
        CharCategory::OtherLetter
    } else if c.is_numeric() {
        CharCategory::OtherNumber
    } else if c.is_ascii_punctuation() || c.is_ascii() == false && (c == '…' || c == '—' || c == '–' || c == '“' || c == '”' || c == '‘' || c == '’') {
        CharCategory::OtherPunctuation
    } else if c.is_whitespace() {
        CharCategory::Whitespace
    } else {
        CharCategory::Other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment {
    pub text: String,
    pub voice_token_id: String,
}

pub struct VoiceRouter {
    profile: VoiceProfile,
    lang_to_voice: AHashMap<String, String>,
    has_arabic_rule: bool,
    detector: Option<Detector>,
}

impl VoiceRouter {
    pub fn new(mut profile: VoiceProfile) -> Self {
        profile.ensure_migrated();
        let mut lang_to_voice = AHashMap::with_capacity(profile.language_rules.len() * 2);
        let mut allowlist = Vec::new();
        let mut has_arabic_rule = false;

        for rule in &profile.language_rules {
            let code_lower = rule.lang_code.to_lowercase();
            lang_to_voice.insert(code_lower.clone(), rule.voice_token_id.clone());

            let a2 = code_to_alpha2(&code_lower);
            lang_to_voice.insert(a2.clone(), rule.voice_token_id.clone());

            if let Some(lang) = find_lang_by_code(&code_lower) {
                lang_to_voice.insert(lang.code().to_string(), rule.voice_token_id.clone());
                lang_to_voice.insert(lang_to_alpha2(lang).to_string(), rule.voice_token_id.clone());
                if lang == whatlang::Lang::Ara {
                    has_arabic_rule = true;
                }
                allowlist.push(lang);
            } else if code_lower == "ara" || code_lower == "ar" {
                has_arabic_rule = true;
            }
        }

        let detector = if !allowlist.is_empty() {
            Some(Detector::with_allowlist(allowlist))
        } else {
            None
        };

        Self {
            profile,
            lang_to_voice,
            has_arabic_rule,
            detector,
        }
    }

    pub fn route_text(&self, text: &str) -> Vec<TextSegment> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut raw_segments: Vec<TextSegment> = Vec::new();
        let mut current_chunk = String::new();
        let mut current_cat: Option<CharCategory> = None;

        for c in text.chars() {
            let cat = classify_char(c);

            // Grouping logic:
            // Whitespace attaches to current text to avoid fragmented voice calls
            let is_compatible = match (current_cat, cat) {
                (None, _) => true,
                (Some(CharCategory::Whitespace), _) => false,
                (Some(_), CharCategory::Whitespace) => true,
                (Some(a), b) if a == b => true,
                // Letters (Arabic vs others)
                (Some(CharCategory::ArabicLetter), CharCategory::ArabicLetter) => true,
                (Some(CharCategory::OtherLetter), CharCategory::OtherLetter) => true,
                // Apostrophes and hyphens inside words stay attached to letters
                (Some(CharCategory::OtherLetter), CharCategory::AsciiPunctuation) if c == '\'' || c == '-' => true,
                (Some(CharCategory::ArabicLetter), CharCategory::ArabicPunctuation) if c == 'ـ' => true,
                // Digits
                (Some(CharCategory::AsciiDigit), CharCategory::AsciiDigit) => true,
                (Some(CharCategory::ArabicDigit), CharCategory::ArabicDigit) => true,
                // Punctuation
                (Some(CharCategory::AsciiPunctuation), CharCategory::AsciiPunctuation) => true,
                (Some(CharCategory::ArabicPunctuation), CharCategory::ArabicPunctuation) => true,
                _ => false,
            };

            if is_compatible {
                if current_cat.is_none() || current_cat == Some(CharCategory::Whitespace) {
                    current_cat = Some(cat);
                }
                current_chunk.push(c);
            } else {
                if !current_chunk.is_empty() {
                    let voice = self.resolve_category(current_cat.unwrap_or(CharCategory::Other), &current_chunk);
                    raw_segments.push(TextSegment {
                        text: std::mem::take(&mut current_chunk),
                        voice_token_id: voice,
                    });
                }
                current_cat = Some(cat);
                current_chunk.push(c);
            }
        }

        if !current_chunk.is_empty() {
            let voice = self.resolve_category(current_cat.unwrap_or(CharCategory::Other), &current_chunk);
            raw_segments.push(TextSegment {
                text: current_chunk,
                voice_token_id: voice,
            });
        }

        // Coalesce consecutive segments with the same voice_token_id
        self.coalesce_segments(raw_segments)
    }

    fn resolve_category(&self, cat: CharCategory, chunk: &str) -> String {
        match cat {
            CharCategory::ArabicDigit => self.resolve_number(true, false),
            CharCategory::AsciiDigit => self.resolve_number(false, true),
            CharCategory::OtherNumber => self.resolve_number(false, false),

            CharCategory::ArabicPunctuation => self.resolve_punctuation(true, false),
            CharCategory::AsciiPunctuation => self.resolve_punctuation(false, true),
            CharCategory::OtherPunctuation => self.resolve_punctuation(false, false),

            CharCategory::ArabicLetter => {
                if self.has_arabic_rule {
                    if let Some(v) = self.lang_to_voice.get("ara") {
                        return v.clone();
                    }
                    if let Some(v) = self.lang_to_voice.get("ar") {
                        return v.clone();
                    }
                }
                self.profile.default_voice_id.clone()
            }
            CharCategory::OtherLetter | CharCategory::Other | CharCategory::Whitespace => {
                self.resolve_natural_language(chunk)
            }
        }
    }

    fn resolve_number(&self, is_arabic: bool, is_english: bool) -> String {
        match &self.profile.number_rules.mode {
            VoiceRuleMode::Unified { voice_token_id } => voice_token_id.clone(),
            VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id,
                english_voice_token_id,
                fallback_voice_token_id,
            } => {
                if is_arabic {
                    arabic_voice_token_id.clone()
                } else if is_english {
                    english_voice_token_id.clone()
                } else {
                    fallback_voice_token_id
                        .clone()
                        .unwrap_or_else(|| self.profile.default_voice_id.clone())
                }
            }
        }
    }

    fn resolve_punctuation(&self, is_arabic: bool, is_english: bool) -> String {
        match &self.profile.punctuation_rules.mode {
            VoiceRuleMode::Unified { voice_token_id } => voice_token_id.clone(),
            VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id,
                english_voice_token_id,
                fallback_voice_token_id,
            } => {
                if is_arabic {
                    arabic_voice_token_id.clone()
                } else if is_english {
                    english_voice_token_id.clone()
                } else {
                    fallback_voice_token_id
                        .clone()
                        .unwrap_or_else(|| self.profile.default_voice_id.clone())
                }
            }
        }
    }

    fn check_distinctive_script(&self, text: &str) -> Option<&'static str> {
        // French accents & ligatures
        if self.lang_to_voice.contains_key("fra") || self.lang_to_voice.contains_key("fr") {
            for c in text.chars() {
                if matches!(c, 'é' | 'è' | 'ê' | 'ë' | 'à' | 'â' | 'ç' | 'ù' | 'û' | 'ô' | 'î' | 'ï' | 'œ' | 'É' | 'È' | 'Ê' | 'Ë' | 'À' | 'Â' | 'Ç' | 'Ù' | 'Û' | 'Ô' | 'Î' | 'Ï' | 'Œ') {
                    return Some("fr");
                }
            }
        }
        // German umlauts/eszett
        if self.lang_to_voice.contains_key("deu") || self.lang_to_voice.contains_key("de") {
            for c in text.chars() {
                if matches!(c, 'ä' | 'ö' | 'ü' | 'ß' | 'Ä' | 'Ö' | 'Ü') {
                    return Some("de");
                }
            }
        }
        // Spanish inverted punctuation and ñ
        if self.lang_to_voice.contains_key("spa") || self.lang_to_voice.contains_key("es") {
            for c in text.chars() {
                if matches!(c, 'ñ' | 'Ñ' | '¿' | '¡' | 'á' | 'í' | 'ó' | 'ú' | 'Á' | 'Í' | 'Ó' | 'Ú') {
                    return Some("es");
                }
            }
        }
        None
    }

    fn check_common_words(&self, text: &str) -> Option<&'static str> {
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        for w in words {
            let clean = w.trim_matches(|c: char| !c.is_alphabetic() && c != '\'');
            if clean.is_empty() {
                continue;
            }
            if self.lang_to_voice.contains_key("eng") || self.lang_to_voice.contains_key("en") {
                if matches!(clean, "the" | "a" | "an" | "and" | "or" | "is" | "are" | "was" | "were" | "this" | "that" | "hello" | "hi" | "my" | "you" | "your" | "we" | "they" | "i" | "it" | "in" | "on" | "to" | "for" | "with" | "of") {
                    return Some("en");
                }
            }
            if self.lang_to_voice.contains_key("fra") || self.lang_to_voice.contains_key("fr") {
                if matches!(clean, "le" | "la" | "les" | "un" | "une" | "des" | "et" | "ou" | "est" | "sont" | "ce" | "cette" | "ces" | "c'est" | "bonjour" | "salut" | "merci" | "oui" | "non" | "dans" | "pour" | "avec" | "je" | "tu" | "il" | "elle" | "nous" | "vous" | "ils" | "elles") {
                    return Some("fr");
                }
            }
        }
        None
    }

    fn resolve_natural_language(&self, chunk: &str) -> String {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            return self.profile.default_voice_id.clone();
        }

        // 1. Check distinctive characters (diacritics/accents)
        if let Some(code) = self.check_distinctive_script(trimmed) {
            if let Some(voice) = self.lang_to_voice.get(code) {
                return voice.clone();
            }
        }

        // 2. Statistical language detector
        let detected = if let Some(detector) = &self.detector {
            detector.detect(trimmed)
        } else {
            whatlang::detect(trimmed)
        };

        if let Some(info) = detected {
            let code_3 = info.lang().code();
            let code_2 = lang_to_alpha2(info.lang());

            // If reliable, trust the detector
            if info.is_reliable() {
                if let Some(voice) = self.lang_to_voice.get(code_2).or_else(|| self.lang_to_voice.get(code_3)) {
                    return voice.clone();
                }
            } else {
                // If not reliable (short snippet / ambiguous), check common words first
                if let Some(code) = self.check_common_words(trimmed) {
                    if let Some(voice) = self.lang_to_voice.get(code) {
                        return voice.clone();
                    }
                }
                // If detector confidence is reasonably positive (> 0.4)
                if info.confidence() > 0.4 {
                    if let Some(voice) = self.lang_to_voice.get(code_2).or_else(|| self.lang_to_voice.get(code_3)) {
                        return voice.clone();
                    }
                }
            }
        } else {
            // Check common words fallback
            if let Some(code) = self.check_common_words(trimmed) {
                if let Some(voice) = self.lang_to_voice.get(code) {
                    return voice.clone();
                }
            }
        }

        self.profile.default_voice_id.clone()
    }

    fn coalesce_segments(&self, segments: Vec<TextSegment>) -> Vec<TextSegment> {
        let mut merged: Vec<TextSegment> = Vec::with_capacity(segments.len());
        for seg in segments {
            if let Some(last) = merged.last_mut() {
                if last.voice_token_id == seg.voice_token_id {
                    last.text.push_str(&seg.text);
                    continue;
                }
            }
            merged.push(seg);
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LanguageVoiceRule, NumberVoiceRules, PunctuationVoiceRules, VoiceProfile};

    #[test]
    fn test_french_vs_english() {
        let profile = VoiceProfile::new(
            "EnFr Test",
            "DavidVoice",
            vec![
                LanguageVoiceRule {
                    lang_code: "en".to_string(),
                    voice_token_id: "DavidVoice".to_string(),
                },
                LanguageVoiceRule {
                    lang_code: "fr".to_string(),
                    voice_token_id: "HortenseVoice".to_string(),
                },
            ],
            NumberVoiceRules {
                mode: VoiceRuleMode::Unified {
                    voice_token_id: "NumberVoice".to_string(),
                },
            },
            PunctuationVoiceRules {
                mode: VoiceRuleMode::Unified {
                    voice_token_id: "PunctVoice".to_string(),
                },
            },
        );
        let router = VoiceRouter::new(profile);

        let res_en = router.route_text("Hello");
        assert_eq!(res_en[0].voice_token_id, "DavidVoice");

        let res_fr = router.route_text("Bonjour");
        assert_eq!(res_fr[0].voice_token_id, "HortenseVoice");

        let res_fr_sent = router.route_text("Ceci est une phrase en français");
        assert_eq!(res_fr_sent[0].voice_token_id, "HortenseVoice");

        let res_en_sent = router.route_text("This is an English sentence");
        assert_eq!(res_en_sent[0].voice_token_id, "DavidVoice");
    }

    #[test]
    fn test_unified_routing() {
        let profile = VoiceProfile::new(
            "Unified Test",
            "DavidVoice",
            vec![
                LanguageVoiceRule {
                    lang_code: "en".to_string(),
                    voice_token_id: "DavidVoice".to_string(),
                },
                LanguageVoiceRule {
                    lang_code: "ar".to_string(),
                    voice_token_id: "HodaVoice".to_string(),
                },
            ],
            NumberVoiceRules {
                mode: VoiceRuleMode::Unified {
                    voice_token_id: "NumberVoice".to_string(),
                },
            },
            PunctuationVoiceRules {
                mode: VoiceRuleMode::Unified {
                    voice_token_id: "PunctVoice".to_string(),
                },
            },
        );

        let router = VoiceRouter::new(profile);
        let segments = router.route_text("Hello 123 world!");

        assert!(!segments.is_empty());
        // Verify numbers route to NumberVoice
        let has_num = segments.iter().any(|s| s.voice_token_id == "NumberVoice" && s.text.contains("123"));
        assert!(has_num, "123 should route to NumberVoice, got: {:?}", segments);
    }

    #[test]
    fn test_differentiated_numbers_and_punctuation() {
        let profile = VoiceProfile::new(
            "Diff Test",
            "FallbackVoice",
            vec![
                LanguageVoiceRule {
                    lang_code: "en".to_string(),
                    voice_token_id: "EngVoice".to_string(),
                },
                LanguageVoiceRule {
                    lang_code: "ar".to_string(),
                    voice_token_id: "AraVoice".to_string(),
                },
            ],
            NumberVoiceRules {
                mode: VoiceRuleMode::LanguageSpecific {
                    arabic_voice_token_id: "AraNumVoice".to_string(),
                    english_voice_token_id: "EngNumVoice".to_string(),
                    fallback_voice_token_id: Some("OtherNumVoice".to_string()),
                },
            },
            PunctuationVoiceRules {
                mode: VoiceRuleMode::LanguageSpecific {
                    arabic_voice_token_id: "AraPunctVoice".to_string(),
                    english_voice_token_id: "EngPunctVoice".to_string(),
                    fallback_voice_token_id: Some("OtherPunctVoice".to_string()),
                },
            },
        );

        let router = VoiceRouter::new(profile);

        // English digits -> EngNumVoice
        let segs_en = router.route_text("456");
        assert_eq!(segs_en[0].voice_token_id, "EngNumVoice");

        // Arabic digits -> AraNumVoice
        let segs_ar = router.route_text("١٢٣");
        assert_eq!(segs_ar[0].voice_token_id, "AraNumVoice");

        // Arabic punctuation -> AraPunctVoice
        let segs_ar_punct = router.route_text("؟");
        assert_eq!(segs_ar_punct[0].voice_token_id, "AraPunctVoice");

        // English punctuation -> EngPunctVoice
        let segs_en_punct = router.route_text("?!");
        assert_eq!(segs_en_punct[0].voice_token_id, "EngPunctVoice");

        // Arabic text
        let segs_ar_text = router.route_text("مرحبا");
        assert_eq!(segs_ar_text[0].voice_token_id, "AraVoice");
    }
}
