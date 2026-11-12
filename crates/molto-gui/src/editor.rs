use std::sync::{Arc, Mutex};
use winsafe::prelude::*;
use winsafe::{co, gui, AnyResult};

use molto_core::languages::{get_supported_languages, LanguageMeta};
use molto_core::models::{
    LanguageVoiceRule, NumberVoiceRules, PunctuationVoiceRules, VoiceProfile, VoiceRuleMode,
};
use molto_core::storage::save_profile;

use crate::sapi::{
    enumerate_sapi_voices, find_engine_dll_path, register_sapi_voice_token, relaunch_as_admin,
    SapiVoiceInfo,
};

#[derive(Clone)]
pub struct VoiceEditorWindow {
    wnd: gui::WindowModal,
    txt_name: gui::Edit,
    cmb_default_voice: gui::ComboBox,
    cmb_lang: gui::ComboBox,
    cmb_lang_voice: gui::ComboBox,
    btn_add_rule: gui::Button,
    lst_rules: gui::ListView,
    btn_remove_rule: gui::Button,

    cmb_num_unified: gui::ComboBox,
    rad_num_mode: gui::RadioGroup,
    lbl_num_ar: gui::Label,
    cmb_num_arabic: gui::ComboBox,
    lbl_num_en: gui::Label,
    cmb_num_english: gui::ComboBox,

    cmb_punct_unified: gui::ComboBox,
    rad_punct_mode: gui::RadioGroup,
    lbl_punct_ar: gui::Label,
    cmb_punct_arabic: gui::ComboBox,
    lbl_punct_en: gui::Label,
    cmb_punct_english: gui::ComboBox,

    btn_save: gui::Button,
    btn_cancel: gui::Button,

    existing_profile: Arc<Mutex<Option<VoiceProfile>>>,
    available_voices: Arc<Mutex<Vec<SapiVoiceInfo>>>,
    supported_langs: Arc<Mutex<Vec<LanguageMeta>>>,
    current_rules: Arc<Mutex<Vec<LanguageVoiceRule>>>,
    saved: Arc<Mutex<bool>>,
}

