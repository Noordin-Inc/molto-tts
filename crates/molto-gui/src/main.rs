#![windows_subsystem = "windows"]

mod cli;
mod editor;
mod main_window;
mod sapi;

use main_window::MainWindow;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::Threading::CreateMutexW;

fn main() {
    unsafe {
        // Attach to calling console if running from terminal (PowerShell / CMD)
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    // 1. Process CLI arguments if passed
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if let Err(e) = cli::handle_cli_args(&args) {
            eprintln!("CLI Error: {}", e);
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        unsafe {
            CoUninitialize();
        }
        return;
    }

    // 2. Single-instance enforcement for GUI mode
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\MoltoTTS_SingleInstance_Mutex"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(Some(hwnd)) = winsafe::HWND::FindWindow(None, Some("Molto-TTS - Custom SAPI Voice Mixer")) {
                hwnd.ShowWindow(winsafe::co::SW::RESTORE);
                hwnd.SetForegroundWindow();
            }
            CoUninitialize();
            return;
        }
    }

    // 3. Launch GUI
    let app = MainWindow::new();
    if let Err(e) = app.run() {
        eprintln!("Application error: {}", e);
    }

    unsafe {
        CoUninitialize();
    }
}
