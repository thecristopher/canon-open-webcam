//! The tray icon: the app's home while no window is open, and where the engine lives.

use std::cell::RefCell;
use std::process::Command;

use anyhow::Result;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFY_ICON_MESSAGE, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{HSTRING, w};

use crate::control::ControlRequest;
use crate::engine::{Engine, EngineConfig};
use crate::icon::icon_rgba;
use crate::log::log;
use crate::panel::{CREDITS, PanelState};
use crate::setup::QUIT_EVENT;

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_STATUS: usize = 1;
const MENU_OPEN: usize = 10;
const MENU_MIRROR: usize = 11;
const MENU_ABOUT: usize = 12;
const MENU_SETUP: usize = 13;
const MENU_LOG: usize = 14;
const MENU_UNINSTALL: usize = 15;
const MENU_QUIT: usize = 16;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

struct App {
    engine: Engine,
    icon: HICON,
    taskbar_created: u32,
}

/// True when another copy already has the tray in this session.
pub fn already_running() -> bool {
    unsafe {
        let probe = CreateMutexW(None, false, w!("Local\\CanonWebcamApp"));
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        if let Ok(handle) = probe {
            let _ = CloseHandle(handle);
        }
        exists
    }
}

/// Opens the camera window, or brings it forward if it's open already. It's its own process,
/// so closing it gives back all its memory and the tray keeps running.
pub fn open_window() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("window").spawn();
    }
}

pub fn run(config: EngineConfig, show_window: bool) -> Result<()> {
    unsafe {
        // One tray icon per session is plenty
        let _single = CreateMutexW(None, true, w!("Local\\CanonWebcamApp"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log!("already running");
            if show_window {
                open_window();
            }
            return Ok(());
        }
        let quit = CreateEventW(None, true, false, QUIT_EVENT)?;

        let engine = Engine::start(config);
        if let Err(e) = crate::control::serve(engine.handler()) {
            log!("no control pipe, so status and set won't reach this engine: {e:#}");
        }

        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("CanonWebcamTray"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let window = CreateWindowExW(
            Default::default(),
            w!("CanonWebcamTray"),
            // Not "Canon Webcam", so finding the camera window by its title never lands on this hidden one
            w!("Canon Webcam Tray"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        APP.with(|app| {
            *app.borrow_mut() =
                Some(App { engine, icon: make_icon(), taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")) })
        });
        add_icon(window, NIM_ADD);
        SetTimer(Some(window), TIMER_STATUS, 1000, None);
        log!("tray running");
        if show_window {
            open_window();
        }

        // The uninstaller (or a second install) asks us to leave through this event
        let (window_id, quit_id) = (window.0 as isize, quit.0 as isize);
        std::thread::spawn(move || {
            WaitForSingleObject(HANDLE(quit_id as _), INFINITE);
            let _ = PostMessageW(Some(HWND(window_id as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        });

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        // Dropping the engine hands the camera back the way we found it
        APP.with(|app| app.borrow_mut().take());
        let _ = CloseHandle(quit);
        log!("tray stopped");
    }
    Ok(())
}

fn with_engine<T>(work: impl FnOnce(&Engine) -> T) -> Option<T> {
    APP.with(|app| app.borrow().as_ref().map(|a| work(&a.engine)))
}

fn panel() -> PanelState {
    with_engine(|engine| PanelState::from(Some(&engine.status()))).unwrap_or(PanelState::NotRunning)
}

unsafe fn add_icon(window: HWND, action: NOTIFY_ICON_MESSAGE) {
    let icon = APP.with(|app| app.borrow().as_ref().map(|a| a.icon)).unwrap_or_default();
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    let state = panel();
    let tip: Vec<u16> = format!("Canon Webcam\n{}\n{}", state.title(), state.subtitle())
        .encode_utf16()
        .take(data.szTip.len() - 1)
        .collect();
    data.szTip[..tip.len()].copy_from_slice(&tip);
    unsafe {
        let _ = Shell_NotifyIconW(action, &data);
    }
}

unsafe fn show_menu(window: HWND) -> Result<()> {
    let state = panel();
    let mirror = with_engine(|engine| engine.status().options.mirror).unwrap_or(false);
    unsafe {
        let menu = CreatePopupMenu()?;
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, &HSTRING::from(state.title()))?;
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, &HSTRING::from(state.subtitle()))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, MENU_OPEN, w!("Open Canon Webcam"))?;
        SetMenuDefaultItem(menu, MENU_OPEN as u32, 0)?;
        let mirror_flags = if mirror { MF_STRING | MF_CHECKED } else { MF_STRING };
        AppendMenuW(menu, mirror_flags, MENU_MIRROR, w!("Mirror for everyone"))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, MENU_ABOUT, w!("About Canon Webcam"))?;
        AppendMenuW(menu, MF_STRING, MENU_SETUP, w!("Set Up…"))?;
        AppendMenuW(menu, MF_STRING, MENU_LOG, w!("Open Log"))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, MENU_UNINSTALL, w!("Uninstall Canon Webcam…"))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit Canon Webcam"))?;
        let mut point = POINT::default();
        GetCursorPos(&mut point)?;
        // Without this the menu won't close when you click elsewhere
        let _ = SetForegroundWindow(window);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, point.x, point.y, None, window, None);
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        DestroyMenu(menu)?;
    }
    Ok(())
}

fn run_self(argument: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg(argument).spawn();
    }
}

extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let taskbar_created = APP.with(|app| app.borrow().as_ref().map(|a| a.taskbar_created)).unwrap_or(0);
        match message {
            WM_TRAY => {
                match (lparam.0 & 0xFFFF) as u32 {
                    WM_LBUTTONUP => open_window(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let _ = show_menu(window);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_TIMER => {
                add_icon(window, NIM_MODIFY);
                LRESULT(0)
            }
            WM_COMMAND => {
                match wparam.0 & 0xFFFF {
                    MENU_OPEN => open_window(),
                    MENU_MIRROR => {
                        let on = with_engine(|engine| engine.status().options.mirror).unwrap_or(false);
                        with_engine(|engine| {
                            engine.handle(ControlRequest::set("mirror", if on { "off" } else { "on" }))
                        });
                    }
                    MENU_ABOUT => {
                        let text = format!("Canon Webcam {}\n\n{CREDITS}", env!("CARGO_PKG_VERSION"));
                        MessageBoxW(
                            Some(window),
                            &HSTRING::from(text),
                            w!("About Canon Webcam"),
                            MB_OK | MB_ICONINFORMATION,
                        );
                    }
                    MENU_SETUP => run_self("setup"),
                    MENU_LOG => {
                        let _ = Command::new("notepad.exe").arg(crate::log::path()).spawn();
                    }
                    MENU_UNINSTALL => run_self("uninstall"),
                    MENU_QUIT => {
                        let _ = DestroyWindow(window);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_DESTROY => {
                add_icon(window, NIM_DELETE);
                PostQuitMessage(0);
                LRESULT(0)
            }
            // Explorer restarted, so the icon has to be added again
            m if m == taskbar_created && m != 0 => {
                add_icon(window, NIM_ADD);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

/// The tray icon, drawn here so there's no icon file to ship.
fn make_icon() -> HICON {
    const SIZE: i32 = 32;
    // Windows wants BGRA
    let mut pixels = icon_rgba(SIZE as u32);
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE,
                biHeight: -SIZE,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(color) = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            return LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        };
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
        let mask = CreateBitmap(SIZE, SIZE, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        })
        .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}
