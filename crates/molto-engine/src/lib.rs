use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, RwLock};

use windows::core::{implement, GUID, HRESULT, HSTRING, PCWSTR, Interface};
use windows::Win32::Foundation::{BOOL, CLASS_E_CLASSNOTAVAILABLE, E_FAIL, E_POINTER, S_FALSE, S_OK};
use windows::Win32::Media::Audio::WAVEFORMATEX;
use windows::Win32::Media::Speech::{
    ISpObjectToken, ISpObjectTokenCategory, ISpObjectWithToken,
    ISpObjectWithToken_Impl, ISpStream, ISpTTSEngine, ISpTTSEngineSite, ISpTTSEngine_Impl,
    ISpVoice, SPCAT_VOICES, SPF_DEFAULT, SPVTEXTFRAG,
    SpObjectTokenCategory, SpStream, SpVoice,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{
    CoCreateInstance, CoTaskMemAlloc, IClassFactory, IClassFactory_Impl,
    IStream, CLSCTX_ALL, STREAM_SEEK_SET,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_ALL_ACCESS, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use ahash::AHashMap;
use molto_core::models::VoiceProfile;
use molto_core::router::VoiceRouter;
use molto_core::storage::load_profile;
use windows_core::IUnknownImpl;

pub const CLSID_MOLTO_TTS_ENGINE: GUID = GUID::from_u128(0xB68E639A_7B56_4DA1_8B3E_5EBE63A48B01);
pub const CLSID_MOLTO_TTS_ENGINE_STR: &str = "{B68E639A-7B56-4DA1-8B3E-5EBE63A48B01}";
pub const SPDFID_WAVE_FORMAT_EX: GUID = GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);

static OBJECT_COUNT: AtomicI32 = AtomicI32::new(0);
static LOCK_COUNT: AtomicI32 = AtomicI32::new(0);

struct CachedVoice {
    voice: ISpVoice,
    _sp_stream: ISpStream,
    mem_stream: IStream,
}

#[implement(ISpTTSEngine, ISpObjectWithToken)]
pub struct MoltoTTSEngine {
    token: Mutex<Option<ISpObjectToken>>,
    profile: RwLock<Option<VoiceProfile>>,
    router: RwLock<Option<VoiceRouter>>,
    voices_cache: Mutex<AHashMap<String, CachedVoice>>,
}

fn escape_xml(s: &str) -> String {
    let mut res = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => res.push_str("&amp;"),
            '<' => res.push_str("&lt;"),
            '>' => res.push_str("&gt;"),
            '"' => res.push_str("&quot;"),
            '\'' => res.push_str("&apos;"),
            _ => res.push(c),
        }
    }
    res
}

/// Splits long text into natural streaming speech chunks bounded by punctuation and clauses.
/// This prevents monolithic synthesis calls (which cause latency to scale linearly with text size)
/// and ensures near-instantaneous Time to First Audio (TTFB < 30ms) regardless of text length.
pub fn split_speech_chunks(text: &str) -> Vec<String> {
    if text.len() <= 120 {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::with_capacity(160);

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        current.push(c);

        let cur_len = current.len();

        // 1. Sentence enders: . ! ? ؟ ; \n
        let is_sentence_end = (c == '.' || c == '!' || c == '?' || c == '؟' || c == ';')
            && (i + 1 == len || chars[i + 1].is_whitespace());

        let is_newline = c == '\n';

        // 2. Clause boundaries (if chunk has accumulated at least 50 characters)
        let is_clause_end = (c == ',' || c == '،' || c == ':' || c == '—')
            && (i + 1 == len || chars[i + 1].is_whitespace())
            && cur_len >= 50;

        // 3. Soft length limit at any whitespace (if chunk exceeds 100 characters)
        let is_soft_limit = cur_len >= 100 && c.is_whitespace();

        // 4. Hard safety limit at 150 characters
        let is_hard_limit = cur_len >= 150;

        if is_sentence_end || is_newline || is_clause_end || is_soft_limit || is_hard_limit {
            // Include trailing whitespace in current chunk if next char is whitespace
            if i + 1 < len && chars[i + 1].is_whitespace() && !is_newline {
                i += 1;
                current.push(chars[i]);
            }
            if !current.trim().is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
        }

        i += 1;
    }

    if !current.trim().is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        vec![text.to_string()]
    } else {
        chunks
    }
}

