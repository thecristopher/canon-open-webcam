//! Install and uninstall.
//!
//! Frame Server runs as LOCAL SERVICE and only reads COM registrations from HKLM, so installing
//! needs admin once. The non elevated process asks for it by relaunching itself with "runas",
//! waits, and then starts the tray app as the normal user (WPD wants the user's session anyway).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use canon_shm::{CAMERA_NAME, CLSID_STRING};
use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND};
use windows::Win32::Media::MediaFoundation::{
    MF_VERSION, MFCreateVirtualCamera, MFSTARTUP_FULL, MFShutdown, MFStartup,
    MFVirtualCameraAccess_AllUsers, MFVirtualCameraLifetime_System,
    MFVirtualCameraType_SoftwareCameraSource,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile};
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, EVENT_MODIFY_STATE, GetExitCodeProcess, INFINITE, OpenEventW, SetEvent,
    WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, IsUserAnAdmin, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows_core::{HSTRING, Interface, PCWSTR, w};

use crate::console::{tell, warn};
use crate::log::log;

pub const APP_EXE: &str = "CanonWebcam.exe";
const CAMERA_DLL: &str = "CanonWebcamCamera.dll";
// What cargo calls the DLL next to the exe in a build or a release zip
const BUILT_DLL: &str = "canon_vcam.dll";
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\CanonWebcam";
const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "CanonWebcam";
pub const QUIT_EVENT: PCWSTR = w!("Local\\CanonWebcamQuit");

fn clsid_key() -> String {
    format!(r"SOFTWARE\Classes\CLSID\{CLSID_STRING}")
}

pub fn install_dir() -> PathBuf {
    let base = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .unwrap_or_else(|| r"C:\Program Files".into());
    PathBuf::from(base).join("Canon Webcam")
}

fn data_dir() -> PathBuf {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    PathBuf::from(base).join("CanonWebcam")
}

fn shortcut_path() -> PathBuf {
    data_dir()
        .parent()
        .unwrap()
        .join(r"Microsoft\Windows\Start Menu\Programs\Canon Webcam.lnk")
}

pub fn installed_exe() -> PathBuf {
    install_dir().join(APP_EXE)
}

pub fn is_installed() -> bool {
    installed_exe().exists() && key_exists(&clsid_key())
}

/// True when this process is the copy in Program Files.
pub fn running_installed_copy() -> bool {
    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let there = installed_exe().canonicalize().ok();
    here.is_some() && here == there
}

pub fn install(quiet: bool, elevated: bool) -> Result<()> {
    if !is_admin() {
        if elevated {
            bail!("still not an administrator after asking");
        }
        run_elevated(&["install", "--elevated"])?;
        // Back as the normal user: start the app so it owns the camera in this session
        Command::new(installed_exe())
            .spawn()
            .context("couldn't start Canon Webcam")?;
        tell(
            "Canon Webcam is installed and running. Pick \"Canon Webcam\" as the camera in Zoom, Teams, Meet or any other app.",
            quiet,
        );
        return Ok(());
    }

    log!("installing into {}", install_dir().display());
    let here = std::env::current_exe()?;
    let from = here.parent().context("exe has no folder")?;
    let dll = [from.join(BUILT_DLL), from.join(CAMERA_DLL)]
        .into_iter()
        .find(|p| p.exists())
        .context("the camera DLL isn't next to CanonWebcam.exe")?;

    ask_app_to_quit();
    // Frame Server holds the old DLL while it's loaded
    stop_frame_server();
    let dir = install_dir();
    std::fs::create_dir_all(&dir)?;
    if !running_installed_copy() {
        copy_retrying(&here, &dir.join(APP_EXE))?;
    }
    copy_retrying(&dll, &dir.join(CAMERA_DLL))?;

    // Somewhere the camera, running as LOCAL SERVICE, may write its log
    std::fs::create_dir_all(data_dir())?;
    let _ = hidden("icacls")
        .arg(data_dir())
        .args(["/grant", "*S-1-5-19:(OI)(CI)M", "/q"])
        .status();

    let dll_path = dir.join(CAMERA_DLL);
    set_string(&clsid_key(), None, CAMERA_NAME)?;
    set_string(
        &format!(r"{}\InprocServer32", clsid_key()),
        None,
        &dll_path.to_string_lossy(),
    )?;
    set_string(
        &format!(r"{}\InprocServer32", clsid_key()),
        Some("ThreadingModel"),
        "Both",
    )?;

    register_virtual_camera().context("Windows wouldn't register the virtual camera")?;

    let exe = dir.join(APP_EXE);
    set_string(RUN_KEY, Some(RUN_VALUE), &format!("\"{}\"", exe.display()))?;
    set_string(UNINSTALL_KEY, Some("DisplayName"), CAMERA_NAME)?;
    set_string(
        UNINSTALL_KEY,
        Some("DisplayVersion"),
        env!("CARGO_PKG_VERSION"),
    )?;
    set_string(UNINSTALL_KEY, Some("Publisher"), "canon-open-webcam")?;
    set_string(
        UNINSTALL_KEY,
        Some("InstallLocation"),
        &dir.to_string_lossy(),
    )?;
    set_string(UNINSTALL_KEY, Some("DisplayIcon"), &exe.to_string_lossy())?;
    set_string(
        UNINSTALL_KEY,
        Some("UninstallString"),
        &format!("\"{}\" uninstall", exe.display()),
    )?;
    set_string(
        UNINSTALL_KEY,
        Some("QuietUninstallString"),
        &format!("\"{}\" uninstall --quiet", exe.display()),
    )?;
    set_dword(UNINSTALL_KEY, "NoModify", 1)?;
    set_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    if let Err(e) = create_shortcut(&exe, &shortcut_path()) {
        log!("no Start menu shortcut: {e}");
    }
    log!("installed");
    Ok(())
}

