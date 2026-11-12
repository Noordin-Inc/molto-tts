use molto_core::languages::code_to_alpha2;
use molto_core::models::{
    LanguageVoiceRule, NumberVoiceRules, PunctuationVoiceRules, VoiceProfile, VoiceRuleMode,
};
use molto_core::storage::{delete_profile, list_profiles, save_profile};
use crate::sapi::{
    clear_all_profiles_and_tokens, enumerate_sapi_voices, find_engine_dll_path,
    find_engine_dll_path_x86, is_elevated, register_all_saved_profiles,
    register_sapi_voice_token, test_speak_voice, unregister_sapi_voice_token,
};

pub fn handle_cli_args(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    if args.is_empty() {
        return Ok(false); // No CLI args -> launch GUI
    }

    match args[0].as_str() {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(true)
        }
        "list" => {
            cmd_list()?;
            Ok(true)
        }
        "create" => {
            cmd_create(&args[1..])?;
            Ok(true)
        }
        "delete" => {
            cmd_delete(&args[1..])?;
            Ok(true)
        }
        "clear-all" => {
            cmd_clear_all()?;
            Ok(true)
        }
        "register-all" => {
            cmd_register_all()?;
            Ok(true)
        }
        "test" => {
            cmd_test(&args[1..])?;
            Ok(true)
        }
        unknown => {
            eprintln!("Unknown command: '{}'. Run 'molto-tts help' for usage.", unknown);
            Ok(true)
        }
    }
}

fn print_help() {
    println!(r#"Molto-TTS - Command Line Interface

USAGE:
    molto-tts                          Launch the graphical user interface
    molto-tts list                     List saved voice profiles and installed SAPI voices
    molto-tts create [OPTIONS]         Create and register a custom voice
    molto-tts delete --name <NAME>     Delete and unregister a custom voice
    molto-tts clear-all                Unregister all custom voices and delete profiles
    molto-tts register-all             Register all saved profiles into SAPI (HKLM + HKCU)
    molto-tts test --name <NAME> --text <TEXT>
                                       Speak test text with a custom or SAPI voice

OPTIONS FOR 'create':
    --name <NAME>              Voice name (required)
    --default <VOICE_ID>       Default/fallback backing SAPI voice (required)
    --lang <CODE=VOICE_ID>     Map ISO language code (e.g. en=David, ar=Hoda) (can repeat)
    --num-unified <VOICE_ID>   Unified backing voice for numbers
    --num-ar <VOICE_ID>        Arabic numbers backing voice
    --num-en <VOICE_ID>        English numbers backing voice
    --punct-unified <VOICE_ID> Unified backing voice for punctuation marks
    --punct-ar <VOICE_ID>      Arabic punctuation backing voice
    --punct-en <VOICE_ID>      English punctuation backing voice
    --unified <VOICE_ID>       Unified voice for both numbers and punctuation
    --ar-voice <VOICE_ID>      Arabic voice for both numbers and punctuation
    --en-voice <VOICE_ID>      English voice for both numbers and punctuation

NOTE:
    To register voices system-wide in HKLM so they appear in SAPI client applications
    (e.g., PowerShell SpVoice.GetVoices, NVDA, Narrator), run this command in an
    elevated (Administrator) terminal.
"#);
}

fn cmd_list() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Custom Molto-TTS Voices ===");
    let profiles = list_profiles()?;
    if profiles.is_empty() {
        println!("  (No custom voices created yet)");
    } else {
        for (i, p) in profiles.iter().enumerate() {
            println!("  [{}] Name: {}", i + 1, p.name);
            println!("      ID: {}", p.id);
            println!("      Default Voice: {}", p.default_voice_id);
            let langs: Vec<String> = p
                .language_rules
                .iter()
                .map(|r| format!("{}={}", code_to_alpha2(&r.lang_code), r.voice_token_id))
                .collect();
            let langs_str = if langs.is_empty() { "None".to_string() } else { langs.join(", ") };
            println!("      Languages (Alpha-2): {}", langs_str);

            match &p.number_rules.mode {
                VoiceRuleMode::Unified { voice_token_id } => {
                    println!("      Numbers: Unified ({})", voice_token_id);
                }
                VoiceRuleMode::LanguageSpecific { arabic_voice_token_id, english_voice_token_id, .. } => {
                    println!("      Numbers: Language-Specific (Ar: {}, En: {})", arabic_voice_token_id, english_voice_token_id);
                }
            }

            match &p.punctuation_rules.mode {
                VoiceRuleMode::Unified { voice_token_id } => {
                    println!("      Punctuation: Unified ({})", voice_token_id);
                }
                VoiceRuleMode::LanguageSpecific { arabic_voice_token_id, english_voice_token_id, .. } => {
                    println!("      Punctuation: Language-Specific (Ar: {}, En: {})", arabic_voice_token_id, english_voice_token_id);
                }
            }
        }
    }

    println!("\n=== Installed Windows SAPI Voices ===");
    match enumerate_sapi_voices() {
        Ok(voices) => {
            if voices.is_empty() {
                println!("  (No SAPI voices found via SPCAT_VOICES)");
            }
            for (i, v) in voices.iter().enumerate() {
                println!("  [{}] {}", i + 1, v.name);
                println!("      Token ID: {}", v.id);
            }
        }
        Err(e) => {
            println!("  Error enumerating voices: {}", e);
        }
    }

    println!("\nElevation status: {}", if is_elevated() { "Administrator (Elevated)" } else { "Standard User" });
    Ok(())
}

