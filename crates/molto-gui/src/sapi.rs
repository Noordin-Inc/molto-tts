use std::path::{Path, PathBuf};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Speech::{
    ISpObjectTokenCategory, ISpVoice, SPCAT_VOICES, SPF_DEFAULT,
    SpObjectTokenCategory, SpVoice,
};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken, WaitForSingleObject};
use windows::Win32::UI::Shell::{ShellExecuteExW, ShellExecuteW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_NORMAL;

use molto_core::models::VoiceProfile;

pub const CLSID_MOLTO_TTS_ENGINE_STR: &str = "{B68E639A-7B56-4DA1-8B3E-5EBE63A48B01}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SapiVoiceInfo {
    pub id: String,
    pub name: String,
    pub is_32bit_only: bool,
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
            let mut elevation = TOKEN_ELEVATION::default();
            let mut size = 0u32;
            let res = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            );
            let _ = CloseHandle(token);
            if res.is_ok() {
                return elevation.TokenIsElevated != 0;
            }
        }
    }
    false
}

pub fn relaunch_as_admin(args: &str) -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let exe_w: Vec<u16> = exe.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let args_w: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();

    unsafe {
        let h = ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            None,
            SW_NORMAL,
        );
        if (h.0 as usize) <= 32 {
            return Err("Failed to elevate process via UAC".into());
        }
    }
    Ok(())
}

pub fn relaunch_as_admin_and_wait(args: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let exe_w: Vec<u16> = exe.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let args_w: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();

    unsafe {
        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: PCWSTR(verb_w.as_ptr()),
            lpFile: PCWSTR(exe_w.as_ptr()),
            lpParameters: PCWSTR(args_w.as_ptr()),
            nShow: SW_NORMAL.0,
            ..Default::default()
        };

        if ShellExecuteExW(&mut sei).is_ok() {
            let h_proc = sei.hProcess;
            if !h_proc.is_invalid() {
                let _ = WaitForSingleObject(h_proc, 30000);
                let _ = CloseHandle(h_proc);
                return Ok(true);
            }
        }
    }

    relaunch_as_admin(args)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(true)
}

fn is_file_newer(src: &Path, dest: &Path) -> bool {
    if let (Ok(m_src), Ok(m_dst)) = (src.metadata(), dest.metadata()) {
        if let (Ok(t_src), Ok(t_dst)) = (m_src.modified(), m_dst.modified()) {
            return t_src > t_dst;
        }
    }
    false
}

pub fn find_engine_dll_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let candidate_x64 = dir.join("molto_tts_engine_x64.dll");
            if candidate_x64.exists() {
                return candidate_x64.canonicalize().unwrap_or(candidate_x64);
            }
            let candidate = dir.join("molto_tts_engine.dll");
            if candidate.exists() {
                return candidate.canonicalize().unwrap_or(candidate);
            }
        }
    }
    let candidate = PathBuf::from("target/debug/molto_tts_engine.dll");
    if candidate.exists() {
        if let Ok(abs) = candidate.canonicalize() {
            return abs;
        }
    }
    let local = PathBuf::from("molto_tts_engine.dll");
    if let Ok(abs) = local.canonicalize() {
        abs
    } else {
        local
    }
}

