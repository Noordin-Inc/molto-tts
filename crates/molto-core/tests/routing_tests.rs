use molto_core::models::{
    LanguageVoiceRule, NumberVoiceRules, PunctuationVoiceRules, VoiceProfile, VoiceRuleMode,
};
use molto_core::router::VoiceRouter;
use std::time::Instant;

#[test]
fn test_mixed_language_and_numbers_end_to_end() {
    let profile = VoiceProfile::new(
        "Mixed David and Hoda",
        "TTS_MS_EN-US_DAVID_11.0",
        vec![
            LanguageVoiceRule {
                lang_code: "en".to_string(),
                voice_token_id: "TTS_MS_EN-US_DAVID_11.0".to_string(),
            },
            LanguageVoiceRule {
                lang_code: "ar".to_string(),
                voice_token_id: "TTS_MS_AR-SA_HODA_11.0".to_string(),
            },
        ],
        NumberVoiceRules {
            mode: VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id: "TTS_MS_AR-SA_HODA_11.0".to_string(),
                english_voice_token_id: "TTS_MS_EN-US_DAVID_11.0".to_string(),
                fallback_voice_token_id: None,
            },
        },
        PunctuationVoiceRules {
            mode: VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id: "TTS_MS_AR-SA_HODA_11.0".to_string(),
                english_voice_token_id: "TTS_MS_EN-US_DAVID_11.0".to_string(),
                fallback_voice_token_id: None,
            },
        },
    );

    let router = VoiceRouter::new(profile);

    // 1. Pure English sentence with numbers
    let segs1 = router.route_text("The temperature is 25 degrees.");
    assert!(!segs1.is_empty());
    assert!(segs1.iter().any(|s| s.text.contains("temperature")));
    assert!(segs1.iter().any(|s| s.text.contains("25")));

    // 2. Pure Arabic sentence with Arabic numerals and punctuation
    let segs2 = router.route_text("مرحباً بكم، اليوم هو ٢٥ من الشهر.");
    assert!(!segs2.is_empty());
    assert!(segs2.iter().any(|s| s.text.contains("مرحباً")));
    assert!(segs2.iter().any(|s| s.text.contains("٢٥")));

    // 3. Performance benchmark: verify sub-millisecond execution time
    let start = Instant::now();
    for _ in 0..100 {
        let _ = router.route_text("Hello world! 123. مرحباً بكم في Molto-TTS.");
    }
    let elapsed = start.elapsed();
    let per_call = elapsed / 100;
    println!("Average routing time per sentence: {:?}", per_call);
    assert!(per_call.as_micros() < 500, "Routing must be lightning fast (<500µs)");
}