pub fn uninstall(quiet: bool, elevated: bool) -> Result<()> {
    if !is_admin() {
        if elevated {
            bail!("still not an administrator after asking");
        }
        if !quiet
            && !crate::console::attached()
            && !crate::console::ask(
                "Remove Canon Webcam? Your Canon and its settings stay as they are.",
            )
        {
            return Ok(());
        }
        run_elevated(&["uninstall", "--elevated", "--quiet"])?;
        tell("Canon Webcam is gone.", quiet);
        return Ok(());
    }

    log!("uninstalling");
    ask_app_to_quit();
    if let Err(e) = remove_virtual_camera() {
        log!("removing the virtual camera: {e}");
    }
    for key in [clsid_key(), UNINSTALL_KEY.to_string()] {
        delete_tree(&key)?;
    }
    delete_value(RUN_KEY, RUN_VALUE)?;
    let _ = std::fs::remove_file(shortcut_path());
    let _ = std::fs::remove_dir_all(data_dir());

    stop_frame_server();
    let dir = install_dir();
    if running_installed_copy() {
        // We can't delete the exe we're running from, so a detached shell does it once we've exited
        delete_after_exit(&dir)?;
    } else if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("couldn't remove {}", dir.display()))?;
    }
    log!("uninstalled");
    Ok(())
}

fn register_virtual_camera() -> Result<()> {
    let _ = remove_virtual_camera();
    with_media_foundation(|| unsafe {
        let camera = MFCreateVirtualCamera(
            MFVirtualCameraType_SoftwareCameraSource,
            MFVirtualCameraLifetime_System,
            MFVirtualCameraAccess_AllUsers,
            &HSTRING::from(CAMERA_NAME),
            &HSTRING::from(CLSID_STRING),
            None,
        )?;
        camera.Start(None)?;
        log!("virtual camera registered");
        Ok(())
    })
}

fn remove_virtual_camera() -> Result<()> {
    with_media_foundation(|| unsafe {
        let camera = MFCreateVirtualCamera(
            MFVirtualCameraType_SoftwareCameraSource,
            MFVirtualCameraLifetime_System,
            MFVirtualCameraAccess_AllUsers,
            &HSTRING::from(CAMERA_NAME),
            &HSTRING::from(CLSID_STRING),
            None,
        )?;
        camera.Remove()?;
        log!("virtual camera removed");
        Ok(())
    })
}

fn with_media_foundation<T>(work: impl FnOnce() -> windows_core::Result<T>) -> Result<T> {
    let _com = canon_wpd::ComApartment::enter()?;
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL)? };
    let result = work();
    unsafe {
        let _ = MFShutdown();
    }
    Ok(result?)
}

fn is_admin() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

/// Relaunches this exe through UAC and waits for it, failing if it did.
fn run_elevated(args: &[&str]) -> Result<()> {
    let exe = HSTRING::from(std::env::current_exe()?.as_os_str());
    let params = HSTRING::from(args.join(" "));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).context("Windows didn't allow the administrator step")?;
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code)?;
        let _ = CloseHandle(info.hProcess);
        if code != 0 {
            bail!(
                "the administrator step failed, see {}",
                crate::log::path().display()
            );
        }
    }
    Ok(())
}