pub fn find_engine_dll_path_x86() -> Option<PathBuf> {
    // 1. Check if molto_tts_engine_x86.dll already exists next to exe or in target/debug/
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let candidate = dir.join("molto_tts_engine_x86.dll");
            if candidate.exists() {
                return candidate.canonicalize().ok().or(Some(candidate));
            }
            let candidate32 = dir.join("molto_tts_engine_32.dll");
            if candidate32.exists() {
                return candidate32.canonicalize().ok().or(Some(candidate32));
            }
        }
    }
    let candidate_dbg_x86 = PathBuf::from("target/debug/molto_tts_engine_x86.dll");
    if candidate_dbg_x86.exists() {
        return candidate_dbg_x86.canonicalize().ok().or(Some(candidate_dbg_x86));
    }

    // 2. Check Cargo's target/i686-pc-windows-msvc/ output directories
    let cargo_i686_dbg = PathBuf::from("target/i686-pc-windows-msvc/debug/molto_tts_engine.dll");
    let cargo_i686_rel = PathBuf::from("target/i686-pc-windows-msvc/release/molto_tts_engine.dll");
    let found = if cargo_i686_dbg.exists() {
        Some(cargo_i686_dbg)
    } else if cargo_i686_rel.exists() {
        Some(cargo_i686_rel)
    } else {
        None
    };

    if let Some(src) = found {
        // Automatically sync to target/debug/molto_tts_engine_x86.dll or next to exe
        // so both 64-bit and 32-bit DLLs sit side-by-side with clear, non-colliding suffixes!
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(dir) = exe_path.parent() {
                let dest = dir.join("molto_tts_engine_x86.dll");
                if !dest.exists() || is_file_newer(&src, &dest) {
                    let _ = std::fs::copy(&src, &dest);
                }
                if dest.exists() {
                    return dest.canonicalize().ok().or(Some(dest));
                }
            }
        }
        let dest = PathBuf::from("target/debug/molto_tts_engine_x86.dll");
        if !dest.exists() || is_file_newer(&src, &dest) {
            let _ = std::fs::copy(&src, &dest);
        }
        if dest.exists() {
            return dest.canonicalize().ok().or(Some(dest));
        }
        return src.canonicalize().ok().or(Some(src));
    }

    let local = PathBuf::from("molto_tts_engine_x86.dll");
    if local.exists() {
        return local.canonicalize().ok().or(Some(local));
    }
    None
}

pub fn enumerate_sapi_voices() -> Result<Vec<SapiVoiceInfo>, Box<dyn std::error::Error>> {
    let mut voices = Vec::new();
    unsafe {
        let cat: ISpObjectTokenCategory = CoCreateInstance(
            &SpObjectTokenCategory,
            None,
            CLSCTX_ALL,
        )?;
        cat.SetId(SPCAT_VOICES, false)?;
        let enum_tokens = cat.EnumTokens(None, None)?;
        let mut count = 0u32;
        enum_tokens.GetCount(&mut count)?;

        let test_voice: Option<ISpVoice> = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).ok();

        for i in 0..count {
            if let Ok(token) = enum_tokens.Item(i) {
                let mut id_str = String::new();
                if let Ok(id_pwstr) = token.GetId() {
                    id_str = id_pwstr.to_string().unwrap_or_default();
                }

                let mut name_str = String::new();
                if let Ok(desc) = token.GetStringValue(windows::core::w!("")) {
                    name_str = desc.to_string().unwrap_or_default();
                }
                if name_str.is_empty() {
                    if let Ok(attr) = token.OpenKey(windows::core::w!("Attributes")) {
                        if let Ok(val) = attr.GetStringValue(windows::core::w!("Name")) {
                            name_str = val.to_string().unwrap_or_default();
                        }
                    }
                }
                if name_str.is_empty() {
                    name_str = id_str.clone();
                }

                // Check if this backing voice can be loaded into the current 64-bit process:
                let is_32bit_only = if let Some(tv) = &test_voice {
                    tv.SetVoice(&token).is_err()
                } else {
                    false
                };

                let display_name = if is_32bit_only {
                    format!("[32-bit only] {}", name_str)
                } else {
                    name_str
                };

                voices.push(SapiVoiceInfo {
                    id: id_str,
                    name: display_name,
                    is_32bit_only,
                });
            }
        }
    }

    Ok(voices)
}

fn write_com_server_keys(root: HKEY, base_path: &str, dll_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let clsid_sub = format!("{}\\{}", base_path, CLSID_MOLTO_TTS_ENGINE_STR);
        let mut key = HKEY::default();
        let sub_w: Vec<u16> = clsid_sub.encode_utf16().chain(std::iter::once(0)).collect();

        let res = RegCreateKeyExW(
            root,
            PCWSTR(sub_w.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            None,
            &mut key,
            None,
        );
        if res.is_err() {
            return Err("Failed to create COM CLSID key".into());
        }

        let desc = "Molto-TTS Voice Engine\0";
        let desc_w: Vec<u16> = desc.encode_utf16().collect();
        let _ = RegSetValueExW(
            key,
            PCWSTR::null(),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(desc_w.as_ptr() as *const u8, desc_w.len() * 2)),
        );

        let mut inproc_key = HKEY::default();
        let inproc_w: Vec<u16> = "InprocServer32\0".encode_utf16().collect();
        let res2 = RegCreateKeyExW(
            key,
            PCWSTR(inproc_w.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            None,
            &mut inproc_key,
            None,
        );
        if res2.is_ok() {
            let path_str = format!("{}\0", dll_path.display());
            let path_w: Vec<u16> = path_str.encode_utf16().collect();
            let _ = RegSetValueExW(
                inproc_key,
                PCWSTR::null(),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(path_w.as_ptr() as *const u8, path_w.len() * 2)),
            );
            let model = "Both\0";
            let model_w: Vec<u16> = model.encode_utf16().collect();
            let name_w: Vec<u16> = "ThreadingModel\0".encode_utf16().collect();
            let _ = RegSetValueExW(
                inproc_key,
                PCWSTR(name_w.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(model_w.as_ptr() as *const u8, model_w.len() * 2)),
            );
            let _ = RegCloseKey(inproc_key);
        }
        let _ = RegCloseKey(key);
    }
    Ok(())
}

