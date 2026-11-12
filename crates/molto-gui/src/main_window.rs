use std::sync::{Arc, Mutex};
use winsafe::prelude::*;
use winsafe::{co, gui, AnyResult, HMENU};

use molto_core::languages::code_to_alpha2;
use molto_core::models::{VoiceProfile, VoiceRuleMode};
use molto_core::storage::{delete_profile, list_profiles};

use crate::editor::VoiceEditorWindow;
use crate::sapi::{
    clear_all_profiles_and_tokens, is_elevated, relaunch_as_admin_and_wait, test_speak_voice,
    unregister_sapi_voice_token,
};

const CMD_EDIT: u16 = 2001;
const CMD_TEST: u16 = 2002;
const CMD_DELETE: u16 = 2003;

#[derive(Clone)]
pub struct MainWindow {
    wnd: gui::WindowMain,
    lst_voices: gui::ListView,
    btn_new: gui::Button,
    btn_edit: gui::Button,
    btn_delete: gui::Button,
    btn_refresh: gui::Button,
    btn_clear_all: gui::Button,
    txt_test: gui::Edit,
    btn_test: gui::Button,

    profiles: Arc<Mutex<Vec<VoiceProfile>>>,
}

impl MainWindow {
    pub fn new() -> Self {
        let wnd = gui::WindowMain::new(
            gui::WindowMainOpts {
                title: "Molto-TTS - Custom SAPI Voice Mixer",
                size: (800, 520),
                style: co::WS::CAPTION | co::WS::SYSMENU | co::WS::MINIMIZEBOX | co::WS::VISIBLE,
                ..Default::default()
            },
        );

        let _lbl_header = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Configured Custom SAPI Voices (Registered in Windows SAPI / COM):",
                position: (20, 15),
                size: (500, 20),
                ..Default::default()
            },
        );

        let lst_voices = gui::ListView::new(
            &wnd,
            gui::ListViewOpts {
                position: (20, 38),
                size: (745, 300),
                columns: &[
                    ("Voice Name", 210),
                    ("Languages (Alpha-2)", 170),
                    ("Number / Punctuation Mode", 210),
                    ("Default Voice", 145),
                ],
                control_style: co::LVS::REPORT | co::LVS::NOSORTHEADER | co::LVS::SHOWSELALWAYS,
                ..Default::default()
            },
        );

        let btn_new = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "+ Create New Voice...",
                position: (20, 350),
                width: 150,
                ..Default::default()
            },
        );

        let btn_edit = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "Edit Voice...",
                position: (180, 350),
                width: 110,
                ..Default::default()
            },
        );

        let btn_delete = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "Delete Voice",
                position: (300, 350),
                width: 110,
                ..Default::default()
            },
        );

        let btn_refresh = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "Refresh List",
                position: (420, 350),
                width: 110,
                ..Default::default()
            },
        );

        let btn_clear_all = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "Clear All Voices",
                position: (540, 350),
                width: 130,
                ..Default::default()
            },
        );

        let _lbl_test = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Test Selected Voice (type text below to hear language & punctuation routing):",
                position: (20, 395),
                size: (600, 20),
                ..Default::default()
            },
        );

        let txt_test = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                text: "Hello! Testing 123. مرحباً بكم في Molto-TTS.",
                position: (20, 415),
                width: 610,
                ..Default::default()
            },
        );

        let btn_test = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "Speak Test",
                position: (640, 413),
                width: 125,
                ..Default::default()
            },
        );

        let new_self = Self {
            wnd,
            lst_voices,
            btn_new,
            btn_edit,
            btn_delete,
            btn_refresh,
            btn_clear_all,
            txt_test,
            btn_test,
            profiles: Arc::new(Mutex::new(Vec::new())),
        };

        new_self.setup_events();
        new_self
    }

    fn setup_events(&self) {
        let self2 = self.clone();
        self.wnd.on().wm_create(move |_| {
            self2.load_profiles()?;
            Ok(0)
        });

        let self2 = self.clone();
        self.btn_new.on().bn_clicked(move || {
            self2.on_new_voice()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_edit.on().bn_clicked(move || {
            self2.on_edit_voice()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_delete.on().bn_clicked(move || {
            self2.on_delete_voice()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_refresh.on().bn_clicked(move || {
            self2.load_profiles()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_clear_all.on().bn_clicked(move || {
            self2.on_clear_all()?;
            Ok(())
        });

        let self2 = self.clone();
        self.btn_test.on().bn_clicked(move || {
            self2.on_test_voice()?;
            Ok(())
        });

        let self2 = self.clone();
        self.lst_voices.on().nm_dbl_clk(move |_| {
            self2.on_edit_voice()?;
            Ok(())
        });

        let self2 = self.clone();
        self.lst_voices.on().nm_r_click(move |_| {
            self2.show_context_menu()?;
            Ok(0)
        });
    }

    fn load_profiles(&self) -> AnyResult<()> {
        let profiles = list_profiles().unwrap_or_default();
        *self.profiles.lock().unwrap() = profiles.clone();

        self.lst_voices.items().delete_all()?;
        for p in &profiles {
            let langs = if p.language_rules.is_empty() {
                "Default only".to_string()
            } else {
                p.language_rules
                    .iter()
                    .map(|r| code_to_alpha2(&r.lang_code))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let num_str = match &p.number_rules.mode {
                VoiceRuleMode::Unified { .. } => "Unified",
                VoiceRuleMode::LanguageSpecific { .. } => "Ar/En",
            };
            let punct_str = match &p.punctuation_rules.mode {
                VoiceRuleMode::Unified { .. } => "Unified",
                VoiceRuleMode::LanguageSpecific { .. } => "Ar/En",
            };
            let mode = format!("Num: {}, Punct: {}", num_str, punct_str);

            self.lst_voices.items().add(
                &[&p.name, &langs, &mode, &p.default_voice_id],
                None,
                (),
            )?;
        }

        Ok(())
    }

    fn get_selected_profile(&self) -> Option<VoiceProfile> {
        if let Some(item) = self.lst_voices.items().iter_selected().next() {
            let idx = item.index() as usize;
            let guard = self.profiles.lock().unwrap();
            guard.get(idx).cloned()
        } else {
            None
        }
    }

    fn on_new_voice(&self) -> AnyResult<()> {
        let editor = VoiceEditorWindow::new(None);
        if editor.show(&self.wnd) {
            self.load_profiles()?;
        }
        Ok(())
    }

    fn on_edit_voice(&self) -> AnyResult<()> {
        if let Some(profile) = self.get_selected_profile() {
            let editor = VoiceEditorWindow::new(Some(profile));
            if editor.show(&self.wnd) {
                self.load_profiles()?;
            }
        } else {
            self.wnd.hwnd().MessageBox("Please select a voice to edit.", "Notice", co::MB::OK | co::MB::ICONINFORMATION)?;
        }
        Ok(())
    }

    fn on_delete_voice(&self) -> AnyResult<()> {
        if let Some(profile) = self.get_selected_profile() {
            let msg = format!("Are you sure you want to delete the voice \"{}\"?\nIt will be unregistered from Windows SAPI.", profile.name);
            let res = self.wnd.hwnd().MessageBox(&msg, "Confirm Deletion", co::MB::YESNO | co::MB::ICONQUESTION)?;
            if res == co::DLGID::YES {
                if is_elevated() {
                    let _ = delete_profile(&profile.id);
                    let _ = unregister_sapi_voice_token(&profile.name);
                } else {
                    let _ = relaunch_as_admin_and_wait(&format!("delete --name \"{}\"", profile.name));
                }
                self.load_profiles()?;
            }
        } else {
            self.wnd.hwnd().MessageBox("Please select a voice to delete.", "Notice", co::MB::OK | co::MB::ICONINFORMATION)?;
        }
        Ok(())
    }

    fn on_clear_all(&self) -> AnyResult<()> {
        let msg = "Are you sure you want to delete ALL custom Molto-TTS voices and unregister them from Windows SAPI?\n\nThis will remove all profiles and clean the SAPI registry, giving you a fresh clean slate.";
        let res = self.wnd.hwnd().MessageBox(msg, "Confirm Clear All Voices", co::MB::YESNO | co::MB::ICONWARNING)?;
        if res == co::DLGID::YES {
            if is_elevated() {
                let _ = clear_all_profiles_and_tokens();
            } else {
                let _ = relaunch_as_admin_and_wait("clear-all");
            }
            self.load_profiles()?;
            self.wnd.hwnd().MessageBox("All custom Molto-TTS voices have been removed and unregistered from Windows SAPI.", "Success", co::MB::OK | co::MB::ICONINFORMATION)?;
        }
        Ok(())
    }

    fn on_test_voice(&self) -> AnyResult<()> {
        let text = self.txt_test.text()?;
        if text.trim().is_empty() {
            self.wnd.hwnd().MessageBox("Please enter test text to speak.", "Notice", co::MB::OK | co::MB::ICONINFORMATION)?;
            return Ok(());
        }

        let voice_id = self
            .get_selected_profile()
            .map(|p| p.name)
            .unwrap_or_default();

        if let Err(e) = test_speak_voice(&voice_id, &text) {
            let is_32bit_hint = if voice_id.contains("[32-bit only]") {
                "\n\nNotice: This voice is an x86 (32-bit) engine. 64-bit processes cannot load 32-bit DLLs directly. However, it will speak correctly when used in 32-bit SAPI applications (such as 32-bit screen readers)."
            } else {
                ""
            };
            let msg = format!("Speech playback failed: {}\nEnsure audio output device is available.{}", e, is_32bit_hint);
            self.wnd.hwnd().MessageBox(&msg, "Speech Notice", co::MB::OK | co::MB::ICONWARNING)?;
        }

        Ok(())
    }

    fn show_context_menu(&self) -> AnyResult<()> {
        if self.get_selected_profile().is_none() {
            return Ok(());
        }

        let pt = winsafe::GetCursorPos()?;
        let hmenu = HMENU::CreatePopupMenu()?;
        hmenu.AppendMenu(co::MF::STRING, winsafe::IdMenu::Id(CMD_EDIT), winsafe::BmpPtrStr::from_str("Edit Voice..."))?;
        hmenu.AppendMenu(co::MF::STRING, winsafe::IdMenu::Id(CMD_TEST), winsafe::BmpPtrStr::from_str("Speak Test Voice"))?;
        hmenu.AppendMenu(co::MF::SEPARATOR, winsafe::IdMenu::None, winsafe::BmpPtrStr::None)?;
        hmenu.AppendMenu(co::MF::STRING, winsafe::IdMenu::Id(CMD_DELETE), winsafe::BmpPtrStr::from_str("Delete Voice"))?;

        let res = hmenu.TrackPopupMenu(
            co::TPM::RIGHTBUTTON | co::TPM::RETURNCMD,
            pt,
            self.wnd.hwnd(),
        )?;

        match res {
            Some(cmd) if cmd == CMD_EDIT as i32 => self.on_edit_voice()?,
            Some(cmd) if cmd == CMD_TEST as i32 => self.on_test_voice()?,
            Some(cmd) if cmd == CMD_DELETE as i32 => self.on_delete_voice()?,
            _ => {}
        }

        Ok(())
    }

    pub fn run(&self) -> AnyResult<i32> {
        self.wnd.run_main(None)
    }
}