fn cmd_clear_all() -> Result<(), Box<dyn std::error::Error>> {
    println!("Clearing all custom Molto-TTS voices and unregistering from SAPI...");
    let count = clear_all_profiles_and_tokens()?;
    println!("Cleared {} voice profile(s) and unregistered from SAPI. (Elevated: {})", count, is_elevated());
    if !is_elevated() {
        println!("Note: Tokens registered in HKLM/WOW6432Node require running 'molto-tts clear-all' in an Administrator prompt to be fully deleted.");
    }
    Ok(())
}

fn cmd_create(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut name = String::new();
    let mut default_voice = String::new();
    let mut lang_rules = Vec::new();

    let mut num_unified = None;
    let mut num_ar = None;
    let mut num_en = None;

    let mut punct_unified = None;
    let mut punct_ar = None;
    let mut punct_en = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--name" if i + 1 < args.len() => {
                name = args[i + 1].clone();
                i += 2;
            }
            "--default" if i + 1 < args.len() => {
                default_voice = args[i + 1].clone();
                i += 2;
            }
            "--lang" if i + 1 < args.len() => {
                let pair = &args[i + 1];
                if let Some((code, v_id)) = pair.split_once('=') {
                    lang_rules.push(LanguageVoiceRule {
                        lang_code: code.trim().to_string(),
                        voice_token_id: v_id.trim().to_string(),
                    });
                }
                i += 2;
            }
            "--num-unified" if i + 1 < args.len() => {
                num_unified = Some(args[i + 1].clone());
                i += 2;
            }
            "--num-ar" if i + 1 < args.len() => {
                num_ar = Some(args[i + 1].clone());
                i += 2;
            }
            "--num-en" if i + 1 < args.len() => {
                num_en = Some(args[i + 1].clone());
                i += 2;
            }
            "--punct-unified" if i + 1 < args.len() => {
                punct_unified = Some(args[i + 1].clone());
                i += 2;
            }
            "--punct-ar" if i + 1 < args.len() => {
                punct_ar = Some(args[i + 1].clone());
                i += 2;
            }
            "--punct-en" if i + 1 < args.len() => {
                punct_en = Some(args[i + 1].clone());
                i += 2;
            }
            "--unified" if i + 1 < args.len() => {
                let v = args[i + 1].clone();
                num_unified = Some(v.clone());
                punct_unified = Some(v);
                i += 2;
            }
            "--ar-voice" if i + 1 < args.len() => {
                let v = args[i + 1].clone();
                num_ar = Some(v.clone());
                punct_ar = Some(v);
                i += 2;
            }
            "--en-voice" if i + 1 < args.len() => {
                let v = args[i + 1].clone();
                num_en = Some(v.clone());
                punct_en = Some(v);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    if name.trim().is_empty() {
        eprintln!("Error: --name <NAME> is required.");
        return Ok(());
    }
    if default_voice.trim().is_empty() {
        eprintln!("Error: --default <VOICE_ID> is required.");
        return Ok(());
    }

    let number_rules = if let Some(u) = num_unified {
        NumberVoiceRules {
            mode: VoiceRuleMode::Unified { voice_token_id: u },
        }
    } else if num_ar.is_some() || num_en.is_some() {
        NumberVoiceRules {
            mode: VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id: num_ar.unwrap_or_else(|| default_voice.clone()),
                english_voice_token_id: num_en.unwrap_or_else(|| default_voice.clone()),
                fallback_voice_token_id: Some(default_voice.clone()),
            },
        }
    } else {
        NumberVoiceRules {
            mode: VoiceRuleMode::Unified {
                voice_token_id: default_voice.clone(),
            },
        }
    };

    let punctuation_rules = if let Some(u) = punct_unified {
        PunctuationVoiceRules {
            mode: VoiceRuleMode::Unified { voice_token_id: u },
        }
    } else if punct_ar.is_some() || punct_en.is_some() {
        PunctuationVoiceRules {
            mode: VoiceRuleMode::LanguageSpecific {
                arabic_voice_token_id: punct_ar.unwrap_or_else(|| default_voice.clone()),
                english_voice_token_id: punct_en.unwrap_or_else(|| default_voice.clone()),
                fallback_voice_token_id: Some(default_voice.clone()),
            },
        }
    } else {
        PunctuationVoiceRules {
            mode: VoiceRuleMode::Unified {
                voice_token_id: default_voice.clone(),
            },
        }
    };

    let mut profile = VoiceProfile::new(&name, &default_voice, lang_rules, number_rules, punctuation_rules);
    profile.validate().map_err(|e| format!("Validation error: {}", e))?;

    let config_path = save_profile(&profile)?;
    println!("Saved profile JSON to: {}", config_path.display());

    let dll_path = find_engine_dll_path();
    println!("Using COM engine DLL: {}", dll_path.display());

    let is_admin = register_sapi_voice_token(&profile, &config_path, &dll_path)?;
    if is_admin {
        println!("Success: Custom voice '{}' registered system-wide in HKLM and HKCU!", profile.name);
    } else {
        println!("Warning: Registered in user profile (HKCU). To register system-wide in HKLM for all apps, rerun this command in an Administrator terminal.");
    }

    Ok(())
}