fn write_voice_token_keys(
    root: HKEY,
    tokens_base: &str,
    profile: &VoiceProfile,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let token_sub = format!("{}\\{}", tokens_base, profile.name);
        let mut key = HKEY::default();
        let sub_w: Vec<u16> = token_sub.encode_utf16().chain(std::iter::once(0)).collect();

        let res = RegCreateKeyExW(
            root,
            PCWSTR(sub_w.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            None,
            &mut key,
            None,
        );
        if res.is_err() {
            return Err("Failed to create COM CLSID key".into());
        }

        // Default value: friendly display name
        let display = format!("Molto - {}\0", profile.name);
        let display_w: Vec<u16> = display.encode_utf16().collect();
        let _ = RegSetValueExW(
            key,
            PCWSTR::null(),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(display_w.as_ptr() as *const u8, display_w.len() * 2)),
        );

        // 409 (English US LCID value, critical for SAPI SpVoice.GetVoices() to populate GetDescription())
        let lcid_w: Vec<u16> = "409\0".encode_utf16().collect();
        let _ = RegSetValueExW(
            key,
            PCWSTR(lcid_w.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(display_w.as_ptr() as *const u8, display_w.len() * 2)),
        );

        // CLSID value
        let clsid_val = format!("{}\0", CLSID_MOLTO_TTS_ENGINE_STR);
        let clsid_val_w: Vec<u16> = clsid_val.encode_utf16().collect();
        let clsid_name_w: Vec<u16> = "CLSID\0".encode_utf16().collect();
        let _ = RegSetValueExW(
            key,
            PCWSTR(clsid_name_w.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(clsid_val_w.as_ptr() as *const u8, clsid_val_w.len() * 2)),
        );

        // ProfileId value
        let pid_val = format!("{}\0", profile.id);
        let pid_val_w: Vec<u16> = pid_val.encode_utf16().collect();
        let pid_name_w: Vec<u16> = "ProfileId\0".encode_utf16().collect();
        let _ = RegSetValueExW(
            key,
            PCWSTR(pid_name_w.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(pid_val_w.as_ptr() as *const u8, pid_val_w.len() * 2)),
        );

        // ConfigPath value
        let cfg_val = format!("{}\0", config_path.display());
        let cfg_val_w: Vec<u16> = cfg_val.encode_utf16().collect();
        let cfg_name_w: Vec<u16> = "ConfigPath\0".encode_utf16().collect();
        let _ = RegSetValueExW(
            key,
            PCWSTR(cfg_name_w.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(cfg_val_w.as_ptr() as *const u8, cfg_val_w.len() * 2)),
        );

        // Attributes subkey
        let mut attr_key = HKEY::default();
        let attr_name_w: Vec<u16> = "Attributes\0".encode_utf16().collect();
        let res_attr = RegCreateKeyExW(
            key,
            PCWSTR(attr_name_w.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            None,
            &mut attr_key,
            None,
        );
        if res_attr.is_ok() {
            let name_attr = format!("{}\0", profile.name);
            let name_attr_w: Vec<u16> = name_attr.encode_utf16().collect();
            let name_key_w: Vec<u16> = "Name\0".encode_utf16().collect();
            let _ = RegSetValueExW(
                attr_key,
                PCWSTR(name_key_w.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(name_attr_w.as_ptr() as *const u8, name_attr_w.len() * 2)),
            );

            let vendor = "MoltoTTS\0";
            let vendor_w: Vec<u16> = vendor.encode_utf16().collect();
            let vendor_key_w: Vec<u16> = "Vendor\0".encode_utf16().collect();
            let _ = RegSetValueExW(
                attr_key,
                PCWSTR(vendor_key_w.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(vendor_w.as_ptr() as *const u8, vendor_w.len() * 2)),
            );

            let mut lcid_list = vec!["409".to_string()];
            for rule in &profile.language_rules {
                let lcid = match rule.lang_code.to_lowercase().as_str() {
                    "eng" | "en" => "409",
                    "ara" | "ar" => "401",
                    "fra" | "fr" => "40c",
                    "deu" | "de" => "407",
                    "spa" | "es" => "40a",
                    "ita" | "it" => "410",
                    "jpn" | "ja" => "411",
                    "rus" | "ru" => "419",
                    "zho" | "zh" | "cmn" => "804",
                    "tur" | "tr" => "41f",
                    "hin" | "hi" => "439",
                    _ => "",
                };
                if !lcid.is_empty() && !lcid_list.iter().any(|x| x == lcid) {
                    lcid_list.push(lcid.to_string());
                }
            }
            let lang = format!("{}\0", lcid_list.join(";"));
            let lang_w: Vec<u16> = lang.encode_utf16().collect();
            let lang_key_w: Vec<u16> = "Language\0".encode_utf16().collect();
            let _ = RegSetValueExW(
                attr_key,
                PCWSTR(lang_key_w.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(lang_w.as_ptr() as *const u8, lang_w.len() * 2)),
            );

            let gender = "Neutral\0";
            let gender_w: Vec<u16> = gender.encode_utf16().collect();
            let gender_key_w: Vec<u16> = "Gender\0".encode_utf16().collect();
            let _ = RegSetValueExW(
                attr_key,
                PCWSTR(gender_key_w.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(gender_w.as_ptr() as *const u8, gender_w.len() * 2)),
            );

            let age = "Adult\0";
            let age_w: Vec<u16> = age.encode_utf16().collect();
            let age_key_w: Vec<u16> = "Age\0".encode_utf16().collect();
            let _ = RegSetValueExW(
                attr_key,
                PCWSTR(age_key_w.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(age_w.as_ptr() as *const u8, age_w.len() * 2)),
            );

            let _ = RegCloseKey(attr_key);
        }

        let _ = RegCloseKey(key);
    }
    Ok(())
}