/// Trims excessive leading and trailing silence padding injected by underlying SAPI TTS engines.
/// SAPI engines treat every isolated text string as an independent sentence, padding it with
/// 150-200ms of leading silence and 500-800ms of trailing silence. When switching voices or
/// languages within a sentence (e.g. "File 123"), this results in nearly 1 second of dead silence
/// sounding like an awkward comma/pause.
/// Trimming to a natural speech margin (~20-30ms) produces crisp, natural language transitions.
pub fn trim_utterance_padding(bytes: &[u8], is_last: bool) -> &[u8] {
    if bytes.len() < 4 {
        return bytes;
    }

    let samples: &[i16] = unsafe {
        std::slice::from_raw_parts(bytes.as_ptr() as *const i16, bytes.len() / 2)
    };

    let threshold = 150i16;
    let lead_margin = 441;   // ~20 ms at 22050 Hz
    let trail_margin = 661;  // ~30 ms at 22050 Hz

    let mut first_active = None;
    for (i, &s) in samples.iter().enumerate() {
        if s.abs() > threshold {
            first_active = Some(i);
            break;
        }
    }

    let first_idx = match first_active {
        Some(idx) => idx.saturating_sub(lead_margin),
        None => return bytes,
    };

    let mut last_active = first_idx;
    for (i, &s) in samples.iter().enumerate().rev() {
        if s.abs() > threshold {
            last_active = i;
            break;
        }
    }

    let last_idx = (last_active + trail_margin).min(samples.len());

    if first_idx >= last_idx {
        return bytes;
    }

    let start_byte = first_idx * 2;
    let end_byte = if is_last {
        bytes.len()
    } else {
        last_idx * 2
    };

    &bytes[start_byte..end_byte.min(bytes.len())]
}

enum FragWorkItem {
    Speak {
        text: String,
        rate_adj: i32,
        volume: u32,
        pitch_adj: i32,
    },
    Bookmark {
        num_val: usize,
        ptr: *const u16,
    },
}

impl MoltoTTSEngine {
    pub fn new() -> Self {
        OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
        Self {
            token: Mutex::new(None),
            profile: RwLock::new(None),
            router: RwLock::new(None),
            voices_cache: Mutex::new(AHashMap::new()),
        }
    }

    fn ensure_profile_loaded(&self) {
        if self.router.read().unwrap().is_some() {
            return;
        }

        let token_guard = self.token.lock().unwrap();
        if let Some(token) = token_guard.as_ref() {
            unsafe {
                if let Ok(profile_id_pwstr) = token.GetStringValue(windows::core::w!("ProfileId")) {
                    let id_str = profile_id_pwstr.to_string().unwrap_or_default();
                    if let Ok(prof) = load_profile(&id_str) {
                        let r = VoiceRouter::new(prof.clone());
                        *self.profile.write().unwrap() = Some(prof);
                        *self.router.write().unwrap() = Some(r);
                    }
                }
            }
        }
    }