fn cmd_delete(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut name = String::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--name" && i + 1 < args.len() {
            name = args[i + 1].clone();
            i += 2;
        } else {
            i += 1;
        }
    }

    if name.is_empty() {
        eprintln!("Error: --name <NAME> is required.");
        return Ok(());
    }

    let profiles = list_profiles()?;
    if let Some(p) = profiles.iter().find(|p| p.name.eq_ignore_ascii_case(&name)) {
        delete_profile(&p.id)?;
        unregister_sapi_voice_token(&p.name)?;
        println!("Deleted profile '{}' and unregistered from SAPI.", p.name);
    } else {
        eprintln!("Profile '{}' not found.", name);
    }
    Ok(())
}

fn cmd_register_all() -> Result<(), Box<dyn std::error::Error>> {
    println!("Registering all saved profiles into SAPI...");
    let count = register_all_saved_profiles()?;
    println!("Registered {} profile(s). (Elevated: {})", count, is_elevated());
    if is_elevated() {
        if let Some(path_32) = find_engine_dll_path_x86() {
            println!("  x86 (32-bit): Registered WOW6432Node engine ({})", path_32.display());
        } else {
            println!("  x86 (32-bit): 32-bit engine DLL not found. Cleaned WOW6432Node to prevent 0x800700C1 crashes in 32-bit apps.");
            println!("  Tip: To support 32-bit apps, compile x86 DLL:");
            println!("       rustup target add i686-pc-windows-msvc");
            println!("       cargo build -p molto-engine --target i686-pc-windows-msvc");
        }
        println!("  x64 (64-bit): Registered system-wide in HKLM.");
    } else {
        println!("  WARNING: Not elevated. Voices were registered in HKCU only.");
        println!("  To make voices visible system-wide, run this in an Administrator command prompt.");
    }
    Ok(())
}

fn cmd_test(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut name = String::new();
    let mut text = "Hello world! Testing 123.".to_string();

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--name" && i + 1 < args.len() {
            name = args[i + 1].clone();
            i += 2;
        } else if args[i] == "--text" && i + 1 < args.len() {
            text = args[i + 1].clone();
            i += 2;
        } else {
            i += 1;
        }
    }

    println!("Speaking text using voice '{}': \"{}\"", name, text);
    test_speak_voice(&name, &text)?;
    println!("Playback complete.");
    Ok(())
}