pub fn register_sapi_voice_token(
    profile: &VoiceProfile,
    config_path: &Path,
    dll_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    // 1. Always register in HKCU
    let _ = write_com_server_keys(HKEY_CURRENT_USER, "Software\\Classes\\CLSID", dll_path);
    let _ = write_voice_token_keys(HKEY_CURRENT_USER, "Software\\Microsoft\\Speech\\Voices\\Tokens", profile, config_path);

    // 2. If elevated, register system-wide in HKLM and WOW6432Node
    let elevated = is_elevated();
    if elevated {
        // HKLM 64-bit
        write_com_server_keys(HKEY_LOCAL_MACHINE, "SOFTWARE\\Classes\\CLSID", dll_path)?;
        write_voice_token_keys(HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Speech\\Voices\\Tokens", profile, config_path)?;

        // HKLM 32-bit (WOW6432Node)
        // A 32-bit (x86) process CANNOT load a 64-bit DLL (causes 0x800700C1 BadImageFormatException).
        // Only register 32-bit SAPI COM server and tokens if a 32-bit DLL (molto_tts_engine_x86.dll) exists!
        if let Some(dll_32) = find_engine_dll_path_x86() {
            let _ = write_com_server_keys(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Classes\\CLSID", &dll_32);
            let _ = write_voice_token_keys(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Speech\\Voices\\Tokens", profile, config_path);
        } else {
            // Clean up any dangling 32-bit token that pointed to 64-bit DLL to avoid 0x800700c1 crashes in 32-bit apps
            unsafe {
                let wow_sub = format!("SOFTWARE\\WOW6432Node\\Microsoft\\Speech\\Voices\\Tokens\\{}\0", profile.name);
                let wow_w: Vec<u16> = wow_sub.encode_utf16().collect();
                let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(wow_w.as_ptr()));
            }
        }
    }

    Ok(elevated)
}

pub fn unregister_sapi_voice_token(profile_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let token_sub = format!("Software\\Microsoft\\Speech\\Voices\\Tokens\\{}\0", profile_name);
        let sub_w: Vec<u16> = token_sub.encode_utf16().collect();
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(sub_w.as_ptr()));

        if is_elevated() {
            let hklm_sub = format!("SOFTWARE\\Microsoft\\Speech\\Voices\\Tokens\\{}\0", profile_name);
            let hklm_w: Vec<u16> = hklm_sub.encode_utf16().collect();
            let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(hklm_w.as_ptr()));

            let wow_sub = format!("SOFTWARE\\WOW6432Node\\Microsoft\\Speech\\Voices\\Tokens\\{}\0", profile_name);
            let wow_w: Vec<u16> = wow_sub.encode_utf16().collect();
            let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(wow_w.as_ptr()));
        }
    }
    Ok(())
}