/// Removes a folder a few seconds from now, from a detached shell, once this process has let go of it.
fn delete_after_exit(dir: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // /s makes cmd strip only the outer quotes so the quoted path inside survives.
    // Rust's own argument quoting would turn those inner quotes into \" which cmd can't read.
    let script = format!(
        "\"ping -n 4 127.0.0.1 >nul & rmdir /s /q \"{}\"\"",
        dir.display()
    );
    hidden("cmd")
        .args(["/d", "/s", "/c"])
        .raw_arg(script)
        .spawn()?;
    Ok(())
}

fn ask_app_to_quit() {
    unsafe {
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, QUIT_EVENT) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
            std::thread::sleep(Duration::from_millis(1500));
        }
    }
}

fn stop_frame_server() {
    for service in ["FrameServer", "FrameServerMonitor"] {
        let _ = hidden("net").args(["stop", service, "/y"]).output();
    }
}

fn hidden(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW.0);
    command
}

fn copy_retrying(from: &Path, to: &Path) -> Result<()> {
    for attempt in 0..10 {
        match std::fs::copy(from, to) {
            Ok(_) => return Ok(()),
            Err(_) if attempt < 9 => std::thread::sleep(Duration::from_millis(300)),
            Err(e) => return Err(e).with_context(|| format!("couldn't copy to {}", to.display())),
        }
    }
    unreachable!()
}

fn create_shortcut(target: &Path, link: &Path) -> Result<()> {
    let _com = canon_wpd::ComApartment::enter()?;
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        shell_link.SetPath(&HSTRING::from(target.as_os_str()))?;
        shell_link.SetDescription(w!("Use your Canon as a webcam"))?;
        shell_link
            .cast::<IPersistFile>()?
            .Save(&HSTRING::from(link.as_os_str()), true)?;
    }
    Ok(())
}

// Registry helpers, all against the 64 bit view of HKLM

fn open_key(path: &str) -> Result<HKEY> {
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS | KEY_WOW64_64KEY,
            None,
            &mut key,
            None,
        )
        .ok()
        .with_context(|| format!(r"couldn't open HKLM\{path}"))?;
    }
    Ok(key)
}

fn key_exists(path: &str) -> bool {
    let mut key = HKEY::default();
    let found = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path),
            None,
            KEY_READ | KEY_WOW64_64KEY,
            &mut key,
        )
    }
    .is_ok();
    if found {
        unsafe {
            let _ = RegCloseKey(key);
        }
    }
    found
}

fn set_string(path: &str, name: Option<&str>, value: &str) -> Result<()> {
    let key = open_key(path)?;
    let wide: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    let name = name.map(HSTRING::from);
    let result = unsafe {
        RegSetValueExW(
            key,
            name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())),
            None,
            REG_SZ,
            Some(bytes),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
        .ok()
        .with_context(|| format!(r"couldn't write HKLM\{path}"))
}

fn set_dword(path: &str, name: &str, value: u32) -> Result<()> {
    let key = open_key(path)?;
    let result = unsafe {
        RegSetValueExW(
            key,
            &HSTRING::from(name),
            None,
            REG_DWORD,
            Some(&value.to_le_bytes()),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
        .ok()
        .with_context(|| format!(r"couldn't write HKLM\{path}"))
}

fn delete_tree(path: &str) -> Result<()> {
    let result = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, &HSTRING::from(path)) };
    if result.is_err() && result != ERROR_FILE_NOT_FOUND {
        return result
            .ok()
            .with_context(|| format!(r"couldn't delete HKLM\{path}"));
    }
    Ok(())
}

fn delete_value(path: &str, name: &str) -> Result<()> {
    let result = unsafe {
        RegDeleteKeyValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path),
            &HSTRING::from(name),
        )
    };
    if result.is_err() && result != ERROR_FILE_NOT_FOUND {
        return result
            .ok()
            .with_context(|| format!(r"couldn't delete HKLM\{path}\{name}"));
    }
    Ok(())
}

pub fn report(result: Result<()>, quiet: bool) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            log!("{e:#}");
            warn(&format!("{e:#}"), quiet);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Slow (waits for the detached shell), so run it on purpose: cargo test -- --ignored
    #[test]
    #[ignore]
    fn a_folder_with_spaces_is_removed_after_we_let_go() {
        let dir = std::env::temp_dir().join("Canon Webcam delete test");
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        std::fs::write(dir.join("inner").join("file.txt"), "x").unwrap();
        delete_after_exit(&dir).unwrap();
        std::thread::sleep(Duration::from_secs(7));
        assert!(!dir.exists(), "{} should be gone", dir.display());
    }
}