    fn find_token_by_id(&self, target_id: &str) -> Option<ISpObjectToken> {
        unsafe {
            // 1. Direct path lookup if target_id starts with HKEY_
            if target_id.starts_with("HKEY_") {
                if let Ok(token) = CoCreateInstance::<_, ISpObjectToken>(&windows::Win32::Media::Speech::SpObjectToken, None, CLSCTX_ALL) {
                    let path_w: Vec<u16> = target_id.encode_utf16().chain(std::iter::once(0)).collect();
                    if token.SetId(None, PCWSTR(path_w.as_ptr()), false).is_ok() {
                        return Some(token);
                    }
                }
            }

            // 2. Search SPCAT_VOICES category
            let cat: ISpObjectTokenCategory = CoCreateInstance(
                &SpObjectTokenCategory,
                None,
                CLSCTX_ALL,
            ).ok()?;
            cat.SetId(SPCAT_VOICES, false).ok()?;
            let enum_tokens = cat.EnumTokens(None, None).ok()?;
            let mut count = 0u32;
            enum_tokens.GetCount(&mut count).ok()?;
            for i in 0..count {
                if let Ok(tok) = enum_tokens.Item(i) {
                    if let Ok(id_pwstr) = tok.GetId() {
                        let id_str = id_pwstr.to_string().unwrap_or_default();
                        if id_str.eq_ignore_ascii_case(target_id) {
                            return Some(tok);
                        }
                    }
                    if let Ok(name_pwstr) = tok.GetStringValue(windows::core::w!("")) {
                        let name_str = name_pwstr.to_string().unwrap_or_default();
                        if name_str.eq_ignore_ascii_case(target_id) {
                            return Some(tok);
                        }
                    }
                    if let Ok(attr) = tok.OpenKey(windows::core::w!("Attributes")) {
                        if let Ok(name_val) = attr.GetStringValue(windows::core::w!("Name")) {
                            if name_val.to_string().unwrap_or_default().eq_ignore_ascii_case(target_id) {
                                return Some(tok);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn get_or_create_voice(
        &self,
        voice_id: &str,
        format_guid: &GUID,
        pwaveformatex: *const WAVEFORMATEX,
    ) -> Option<CachedVoice> {
        let mut cache = self.voices_cache.lock().unwrap();
        if let Some(cached) = cache.remove(voice_id) {
            return Some(cached);
        }

        unsafe {
            let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).ok()?;
            if let Some(token) = self.find_token_by_id(voice_id) {
                let _ = voice.SetVoice(&token);
            }

            let mem_stream: IStream = CreateStreamOnHGlobal(None, true).ok()?;
            let sp_stream: ISpStream = CoCreateInstance(
                &SpStream,
                None,
                CLSCTX_ALL,
            ).ok()?;
            sp_stream.SetBaseStream(&mem_stream, format_guid, pwaveformatex).ok()?;
            voice.SetOutput(&sp_stream, false).ok()?;

            Some(CachedVoice {
                voice,
                _sp_stream: sp_stream,
                mem_stream,
            })
        }
    }
}

impl Drop for MoltoTTSEngine {
    fn drop(&mut self) {
        OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ISpObjectWithToken_Impl for MoltoTTSEngine_Impl {
    fn SetObjectToken(&self, ptoken: Option<&ISpObjectToken>) -> windows_core::Result<()> {
        let this = self.get_impl();
        let mut guard = this.token.lock().unwrap();
        *guard = ptoken.cloned();
        drop(guard);
        this.ensure_profile_loaded();
        Ok(())
    }

    fn GetObjectToken(&self) -> windows_core::Result<ISpObjectToken> {
        let this = self.get_impl();
        let guard = this.token.lock().unwrap();
        guard.as_ref().cloned().ok_or_else(|| windows_core::Error::from(E_FAIL))
    }
}

impl ISpTTSEngine_Impl for MoltoTTSEngine_Impl {
    fn GetOutputFormat(
        &self,
        _ptargetfmtid: *const windows_core::GUID,
        _ptargetwaveformatex: *const WAVEFORMATEX,
        poutputformatid: *mut windows_core::GUID,
        ppcomemoutputwaveformatex: *mut *mut WAVEFORMATEX,
    ) -> windows_core::Result<()> {
        if poutputformatid.is_null() || ppcomemoutputwaveformatex.is_null() {
            return Err(windows_core::Error::from(E_POINTER));
        }

        unsafe {
            *poutputformatid = SPDFID_WAVE_FORMAT_EX;
            let mem = CoTaskMemAlloc(std::mem::size_of::<WAVEFORMATEX>()) as *mut WAVEFORMATEX;
            if mem.is_null() {
                return Err(windows_core::Error::from(windows::Win32::Foundation::E_OUTOFMEMORY));
            }

            // Standard PCM: 22050 Hz, 16 bit, Mono
            (*mem).wFormatTag = 1; // WAVE_FORMAT_PCM
            (*mem).nChannels = 1;
            (*mem).nSamplesPerSec = 22050;
            (*mem).wBitsPerSample = 16;
            (*mem).nBlockAlign = 2;
            (*mem).nAvgBytesPerSec = 44100;
            (*mem).cbSize = 0;

            *ppcomemoutputwaveformatex = mem;
        }

        Ok(())
    }

    fn Speak(
        &self,
        _dwspeakflags: u32,
        rguidformatid: *const windows_core::GUID,
        pwaveformatex: *const WAVEFORMATEX,
        ptextfraglist: *const SPVTEXTFRAG,
        poutputsite: Option<&ISpTTSEngineSite>,
    ) -> windows_core::Result<()> {
        let this = self.get_impl();
        let site = poutputsite.ok_or_else(|| windows_core::Error::from(E_POINTER))?;
        this.ensure_profile_loaded();

        // 1. Traverse SPVTEXTFRAG linked list to collect work items (Speak fragments and Bookmarks)
        let mut work_items: Vec<FragWorkItem> = Vec::new();
        let mut cur = ptextfraglist;
        while !cur.is_null() {
            unsafe {
                let frag = &*cur;
                if frag.State.eAction == windows::Win32::Media::Speech::SPVA_Bookmark {
                    let num_val: usize = if !frag.pTextStart.is_null() && frag.ulTextLen > 0 {
                        let slice = std::slice::from_raw_parts(frag.pTextStart.0, frag.ulTextLen as usize);
                        String::from_utf16_lossy(slice).trim().parse().unwrap_or(0)
                    } else {
                        0
                    };
                    work_items.push(FragWorkItem::Bookmark {
                        num_val,
                        ptr: if frag.pTextStart.is_null() { std::ptr::null() } else { frag.pTextStart.0 },
                    });
                } else if frag.State.eAction == windows::Win32::Media::Speech::SPVA_Speak
                    || frag.State.eAction == windows::Win32::Media::Speech::SPVA_SpellOut
                    || frag.State.eAction == windows::Win32::Media::Speech::SPVA_Pronounce
                {
                    if !frag.pTextStart.is_null() && frag.ulTextLen > 0 {
                        let slice = std::slice::from_raw_parts(frag.pTextStart.0, frag.ulTextLen as usize);
                        let s = String::from_utf16_lossy(slice);
                        if !s.is_empty() {
                            work_items.push(FragWorkItem::Speak {
                                text: s,
                                rate_adj: frag.State.RateAdj,
                                volume: frag.State.Volume,
                                pitch_adj: frag.State.PitchAdj.MiddleAdj,
                            });
                        }
                    }
                }
                cur = frag.pNext;
            }
        }

        if work_items.is_empty() {
            return Ok(());
        }

        // Format GUID
        let fmt_guid = if rguidformatid.is_null() {
            SPDFID_WAVE_FORMAT_EX
        } else {
            unsafe { *rguidformatid }
        };

        let mut total_audio_bytes: u64 = 0;
        let mut base_rate = unsafe { site.GetRate().unwrap_or(0) };
        let mut base_volume = unsafe { site.GetVolume().unwrap_or(100) };

        let work_items_len = work_items.len();

        // 2. Process work items
        for (item_idx, item) in work_items.into_iter().enumerate() {
            unsafe {
                let actions = site.GetActions();
                if (actions & windows::Win32::Media::Speech::SPVES_ABORT.0 as u32) != 0 {
                    break;
                }
                if (actions & windows::Win32::Media::Speech::SPVES_RATE.0 as u32) != 0 {
                    base_rate = site.GetRate().unwrap_or(0);
                }
                if (actions & windows::Win32::Media::Speech::SPVES_VOLUME.0 as u32) != 0 {
                    base_volume = site.GetVolume().unwrap_or(100);
                }

                match item {
                    FragWorkItem::Bookmark { num_val, ptr } => {
                        let bitfield = (windows::Win32::Media::Speech::SPEI_TTS_BOOKMARK.0 as u16 as i32)
                            | ((windows::Win32::Media::Speech::SPET_LPARAM_IS_STRING.0 as u16 as i32) << 16);
                        let evt = windows::Win32::Media::Speech::SPEVENT {
                            _bitfield: bitfield,
                            ulStreamNum: 0,
                            ullAudioStreamOffset: total_audio_bytes,
                            wParam: windows::Win32::Foundation::WPARAM(num_val),
                            lParam: windows::Win32::Foundation::LPARAM(ptr as isize),
                        };
                        let _ = site.AddEvents(&evt as *const _, 1);
                    }
                    FragWorkItem::Speak { text, rate_adj, volume, pitch_adj } => {
                        let router_guard = this.router.read().unwrap();
                        let segments = if let Some(r) = router_guard.as_ref() {
                            r.route_text(&text)
                        } else {
                            vec![molto_core::router::TextSegment {
                                text,
                                voice_token_id: String::new(),
                            }]
                        };
                        drop(router_guard);

                        let effective_rate = (base_rate + rate_adj).clamp(-10, 10);
                        let effective_vol = ((base_volume as u32 * volume) / 100).clamp(0, 100) as u16;

                        let segments_len = segments.len();

                        for (seg_idx, seg) in segments.into_iter().enumerate() {
                            if seg.text.trim().is_empty() {
                                continue;
                            }

                            let chunks = split_speech_chunks(&seg.text);
                            let chunks_len = chunks.len();

                            for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
                                if chunk.trim().is_empty() {
                                    continue;
                                }

                                // Check abort before each chunk
                                let actions = site.GetActions();
                                if (actions & windows::Win32::Media::Speech::SPVES_ABORT.0 as u32) != 0 {
                                    return Ok(());
                                }

                                let is_last = (item_idx + 1 == work_items_len)
                                    && (seg_idx + 1 == segments_len)
                                    && (chunk_idx + 1 == chunks_len);

                                if let Some(cached) = this.get_or_create_voice(&seg.voice_token_id, &fmt_guid, pwaveformatex) {
                                    let _ = cached.voice.SetRate(effective_rate);
                                    let _ = cached.voice.SetVolume(effective_vol);

                                    let _ = cached.mem_stream.SetSize(0);
                                    let _ = cached.mem_stream.Seek(0, STREAM_SEEK_SET, None);

                                    let (speak_text, flags) = if pitch_adj != 0 {
                                        let escaped = escape_xml(&chunk);
                                        let xml = format!("<pitch middle=\"{}\">{}</pitch>", pitch_adj.clamp(-10, 10), escaped);
                                        (xml, (SPF_DEFAULT.0 as u32) | (windows::Win32::Media::Speech::SPF_IS_XML.0 as u32))
                                    } else {
                                        (chunk, SPF_DEFAULT.0 as u32)
                                    };

                                    let htext = HSTRING::from(&speak_text);
                                    let _ = cached.voice.Speak(&htext, flags, None);

                                    let _ = cached.mem_stream.Seek(0, STREAM_SEEK_SET, None);
                                    let mut raw_bytes = Vec::new();
                                    let mut buf = vec![0u8; 8192];
                                    loop {
                                        let mut bytes_read = 0u32;
                                        let hr = cached.mem_stream.Read(buf.as_mut_ptr() as _, buf.len() as u32, Some(&mut bytes_read));
                                        if hr.is_err() || bytes_read == 0 {
                                            break;
                                        }
                                        raw_bytes.extend_from_slice(&buf[..bytes_read as usize]);
                                    }

                                    let trimmed = trim_utterance_padding(&raw_bytes, is_last);
                                    if !trimmed.is_empty() {
                                        let _ = site.Write(trimmed.as_ptr() as _, trimmed.len() as u32);
                                        total_audio_bytes += trimmed.len() as u64;
                                    }

                                    let mut cache = this.voices_cache.lock().unwrap();
                                    cache.insert(seg.voice_token_id.clone(), cached);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// COM Class Factory
#[implement(IClassFactory)]
pub struct MoltoClassFactory;

impl IClassFactory_Impl for MoltoClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&windows::core::IUnknown>,
        riid: *const windows::core::GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> windows_core::Result<()> {
        if punkouter.is_some() {
            return Err(windows_core::Error::from(windows::Win32::Foundation::CLASS_E_NOAGGREGATION));
        }
        if ppvobject.is_null() || riid.is_null() {
            return Err(windows_core::Error::from(E_POINTER));
        }

        let engine = MoltoTTSEngine::new();
        let unknown: windows::core::IUnknown = engine.into();
        unsafe { unknown.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, flock: BOOL) -> windows_core::Result<()> {
        if flock.as_bool() {
            LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

// Exported standard COM DLL entry points

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_POINTER;
    }

    unsafe {
        if *rclsid != CLSID_MOLTO_TTS_ENGINE {
            return CLASS_E_CLASSNOTAVAILABLE;
        }

        let factory = MoltoClassFactory;
        let unknown: windows::core::IUnknown = factory.into();
        unknown.query(riid, ppv)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECT_COUNT.load(Ordering::SeqCst) == 0 && LOCK_COUNT.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    if register_engine_in_hkcu().is_ok() {
        S_OK
    } else {
        E_FAIL
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    if unregister_engine_in_hkcu().is_ok() {
        S_OK
    } else {
        E_FAIL
    }
}

pub fn get_current_dll_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    unsafe {
        let mut hmodule = windows::Win32::Foundation::HMODULE::default();
        let _ = windows::Win32::System::LibraryLoader::GetModuleHandleExW(
            windows::Win32::System::LibraryLoader::GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                | windows::Win32::System::LibraryLoader::GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCWSTR(get_current_dll_path as *const () as _),
            &mut hmodule,
        );
        let mut buf = vec![0u16; 1024];
        let len = windows::Win32::System::LibraryLoader::GetModuleFileNameW(
            hmodule,
            &mut buf,
        );
        if len > 0 {
            let path_str = String::from_utf16_lossy(&buf[..len as usize]);
            Ok(std::path::PathBuf::from(path_str))
        } else {
            Err("Failed to get module file name".into())
        }
    }
}

pub fn register_engine_in_hkcu() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let clsid_sub = format!("Software\\Classes\\CLSID\\{}", CLSID_MOLTO_TTS_ENGINE_STR);
        let mut key = HKEY::default();
        let sub_w: Vec<u16> = clsid_sub.encode_utf16().chain(std::iter::once(0)).collect();

        let res = RegCreateKeyExW(
            HKEY_CURRENT_USER,
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
            return Err("Failed to create CLSID registry key".into());
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
            if let Ok(dll_path) = get_current_dll_path() {
                let path_str = format!("{}\0", dll_path.display());
                let path_w: Vec<u16> = path_str.encode_utf16().collect();
                let _ = RegSetValueExW(
                    inproc_key,
                    PCWSTR::null(),
                    0,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(path_w.as_ptr() as *const u8, path_w.len() * 2)),
                );
            }
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

pub fn unregister_engine_in_hkcu() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let clsid_sub = format!("Software\\Classes\\CLSID\\{}\0", CLSID_MOLTO_TTS_ENGINE_STR);
        let sub_w: Vec<u16> = clsid_sub.encode_utf16().collect();
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(sub_w.as_ptr()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_speech_chunks_sentences() {
        let text = "This is sentence one. This is sentence two! And this is sentence three? Yes it is.";
        let chunks = split_speech_chunks(text);
        assert!(!chunks.is_empty());
        let joined: String = chunks.concat();
        assert_eq!(joined.trim(), text.trim());
    }

    #[test]
    fn test_split_speech_chunks_long_paragraph() {
        let long_text = "Microsoft Speech API SAPI 5 is an in-process COM architecture. When synthesizing large paragraphs of text exceeding five hundred characters, monolithic speech synthesis blocks audio output until the entire text is fully rendered into memory. By splitting text at natural punctuation marks, sentence boundaries, and clause boundaries into streaming units of approximately sixty to one hundred characters, Time to First Audio TTFB drops down to twenty milliseconds, and playback starts instantaneously.";
        let chunks = split_speech_chunks(long_text);
        assert!(chunks.len() >= 3, "Should split 500+ character paragraph into multiple chunks, got {}", chunks.len());
        for chunk in &chunks {
            assert!(chunk.len() <= 160, "Chunk should respect maximum length bound: {}", chunk.len());
        }
        let joined: String = chunks.concat();
        assert_eq!(joined.trim(), long_text.trim(), "No text should be lost during chunking");
    }

    #[test]
    fn test_escape_xml() {
        let s = "Hello & <world> \"test\" '123'";
        let escaped = escape_xml(s);
        assert_eq!(escaped, "Hello &amp; &lt;world&gt; &quot;test&quot; &apos;123&apos;");
    }

    #[test]
    fn test_trim_utterance_padding() {
        // Construct synthetic PCM: 200ms silence (4410 samples of 0) + 200ms audio (4410 samples of 500) + 800ms silence (17640 samples of 0)
        let mut samples = vec![0i16; 4410];
        samples.extend(vec![500i16; 4410]);
        samples.extend(vec![0i16; 17640]);

        let bytes: Vec<u8> = samples.iter().flat_map(|&s| s.to_le_bytes()).collect();

        // 1. Intermediate chunk (is_last = false)
        let trimmed_mid = trim_utterance_padding(&bytes, false);
        // Original bytes: (4410 + 4410 + 17640) * 2 = 52920 bytes (~1200ms)
        // Trimmed bytes should be approximately (441 lead + 4410 audio + 661 trail) * 2 = ~11024 bytes (~250ms)
        assert!(trimmed_mid.len() < bytes.len() / 2, "Intermediate chunk should trim trailing silence from 1200ms to ~250ms, got {} bytes vs original {}", trimmed_mid.len(), bytes.len());

        // 2. Final chunk (is_last = true)
        let trimmed_last = trim_utterance_padding(&bytes, true);
        // Leading silence is trimmed, but trailing silence is preserved for natural decay
        assert!(trimmed_last.len() > trimmed_mid.len());
    }
}