pub fn clear_all_profiles_and_tokens() -> Result<usize, Box<dyn std::error::Error>> {
    let profiles = molto_core::storage::list_profiles().unwrap_or_default();
    let mut count = 0;
    for p in &profiles {
        let _ = unregister_sapi_voice_token(&p.name);
        let _ = molto_core::storage::delete_profile(&p.id);
        count += 1;
    }

    // Additionally delete any lingering Molto profile files in profiles directory
    if let Ok(dir) = molto_core::storage::ensure_profiles_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(count)
}

pub fn register_all_saved_profiles() -> Result<usize, Box<dyn std::error::Error>> {
    let dll_path = find_engine_dll_path();
    let _ = find_engine_dll_path_x86(); // Automatically sync 32-bit engine DLL if present
    let profiles = molto_core::storage::list_profiles()?;
    let mut count = 0;
    for profile in &profiles {
        let config_path = molto_core::storage::get_profile_path(&profile.id);
        let _ = register_sapi_voice_token(profile, &config_path, &dll_path)?;
        count += 1;
    }
    Ok(count)
}

pub fn test_speak_voice(voice_token_id: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)?;
        if !voice_token_id.is_empty() {
            // Direct token resolution if starts with HKEY_
            if voice_token_id.starts_with("HKEY_") {
                if let Ok(tok) = CoCreateInstance::<_, windows::Win32::Media::Speech::ISpObjectToken>(&windows::Win32::Media::Speech::SpObjectToken, None, CLSCTX_ALL) {
                    let path_w: Vec<u16> = voice_token_id.encode_utf16().chain(std::iter::once(0)).collect();
                    if tok.SetId(None, PCWSTR(path_w.as_ptr()), false).is_ok() {
                        let _ = voice.SetVoice(&tok);
                        let htext = HSTRING::from(text);
                        voice.Speak(&htext, SPF_DEFAULT.0 as u32, None)?;
                        return Ok(());
                    }
                }
            }

            // Search SPCAT_VOICES
            let cat: ISpObjectTokenCategory = CoCreateInstance(
                &SpObjectTokenCategory,
                None,
                CLSCTX_ALL,
            )?;
            cat.SetId(SPCAT_VOICES, false)?;
            let enum_tokens = cat.EnumTokens(None, None)?;
            let mut count = 0u32;
            enum_tokens.GetCount(&mut count)?;
            for i in 0..count {
                if let Ok(tok) = enum_tokens.Item(i) {
                    let mut found = false;
                    if let Ok(id) = tok.GetId() {
                        if id.to_string().unwrap_or_default().eq_ignore_ascii_case(voice_token_id) {
                            found = true;
                        }
                    }
                    if !found {
                        if let Ok(name) = tok.GetStringValue(windows::core::w!("")) {
                            if name.to_string().unwrap_or_default().eq_ignore_ascii_case(voice_token_id) {
                                found = true;
                            }
                        }
                    }
                    if !found {
                        if let Ok(attr) = tok.OpenKey(windows::core::w!("Attributes")) {
                            if let Ok(name_val) = attr.GetStringValue(windows::core::w!("Name")) {
                                if name_val.to_string().unwrap_or_default().eq_ignore_ascii_case(voice_token_id) {
                                    found = true;
                                }
                            }
                        }
                    }
                    if found {
                        let _ = voice.SetVoice(&tok);
                        break;
                    }
                }
            }
        }

        let htext = HSTRING::from(text);
        voice.Speak(&htext, SPF_DEFAULT.0 as u32, None)?;
    }
    Ok(())
}
