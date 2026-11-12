use crate::models::VoiceProfile;
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_profiles_dir() -> PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("io", "noordin", "MoltoTTS") {
        proj.config_dir().join("profiles")
    } else {
        PathBuf::from("profiles")
    }
}

pub fn ensure_profiles_dir() -> std::io::Result<PathBuf> {
    let dir = get_profiles_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

pub fn get_profile_path(id: &str) -> PathBuf {
    get_profiles_dir().join(format!("{}.json", id))
}

pub fn save_profile(profile: &VoiceProfile) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = ensure_profiles_dir()?;
    let path = dir.join(format!("{}.json", profile.id));
    let json = serde_json::to_string_pretty(profile)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_profile(id: &str) -> Result<VoiceProfile, Box<dyn std::error::Error>> {
    let path = get_profile_path(id);
    load_profile_from_path(&path)
}

pub fn load_profile_from_path(path: &Path) -> Result<VoiceProfile, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut profile: VoiceProfile = serde_json::from_str(&content)?;
    profile.ensure_migrated();
    Ok(profile)
}

pub fn list_profiles() -> Result<Vec<VoiceProfile>, Box<dyn std::error::Error>> {
    let dir = ensure_profiles_dir()?;
    let mut profiles = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(profile) = load_profile_from_path(&path) {
                    profiles.push(profile);
                }
            }
        }
    }
    profiles.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(profiles)
}

pub fn delete_profile(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_profile_path(id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LanguageVoiceRule;

    #[test]
    fn test_profile_serialization() {
        let profile = VoiceProfile::new(
            "Test Voice",
            "David",
            vec![LanguageVoiceRule {
                lang_code: "en".to_string(),
                voice_token_id: "David".to_string(),
            }],
            crate::models::NumberVoiceRules {
                mode: crate::models::VoiceRuleMode::Unified {
                    voice_token_id: "David".to_string(),
                },
            },
            crate::models::PunctuationVoiceRules {
                mode: crate::models::VoiceRuleMode::Unified {
                    voice_token_id: "David".to_string(),
                },
            },
        );

        let json = serde_json::to_string(&profile).unwrap();
        let mut deserialized: VoiceProfile = serde_json::from_str(&json).unwrap();
        deserialized.ensure_migrated();
        assert_eq!(profile.name, deserialized.name);
        assert_eq!(profile.default_voice_id, deserialized.default_voice_id);
    }
}
