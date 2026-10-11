use std::cell::RefCell;
use std::process::Command;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{HSTRING, w};

use crate::engine::Engine;
use crate::log::log;
use crate::setup::QUIT_EVENT;

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_STATUS: usize = 1;
const MENU_LOG: usize = 10;
const MENU_UNINSTALL: usize = 11;
const MENU_QUIT: usize = 12;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

struct App {
    engine: Engine,
    icon: HICON,
    taskbar_created: u32,
}

pub fn run() -> Result<()> {
    unsafe {
        // One tray icon per session is plenty
        let _single = CreateMutexW(None, true, w!("Local\\CanonWebcamApp"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log!("already running");
            return Ok(());
        }
        let quit = CreateEventW(None, true, false, QUIT_EVENT)?;

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
            w!("Canon Webcam"),
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
            *app.borrow_mut() = Some(App {
                engine: Engine::start(),
                icon: make_icon(),
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            })
        });
        add_icon(window, NIM_ADD);
        SetTimer(Some(window), TIMER_STATUS, 1000, None);
        log!("tray running");

        // The uninstaller (or a second install) asks us to leave through this event
        let (window_id, quit_id) = (window.0 as isize, quit.0 as isize);
        std::thread::spawn(move || {
            let quit = windows::Win32::Foundation::HANDLE(quit_id as _);
            WaitForSingleObject(quit, INFINITE);
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

fn status_text() -> String {
    APP.with(|app| {
        app.borrow()
            .as_ref()
            .map(|a| a.engine.status().describe())
            .unwrap_or_default()
    })
}

unsafe fn add_icon(window: HWND, action: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE) {
    let icon = APP
        .with(|app| app.borrow().as_ref().map(|a| a.icon))
        .unwrap_or_default();
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    let tip: Vec<u16> = format!("Canon Webcam\n{}", status_text())
        .encode_utf16()
        .take(data.szTip.len() - 1)
        .collect();
    data.szTip[..tip.len()].copy_from_slice(&tip);
    unsafe {
        let _ = Shell_NotifyIconW(action, &data);
    }
}

unsafe fn show_menu(window: HWND) -> Result<()> {
    unsafe {
        let menu = CreatePopupMenu()?;
        AppendMenuW(
            menu,
            MF_STRING | MF_GRAYED,
            0,
            &HSTRING::from(status_text()),
        )?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, MENU_LOG, w!("Open log folder"))?;
        AppendMenuW(
            menu,
            MF_STRING,
            MENU_UNINSTALL,
            w!("Uninstall Canon Webcam…"),
        )?;
        AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit"))?;
        let mut point = POINT::default();
        GetCursorPos(&mut point)?;
        // Without this the menu won't close when you click elsewhere
        let _ = SetForegroundWindow(window);
        let _ = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            None,
            window,
            None,
        );
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        DestroyMenu(menu)?;
    }
    Ok(())
}

extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let taskbar_created = APP
            .with(|app| app.borrow().as_ref().map(|a| a.taskbar_created))
            .unwrap_or(0);
        match message {
            WM_TRAY => {
                let event = (lparam.0 & 0xFFFF) as u32;
                if event == WM_RBUTTONUP || event == WM_LBUTTONUP || event == WM_CONTEXTMENU {
                    let _ = show_menu(window);
                }
                LRESULT(0)
            }
            WM_TIMER => {
                add_icon(window, NIM_MODIFY);
                LRESULT(0)
            }
            WM_COMMAND => {
                match wparam.0 & 0xFFFF {
                    MENU_LOG => {
                        let _ = Command::new("explorer.exe")
                            .arg(crate::log::folder())
                            .spawn();
                    }
                    MENU_UNINSTALL => {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = Command::new(exe).arg("uninstall").spawn();
                        }
                    }
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

/// A little white camera on a transparent background, drawn here so there's no icon file to ship.
fn make_icon() -> HICON {
    const SIZE: i32 = 32;
    let mut pixels = vec![0u32; (SIZE * SIZE) as usize];
    let inside_round_rect = |x: f32, y: f32, (l, t, r, b): (f32, f32, f32, f32), radius: f32| {
        let cx = x.clamp(l + radius, r - radius);
        let cy = y.clamp(t + radius, b - radius);
        (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
            && x >= l
            && x <= r
            && y >= t
            && y <= b
    };
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let body = inside_round_rect(fx, fy, (3.0, 9.0, 29.0, 27.0), 4.0)
                || inside_round_rect(fx, fy, (11.0, 5.0, 21.0, 11.0), 2.0);
            let distance = ((fx - 16.0).powi(2) + (fy - 17.5).powi(2)).sqrt();
            let lens_hole = (4.0..6.5).contains(&distance);
            if body && !lens_hole {
                pixels[(y * SIZE + x) as usize] = 0xFFFF_FFFF;
            }
        }
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
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len());
        let mask = CreateBitmap(SIZE, SIZE, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        })
        .context("icon")
        .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}