impl VoiceEditorWindow {
    pub fn new(profile_to_edit: Option<VoiceProfile>) -> Self {
        let is_edit = profile_to_edit.is_some();
        let title = if is_edit {
            "Molto-TTS - Edit Custom Voice"
        } else {
            "Molto-TTS - Create New Custom Voice"
        };

        let wnd = gui::WindowModal::new(
            gui::WindowModalOpts {
                title,
                size: (680, 680),
                style: co::WS::CAPTION | co::WS::SYSMENU | co::WS::VISIBLE,
                ..Default::default()
            },
        );

        let _lbl_name = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Voice Name:",
                position: (20, 15),
                size: (290, 20),
                ..Default::default()
            },
        );

        let txt_name = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: (20, 35),
                width: 290,
                ..Default::default()
            },
        );

        let _lbl_default = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Default Backing Voice (Fallback):",
                position: (330, 15),
                size: (310, 20),
                ..Default::default()
            },
        );

        let cmb_default_voice = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (330, 35),
                width: 310,
                ..Default::default()
            },
        );

        // Language section
        let _lbl_lang_sec = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Language:",
                position: (20, 70),
                size: (170, 20),
                ..Default::default()
            },
        );

        let cmb_lang = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (20, 90),
                width: 170,
                ..Default::default()
            },
        );

        let _lbl_voice_sec = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Backing SAPI Voice:",
                position: (200, 70),
                size: (260, 20),
                ..Default::default()
            },
        );

        let cmb_lang_voice = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (200, 90),
                width: 260,
                ..Default::default()
            },
        );

        let btn_add_rule = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "+ Add Rule",
                position: (470, 88),
                width: 170,
                ..Default::default()
            },
        );

        let lst_rules = gui::ListView::new(
            &wnd,
            gui::ListViewOpts {
                position: (20, 120),
                size: (620, 105),
                columns: &[
                    ("Language", 180),
                    ("Code (Alpha-2)", 110),
                    ("Backing SAPI Voice", 310),
                ],
                control_style: co::LVS::REPORT | co::LVS::NOSORTHEADER | co::LVS::SHOWSELALWAYS,
                ..Default::default()
            },
        );

        let btn_remove_rule = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "- Remove Selected Rule",
                position: (20, 232),
                width: 180,
                ..Default::default()
            },
        );

        // ===================== NUMBERS SECTION =====================
        let _lbl_num_header = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Numbers Routing Rules:",
                position: (20, 265),
                size: (620, 18),
                ..Default::default()
            },
        );

        let _lbl_num_unified = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Unified Number Voice:",
                position: (20, 285),
                size: (290, 18),
                ..Default::default()
            },
        );

        let cmb_num_unified = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (20, 305),
                width: 290,
                ..Default::default()
            },
        );

        let rad_num_mode = gui::RadioGroup::new(
            &wnd,
            &[
                gui::RadioButtonOpts {
                    text: "Unified Voice",
                    position: (330, 307),
                    size: (110, 20),
                    ..Default::default()
                },
                gui::RadioButtonOpts {
                    text: "Language-Specific (Arabic & English)",
                    position: (450, 307),
                    size: (200, 20),
                    ..Default::default()
                },
            ],
        );

        let lbl_num_ar = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Arabic Numbers Voice:",
                position: (20, 340),
                size: (290, 18),
                ..Default::default()
            },
        );

        let cmb_num_arabic = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (20, 360),
                width: 290,
                ..Default::default()
            },
        );

        let lbl_num_en = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "English Numbers Voice:",
                position: (330, 340),
                size: (310, 18),
                ..Default::default()
            },
        );

        let cmb_num_english = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (330, 360),
                width: 310,
                ..Default::default()
            },
        );

        // ===================== PUNCTUATION SECTION =====================
        let _lbl_punct_header = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Punctuation Marks Routing Rules:",
                position: (20, 395),
                size: (620, 18),
                ..Default::default()
            },
        );

        let _lbl_punct_unified = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Unified Punctuation Voice:",
                position: (20, 415),
                size: (290, 18),
                ..Default::default()
            },
        );

        let cmb_punct_unified = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (20, 435),
                width: 290,
                ..Default::default()
            },
        );

        let rad_punct_mode = gui::RadioGroup::new(
            &wnd,
            &[
                gui::RadioButtonOpts {
                    text: "Unified Voice",
                    position: (330, 437),
                    size: (110, 20),
                    ..Default::default()
                },
                gui::RadioButtonOpts {
                    text: "Language-Specific (Arabic & English)",
                    position: (450, 437),
                    size: (200, 20),
                    ..Default::default()
                },
            ],
        );

        let lbl_punct_ar = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Arabic Punctuation Voice:",
                position: (20, 470),
                size: (290, 18),
                ..Default::default()
            },
        );

        let cmb_punct_arabic = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (20, 490),
                width: 290,
                ..Default::default()
            },
        );

        let lbl_punct_en = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "English Punctuation Voice:",
                position: (330, 470),
                size: (310, 18),
                ..Default::default()
            },
        );

        let cmb_punct_english = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: (330, 490),
                width: 310,
                ..Default::default()
            },
        );

        // ===================== ARCHITECTURE NOTICE & BUTTONS =====================
        let _lbl_arch_notice = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Notice: Voices marked [32-bit only] work inside 32-bit SAPI applications (e.g. 32-bit screen readers), but cannot speak in the 64-bit preview tester.",
                position: (20, 535),
                size: (620, 32),
                ..Default::default()
            },
        );

        let btn_save = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: if is_edit { "Save Voice" } else { "Create & Register Voice" },
                position: (360, 580),
                width: 180,
                ..Default::default()
            },
        );

        let btn_cancel = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "Cancel",
                position: (550, 580),
                width: 90,
                ..Default::default()
            },
        );

        let initial_rules = profile_to_edit
            .as_ref()
            .map(|p| p.language_rules.clone())
            .unwrap_or_default();

        let new_self = Self {
            wnd,
            txt_name,
            cmb_default_voice,
            cmb_lang,
            cmb_lang_voice,
            btn_add_rule,
            lst_rules,
            btn_remove_rule,

            cmb_num_unified,
            rad_num_mode,
            lbl_num_ar,
            cmb_num_arabic,
            lbl_num_en,
            cmb_num_english,

            cmb_punct_unified,
            rad_punct_mode,
            lbl_punct_ar,
            cmb_punct_arabic,
            lbl_punct_en,
            cmb_punct_english,

            btn_save,
            btn_cancel,
            existing_profile: Arc::new(Mutex::new(profile_to_edit)),
            available_voices: Arc::new(Mutex::new(Vec::new())),
            supported_langs: Arc::new(Mutex::new(get_supported_languages())),
            current_rules: Arc::new(Mutex::new(initial_rules)),
            saved: Arc::new(Mutex::new(false)),
        };

        new_self.setup_events();
        new_self
    }

    fn setup_events(&self) {
        let self2 = self.clone();
        self.wnd.on().wm_create(move |_| {
            self2.populate_dropdowns()?;
            self2.load_existing_values()?;
            Ok(0)
        });

        let self2 = self.clone();
        self.rad_num_mode.on().bn_clicked(move || {
            self2.update_num_mode_visibility();
            Ok(())
        });

        let self2 = self.clone();
        self.rad_punct_mode.on().bn_clicked(move || {
            self2.update_punct_mode_visibility();
            Ok(())
        });

        let self2 = self.clone();
        self.btn_add_rule.on().bn_clicked(move || {
            self2.on_add_rule()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_remove_rule.on().bn_clicked(move || {
            self2.on_remove_rule()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_save.on().bn_clicked(move || {
            self2.on_save()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_cancel.on().bn_clicked(move || {
            self2.wnd.close();
            Ok(())
        });
    }

    fn update_num_mode_visibility(&self) {
        let is_specific = self.rad_num_mode.selected_index() == Some(1);
        let sw = if is_specific { co::SW::SHOW } else { co::SW::HIDE };
        self.lbl_num_ar.hwnd().ShowWindow(sw);
        self.cmb_num_arabic.hwnd().ShowWindow(sw);
        self.lbl_num_en.hwnd().ShowWindow(sw);
        self.cmb_num_english.hwnd().ShowWindow(sw);
    }

    fn update_punct_mode_visibility(&self) {
        let is_specific = self.rad_punct_mode.selected_index() == Some(1);
        let sw = if is_specific { co::SW::SHOW } else { co::SW::HIDE };
        self.lbl_punct_ar.hwnd().ShowWindow(sw);
        self.cmb_punct_arabic.hwnd().ShowWindow(sw);
        self.lbl_punct_en.hwnd().ShowWindow(sw);
        self.cmb_punct_english.hwnd().ShowWindow(sw);
    }

    fn populate_dropdowns(&self) -> AnyResult<()> {
        let voices = enumerate_sapi_voices().unwrap_or_default();
        *self.available_voices.lock().unwrap() = voices.clone();

        let voice_names: Vec<String> = voices.iter().map(|v| v.name.clone()).collect();
        let voice_slices: Vec<&str> = voice_names.iter().map(|s| s.as_str()).collect();

        // 1. Default Voice
        self.cmb_default_voice.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_default_voice.items().select(Some(0));
        }

        // 2. Language Voice
        self.cmb_lang_voice.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_lang_voice.items().select(Some(0));
        }

        // 3. Supported Languages (Alpha-2 formatted)
        let langs = self.supported_langs.lock().unwrap();
        let lang_items: Vec<String> = langs
            .iter()
            .map(|l| format!("{} ({})", l.name, l.alpha2))
            .collect();
        let lang_slices: Vec<&str> = lang_items.iter().map(|s| s.as_str()).collect();
        self.cmb_lang.items().add(&lang_slices)?;
        if !lang_slices.is_empty() {
            self.cmb_lang.items().select(Some(0));
        }

        // 4. Numbers ComboBoxes
        self.cmb_num_unified.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_num_unified.items().select(Some(0));
        }
        self.cmb_num_arabic.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_num_arabic.items().select(Some(0));
        }
        self.cmb_num_english.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_num_english.items().select(Some(0));
        }

        // 5. Punctuation ComboBoxes
        self.cmb_punct_unified.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_punct_unified.items().select(Some(0));
        }
        self.cmb_punct_arabic.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_punct_arabic.items().select(Some(0));
        }
        self.cmb_punct_english.items().add(&voice_slices)?;
        if !voice_slices.is_empty() {
            self.cmb_punct_english.items().select(Some(0));
        }

        Ok(())
    }

    fn load_existing_values(&self) -> AnyResult<()> {
        let guard = self.existing_profile.lock().unwrap();
        let voices = self.available_voices.lock().unwrap();

        if let Some(p) = guard.as_ref() {
            self.txt_name.set_text(&p.name)?;

            if let Some(idx) = voices.iter().position(|v| v.id == p.default_voice_id || v.name == p.default_voice_id) {
                self.cmb_default_voice.items().select(Some(idx as u32));
            }

            // Numbers
            match &p.number_rules.mode {
                VoiceRuleMode::Unified { voice_token_id } => {
                    self.rad_num_mode[0].select(true);
                    if let Some(idx) = voices.iter().position(|v| v.id == *voice_token_id || v.name == *voice_token_id) {
                        self.cmb_num_unified.items().select(Some(idx as u32));
                    }
                }
                VoiceRuleMode::LanguageSpecific {
                    arabic_voice_token_id,
                    english_voice_token_id,
                    ..
                } => {
                    self.rad_num_mode[1].select(true);
                    if let Some(idx) = voices.iter().position(|v| v.id == *arabic_voice_token_id || v.name == *arabic_voice_token_id) {
                        self.cmb_num_arabic.items().select(Some(idx as u32));
                    }
                    if let Some(idx) = voices.iter().position(|v| v.id == *english_voice_token_id || v.name == *english_voice_token_id) {
                        self.cmb_num_english.items().select(Some(idx as u32));
                    }
                }
            }

            // Punctuation
            match &p.punctuation_rules.mode {
                VoiceRuleMode::Unified { voice_token_id } => {
                    self.rad_punct_mode[0].select(true);
                    if let Some(idx) = voices.iter().position(|v| v.id == *voice_token_id || v.name == *voice_token_id) {
                        self.cmb_punct_unified.items().select(Some(idx as u32));
                    }
                }
                VoiceRuleMode::LanguageSpecific {
                    arabic_voice_token_id,
                    english_voice_token_id,
                    ..
                } => {
                    self.rad_punct_mode[1].select(true);
                    if let Some(idx) = voices.iter().position(|v| v.id == *arabic_voice_token_id || v.name == *arabic_voice_token_id) {
                        self.cmb_punct_arabic.items().select(Some(idx as u32));
                    }
                    if let Some(idx) = voices.iter().position(|v| v.id == *english_voice_token_id || v.name == *english_voice_token_id) {
                        self.cmb_punct_english.items().select(Some(idx as u32));
                    }
                }
            }
        } else {
            // New voice: default to Unified mode
            self.rad_num_mode[0].select(true);
            self.rad_punct_mode[0].select(true);
        }

        drop(voices);
        drop(guard);

        self.update_num_mode_visibility();
        self.update_punct_mode_visibility();
        self.refresh_rules_list()?;
        Ok(())
    }

    fn refresh_rules_list(&self) -> AnyResult<()> {
        self.lst_rules.items().delete_all()?;
        let rules = self.current_rules.lock().unwrap();
        let langs = self.supported_langs.lock().unwrap();
        let voices = self.available_voices.lock().unwrap();

        for rule in rules.iter() {
            let meta = langs
                .iter()
                .find(|l| l.code.eq_ignore_ascii_case(&rule.lang_code) || l.alpha2.eq_ignore_ascii_case(&rule.lang_code));
            let lang_name = meta.map(|l| l.name).unwrap_or("Custom");
            let alpha2_code = meta.map(|l| l.alpha2).unwrap_or(&rule.lang_code);

            let voice_name = voices
                .iter()
                .find(|v| v.id == rule.voice_token_id || v.name == rule.voice_token_id)
                .map(|v| v.name.as_str())
                .unwrap_or(&rule.voice_token_id);

            self.lst_rules.items().add(
                &[lang_name, alpha2_code, voice_name],
                None,
                (),
            )?;
        }

        Ok(())
    }

    fn on_add_rule(&self) -> AnyResult<()> {
        let lang_idx = self.cmb_lang.items().selected_index();
        let voice_idx = self.cmb_lang_voice.items().selected_index();

        if let (Some(l_idx), Some(v_idx)) = (lang_idx, voice_idx) {
            let langs = self.supported_langs.lock().unwrap();
            let voices = self.available_voices.lock().unwrap();

            if let (Some(meta), Some(v_info)) = (langs.get(l_idx as usize), voices.get(v_idx as usize)) {
                let mut rules = self.current_rules.lock().unwrap();
                rules.retain(|r| {
                    !r.lang_code.eq_ignore_ascii_case(meta.code)
                        && !r.lang_code.eq_ignore_ascii_case(meta.alpha2)
                });
                rules.push(LanguageVoiceRule {
                    lang_code: meta.alpha2.to_string(),
                    voice_token_id: v_info.id.clone(),
                });
            }
        }

        self.refresh_rules_list()?;
        Ok(())
    }

    fn on_remove_rule(&self) -> AnyResult<()> {
        if let Some(item) = self.lst_rules.items().iter_selected().next() {
            let idx = item.index() as usize;
            let mut rules = self.current_rules.lock().unwrap();
            if idx < rules.len() {
                rules.remove(idx);
            }
            drop(rules);
            self.refresh_rules_list()?;
        }
        Ok(())
    }

    fn on_save(&self) -> AnyResult<()> {
        let name = self.txt_name.text()?;
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            self.wnd.hwnd().MessageBox("Voice name cannot be empty.", "Validation Error", co::MB::OK | co::MB::ICONERROR)?;
            return Ok(());
        }

        let def_idx = self.cmb_default_voice.items().selected_index();
        let voices = self.available_voices.lock().unwrap();
        let default_voice = match def_idx.and_then(|i| voices.get(i as usize)) {
            Some(v) => v.id.clone(),
            None => {
                self.wnd.hwnd().MessageBox("Please select a default backing voice.", "Validation Error", co::MB::OK | co::MB::ICONERROR)?;
                return Ok(());
            }
        };

        // Numbers rules
        let num_mode_idx = self.rad_num_mode.selected_index().unwrap_or(0);
        let number_rules = if num_mode_idx == 0 {
            let u_idx = self.cmb_num_unified.items().selected_index().unwrap_or(0);
            let u_voice = voices.get(u_idx as usize).map(|v| v.id.clone()).unwrap_or_else(|| default_voice.clone());
            NumberVoiceRules {
                mode: VoiceRuleMode::Unified { voice_token_id: u_voice },
            }
        } else {
            let ar_idx = self.cmb_num_arabic.items().selected_index().unwrap_or(0);
            let en_idx = self.cmb_num_english.items().selected_index().unwrap_or(0);
            let ar_voice = voices.get(ar_idx as usize).map(|v| v.id.clone()).unwrap_or_else(|| default_voice.clone());
            let en_voice = voices.get(en_idx as usize).map(|v| v.id.clone()).unwrap_or_else(|| default_voice.clone());
            NumberVoiceRules {
                mode: VoiceRuleMode::LanguageSpecific {
                    arabic_voice_token_id: ar_voice,
                    english_voice_token_id: en_voice,
                    fallback_voice_token_id: Some(default_voice.clone()),
                },
            }
        };

        // Punctuation rules
        let punct_mode_idx = self.rad_punct_mode.selected_index().unwrap_or(0);
        let punctuation_rules = if punct_mode_idx == 0 {
            let u_idx = self.cmb_punct_unified.items().selected_index().unwrap_or(0);
            let u_voice = voices.get(u_idx as usize).map(|v| v.id.clone()).unwrap_or_else(|| default_voice.clone());
            PunctuationVoiceRules {
                mode: VoiceRuleMode::Unified { voice_token_id: u_voice },
            }
        } else {
            let ar_idx = self.cmb_punct_arabic.items().selected_index().unwrap_or(0);
            let en_idx = self.cmb_punct_english.items().selected_index().unwrap_or(0);
            let ar_voice = voices.get(ar_idx as usize).map(|v| v.id.clone()).unwrap_or_else(|| default_voice.clone());
            let en_voice = voices.get(en_idx as usize).map(|v| v.id.clone()).unwrap_or_else(|| default_voice.clone());
            PunctuationVoiceRules {
                mode: VoiceRuleMode::LanguageSpecific {
                    arabic_voice_token_id: ar_voice,
                    english_voice_token_id: en_voice,
                    fallback_voice_token_id: Some(default_voice.clone()),
                },
            }
        };

        let rules = self.current_rules.lock().unwrap().clone();

        let mut profile = if let Some(mut existing) = self.existing_profile.lock().unwrap().clone() {
            existing.name = trimmed_name.to_string();
            existing.default_voice_id = default_voice;
            existing.language_rules = rules;
            existing.number_rules = number_rules;
            existing.punctuation_rules = punctuation_rules;
            existing.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            existing
        } else {
            VoiceProfile::new(trimmed_name, default_voice, rules, number_rules, punctuation_rules)
        };

        if let Err(err) = profile.validate() {
            self.wnd.hwnd().MessageBox(&err, "Validation Error", co::MB::OK | co::MB::ICONERROR)?;
            return Ok(());
        }

        // 1. Save JSON profile to disk
        let config_path = match save_profile(&profile) {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("Failed to save voice profile: {}", e);
                self.wnd.hwnd().MessageBox(&msg, "Save Error", co::MB::OK | co::MB::ICONERROR)?;
                return Ok(());
            }
        };

        // 2. Find DLL path
        let dll_path = find_engine_dll_path();

        // 3. Register voice token in SAPI registry
        match register_sapi_voice_token(&profile, &config_path, &dll_path) {
            Ok(true) => {
                self.wnd.hwnd().MessageBox(
                    "Custom voice created and registered system-wide in Windows SAPI (HKLM)!\nIt is now immediately available to all applications.",
                    "Success",
                    co::MB::OK | co::MB::ICONINFORMATION,
                )?;
            }
            Ok(false) => {
                let res = self.wnd.hwnd().MessageBox(
                    "Voice profile was saved to user configuration.\n\nNote: Windows SAPI applications (like SpVoice.GetVoices() and screen readers) require registration in HKLM (Administrator) to be visible system-wide.\n\nWould you like to elevate and register system-wide now?",
                    "Administrator Privileges Recommended",
                    co::MB::YESNO | co::MB::ICONQUESTION,
                )?;
                if res == co::DLGID::YES {
                    let _ = relaunch_as_admin("register-all");
                }
            }
            Err(e) => {
                let msg = format!("Voice profile saved, but registration encountered an error: {}", e);
                self.wnd.hwnd().MessageBox(&msg, "Notice", co::MB::OK | co::MB::ICONWARNING)?;
            }
        }

        *self.saved.lock().unwrap() = true;
        self.wnd.close();
        Ok(())
    }

    pub fn show(&self, parent: &impl GuiParent) -> bool {
        let _ = self.wnd.show_modal(parent);
        *self.saved.lock().unwrap()
    }
}
