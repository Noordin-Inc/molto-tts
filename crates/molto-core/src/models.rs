use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageVoiceRule {
    /// Language code (ISO 639-1 two-letter code preferred, e.g. "en", "ar", "fr", or ISO 639-3 "eng")
    pub lang_code: String,
    /// Backing SAPI Voice Token ID or Name
    pub voice_token_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VoiceRuleMode {
    Unified {
        voice_token_id: String,
    },
    LanguageSpecific {
        arabic_voice_token_id: String,
        english_voice_token_id: String,
        fallback_voice_token_id: Option<String>,
    },
}

// Backward compatibility aliases
pub type PunctuationNumberMode = VoiceRuleMode;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberVoiceRules {
    pub mode: VoiceRuleMode,
}

impl Default for NumberVoiceRules {
    fn default() -> Self {
        Self {
            mode: VoiceRuleMode::Unified {
                voice_token_id: String::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PunctuationVoiceRules {
    pub mode: VoiceRuleMode,
}

impl Default for PunctuationVoiceRules {
    fn default() -> Self {
        Self {
            mode: VoiceRuleMode::Unified {
                voice_token_id: String::new(),
            },
        }
    }
}

// Backward compatibility alias
pub type PunctuationNumberRules = PunctuationVoiceRules;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceProfile {
    /// Unique identifier (e.g. UUID)
    pub id: String,
    /// Friendly user-facing name for SAPI enumeration (e.g. "Molto - David & Hoda Mix")
    pub name: String,
    /// Default fallback SAPI voice token ID
    pub default_voice_id: String,
    /// Language specific routing rules
    pub language_rules: Vec<LanguageVoiceRule>,
    /// Number routing rules
    #[serde(default)]
    pub number_rules: NumberVoiceRules,
    /// Punctuation routing rules
    #[serde(default)]
    pub punctuation_rules: PunctuationVoiceRules,
    /// Legacy field for backward compatibility when deserializing older profiles
    #[serde(default, skip_serializing)]
    pub punctuation_number_rules: Option<NumberVoiceRules>,
    /// Creation timestamp (seconds since UNIX epoch)
    pub created_at: u64,
    /// Last modified timestamp (seconds since UNIX epoch)
    pub updated_at: u64,
}

impl VoiceProfile {
    pub fn new(
        name: impl Into<String>,
        default_voice_id: impl Into<String>,
        language_rules: Vec<LanguageVoiceRule>,
        number_rules: NumberVoiceRules,
        punctuation_rules: PunctuationVoiceRules,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let name_str = name.into();
        let safe_id = format!(
            "{}-{}",
            name_str
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
                .to_lowercase(),
            now
        );

        Self {
            id: safe_id,
            name: name_str,
            default_voice_id: default_voice_id.into(),
            language_rules,
            number_rules,
            punctuation_rules,
            punctuation_number_rules: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn ensure_migrated(&mut self) {
        if let Some(legacy) = self.punctuation_number_rules.take() {
            let num_empty = match &self.number_rules.mode {
                VoiceRuleMode::Unified { voice_token_id } => voice_token_id.is_empty(),
                VoiceRuleMode::LanguageSpecific { arabic_voice_token_id, english_voice_token_id, .. } => {
                    arabic_voice_token_id.is_empty() && english_voice_token_id.is_empty()
                }
            };
            if num_empty {
                self.number_rules = legacy.clone();
            }

            let punct_empty = match &self.punctuation_rules.mode {
                VoiceRuleMode::Unified { voice_token_id } => voice_token_id.is_empty(),
                VoiceRuleMode::LanguageSpecific { arabic_voice_token_id, english_voice_token_id, .. } => {
                    arabic_voice_token_id.is_empty() && english_voice_token_id.is_empty()
                }
            };
            if punct_empty {
                self.punctuation_rules = PunctuationVoiceRules { mode: legacy.mode };
            }
        }
    }

    pub fn validate(&mut self) -> Result<(), String> {
        self.ensure_migrated();
        if self.name.trim().is_empty() {
            return Err("Voice name cannot be empty".to_string());
        }
        if self.default_voice_id.trim().is_empty() {
            return Err("Default fallback voice must be selected".to_string());
        }
        for (i, rule) in self.language_rules.iter().enumerate() {
            if rule.lang_code.trim().is_empty() {
                return Err(format!("Language rule {} has empty language code", i + 1));
            }
            if rule.voice_token_id.trim().is_empty() {
                return Err(format!("Language rule {} ({}) has no backing voice selected", i + 1, rule.lang_code));
            }
        }
        match &self.number_rules.mode {
            VoiceRuleMode::Unified { voice_token_id } => {
                if voice_token_id.trim().is_empty() {
                    return Err("Unified number voice must be selected".to_string());
                }
            }
            VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id,
                english_voice_token_id,
                ..
            } => {
                if arabic_voice_token_id.trim().is_empty() {
                    return Err("Arabic number voice must be selected".to_string());
                }
                if english_voice_token_id.trim().is_empty() {
                    return Err("English number voice must be selected".to_string());
                }
            }
        }
        match &self.punctuation_rules.mode {
            VoiceRuleMode::Unified { voice_token_id } => {
                if voice_token_id.trim().is_empty() {
                    return Err("Unified punctuation voice must be selected".to_string());
                }
            }
            VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id,
                english_voice_token_id,
                ..
            } => {
                if arabic_voice_token_id.trim().is_empty() {
                    return Err("Arabic punctuation voice must be selected".to_string());
                }
                if english_voice_token_id.trim().is_empty() {
                    return Err("English punctuation voice must be selected".to_string());
                }
            }
        }
        Ok(())
    }
}
