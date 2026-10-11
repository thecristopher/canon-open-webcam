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
    MF_VERSION, MFCreateVirtualCamera, MFSTARTUP_FULL, MFShutdown, MFStartup, MFVirtualCameraAccess_AllUsers,
    MFVirtualCameraLifetime_System, MFVirtualCameraType_SoftwareCameraSource,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile};
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, EVENT_MODIFY_STATE, GetExitCodeProcess, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, IsUserAnAdmin, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows_core::{HSTRING, Interface, PCWSTR, w};

use crate::console::tell;
use crate::log::log;

pub const APP_EXE: &str = "CanonWebcam.exe";
const CAMERA_DLL: &str = "CanonWebcamCamera.dll";
// What cargo calls the DLL next to the exe in a build or a release zip
const BUILT_DLL: &str = "canon_vcam.dll";
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\CanonWebcam";
const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "CanonWebcam";
pub const QUIT_EVENT: PCWSTR = w!("Local\\CanonWebcamQuit");
/// Passed by the sign in entry, so the app starts quietly in the tray instead of opening its window.
pub const BACKGROUND_FLAG: &str = "--background";
// Canon's EOS Webcam Utility keeps services running that grab the camera the moment it's plugged in
const CANON_SERVICES: [&str; 3] = ["EWCService", "EWCPairingService", "EWCVirtualCameraService"];

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
    data_dir().parent().unwrap().join(r"Microsoft\Windows\Start Menu\Programs\Canon Webcam.lnk")
}

pub fn installed_exe() -> PathBuf {
    install_dir().join(APP_EXE)
}

pub fn is_installed() -> bool {
    installed_exe().exists() && key_exists(&clsid_key())
}

/// True when this process is the copy in Program Files.
pub fn running_installed_copy() -> bool {
    let here = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    let there = installed_exe().canonicalize().ok();
    here.is_some() && here == there
}

pub fn install(quiet: bool, elevated: bool) -> Result<()> {
    if !is_admin() {
        if elevated {
            bail!("still not an administrator after asking");
        }
        request_install()?;
        // Back as the normal user: start the app so it owns the camera in this session
        start_app(false)?;
        tell(
            "Canon Webcam is installed and running. Pick \"Canon Webcam\" as the camera in Zoom, Teams, Meet or any other app.",
            quiet,
        );
        return Ok(());
    }

    install_as_admin()
}

/// Asks Windows for admin and runs the install part that needs it. No dialogs, for the setup window.
pub fn request_install() -> Result<()> {
    run_elevated(&["install", "--elevated"])
}

/// Starts the installed app as the normal user, in the tray, or with its window open.
pub fn start_app(show_window: bool) -> Result<()> {
    let mut command = Command::new(installed_exe());
    if !show_window {
        command.arg(BACKGROUND_FLAG);
    }
    command.spawn().context("couldn't start Canon Webcam")?;
    Ok(())
}

fn install_as_admin() -> Result<()> {
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
    let _ = hidden("icacls").arg(data_dir()).args(["/grant", "*S-1-5-19:(OI)(CI)M", "/q"]).status();

    let dll_path = dir.join(CAMERA_DLL);
    set_string(&clsid_key(), None, CAMERA_NAME)?;
    set_string(&format!(r"{}\InprocServer32", clsid_key()), None, &dll_path.to_string_lossy())?;
    set_string(&format!(r"{}\InprocServer32", clsid_key()), Some("ThreadingModel"), "Both")?;

    register_virtual_camera().context("Windows wouldn't register the virtual camera")?;
    if canon_utility_present() {
        disable_canon_services();
        log!("switched off Canon's EOS Webcam Utility services so they stop grabbing the camera");
    }

    let exe = dir.join(APP_EXE);
    set_string(RUN_KEY, Some(RUN_VALUE), &format!("\"{}\" {BACKGROUND_FLAG}", exe.display()))?;
    set_string(UNINSTALL_KEY, Some("DisplayName"), CAMERA_NAME)?;
    set_string(UNINSTALL_KEY, Some("DisplayVersion"), env!("CARGO_PKG_VERSION"))?;
    set_string(UNINSTALL_KEY, Some("Publisher"), "canon-open-webcam")?;
    set_string(UNINSTALL_KEY, Some("InstallLocation"), &dir.to_string_lossy())?;
    set_string(UNINSTALL_KEY, Some("DisplayIcon"), &exe.to_string_lossy())?;
    set_string(UNINSTALL_KEY, Some("UninstallString"), &format!("\"{}\" uninstall", exe.display()))?;
    set_string(UNINSTALL_KEY, Some("QuietUninstallString"), &format!("\"{}\" uninstall --quiet", exe.display()))?;
    set_dword(UNINSTALL_KEY, "NoModify", 1)?;
    set_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    if let Err(e) = create_shortcut(&exe, &shortcut_path()) {
        log!("no Start menu shortcut: {e}");
    }
    log!("installed");
    Ok(())
}

pub fn uninstall(quiet: bool, elevated: bool, restore_canon: bool) -> Result<()> {
    if !is_admin() {
        if elevated {
            bail!("still not an administrator after asking");
        }
        request_uninstall(restore_canon)?;
        let mut message = String::from("Uninstalled. Everything Canon Webcam put on this PC is gone.");
        if restore_canon {
            message.push_str(" Canon's EOS Webcam Utility is back on.");
        } else if canon_utility_present() {
            message.push_str(" Canon's EOS Webcam Utility stays off. Run with --restore-canon to bring it back.");
        }
        tell(&message, quiet);
        return Ok(());
    }

    log!("uninstalling");
    ask_app_to_quit();
    if restore_canon && canon_utility_present() {
        restore_canon_services();
        log!("turned Canon's EOS Webcam Utility services back on");
    }
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
        std::fs::remove_dir_all(&dir).with_context(|| format!("couldn't remove {}", dir.display()))?;
    }
    log!("uninstalled");
    Ok(())
}

/// Asks Windows for admin and runs the uninstall part that needs it. No dialogs, for the uninstall window.
pub fn request_uninstall(restore_canon: bool) -> Result<()> {
    let mut args = vec!["uninstall", "--elevated", "--quiet"];
    if restore_canon {
        args.push("--restore-canon");
    }
    run_elevated(&args)
}

/// What uninstalling removes, for the uninstall window to show before anything happens.
pub fn removal_list() -> Vec<String> {
    vec![
        format!("The app in {}", install_dir().display()),
        format!("The \"{CAMERA_NAME}\" virtual camera"),
        "Starting when you sign in, and the Start menu shortcut".into(),
        format!("The camera's log in {}", data_dir().display()),
    ]
}

pub fn starts_at_sign_in() -> bool {
    get_string(RUN_KEY, RUN_VALUE).is_some()
}

/// The Windows build number, 22000 and up being Windows 11.
pub fn windows_build() -> u32 {
    get_string(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
        .and_then(|build| build.parse().ok())
        .unwrap_or(0)
}

/// The name apps list the virtual camera under, if Windows shows it to them.
pub fn virtual_camera_visible() -> Option<String> {
    crate::diagnose::camera_names().ok()?.into_iter().find(|name| name.starts_with(CAMERA_NAME))
}

/// A Canon that Zadig (or another libusb tool) moved onto a generic USB driver. Windows can't share
/// such a camera through WPD, so it's invisible to us and to Canon's own software until it's moved back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HijackedCamera {
    pub driver: String,
    /// The oemNN.inf package to remove, which hands the camera back to Windows' own driver
    pub package: String,
}

pub fn canon_on_generic_usb_driver() -> Option<HijackedCamera> {
    let script = "Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match 'VID_04A9' } | ForEach-Object { \
        $p = Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName DEVPKEY_Device_Service, DEVPKEY_Device_DriverInfPath; \
        ($p | ForEach-Object { $_.Data }) -join '|' }";
    let output = hidden("powershell").args(["-NoProfile", "-NonInteractive", "-Command", script]).output().ok()?;
    String::from_utf8_lossy(&output.stdout).lines().find_map(|line| {
        let (driver, package) = line.trim().split_once('|')?;
        let generic = ["winusb", "libusbk", "libusb0"].contains(&driver.to_lowercase().as_str());
        (generic && package.to_lowercase().starts_with("oem"))
            .then(|| HijackedCamera { driver: driver.to_string(), package: package.to_string() })
    })
}

/// Asks for admin and hands the camera back to Windows' own driver.
pub fn request_camera_driver_fix() -> Result<()> {
    run_elevated(&["fix-driver", "--elevated"])
}

/// The admin half: remove the generic driver package and let Windows pick its own driver again.
pub fn fix_camera_driver() -> Result<()> {
    let camera = canon_on_generic_usb_driver().context("the Canon is already on Windows' own driver")?;
    log!("removing {} ({}) so Windows puts its own camera driver back", camera.package, camera.driver);
    let removed = hidden("pnputil").args(["/delete-driver", &camera.package, "/uninstall"]).status()?;
    if !removed.success() {
        bail!("Windows wouldn't remove the {} driver package", camera.package);
    }
    let _ = hidden("pnputil").arg("/scan-devices").status();
    Ok(())
}

/// True when Canon's EOS Webcam Utility is on this PC, so offering to bring it back makes sense.
pub fn canon_utility_present() -> bool {
    CANON_SERVICES.iter().any(|service| service_exists(service))
}

pub fn canon_services_disabled() -> bool {
    CANON_SERVICES.iter().filter(|service| service_exists(service)).all(|service| {
        hidden("sc")
            .args(["qc", service])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("DISABLED"))
    })
}

fn service_exists(service: &str) -> bool {
    hidden("sc").args(["query", service]).output().is_ok_and(|out| out.status.success())
}

fn disable_canon_services() {
    for service in CANON_SERVICES.iter().filter(|service| service_exists(service)) {
        let _ = hidden("sc").args(["stop", service]).output();
        let _ = hidden("sc").args(["config", service, "start=", "disabled"]).output();
    }
}

fn restore_canon_services() {
    for service in CANON_SERVICES.iter().filter(|service| service_exists(service)) {
        let _ = hidden("sc").args(["config", service, "start=", "auto"]).output();
        let _ = hidden("sc").args(["start", service]).output();
    }
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
            bail!("the administrator step failed, see {}", crate::log::path().display());
        }
    }
    Ok(())
}

/// Removes a folder from a detached shell once every process running from it has let go,
/// retrying each second for up to a minute (the uninstall window may still be open from there).
fn delete_after_exit(dir: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // /s makes cmd strip only the outer quotes so the quoted path inside survives.
    // Rust's own argument quoting would turn those inner quotes into \" which cmd can't read.
    let dir = dir.display();
    let script = format!(
        "\"for /l %i in (1,1,60) do @(if exist \"{dir}\" (ping -n 2 127.0.0.1 >nul & rmdir /s /q \"{dir}\" 2>nul))\""
    );
    hidden("cmd").args(["/d", "/s", "/c"]).raw_arg(script).spawn()?;
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
        shell_link.cast::<IPersistFile>()?.Save(&HSTRING::from(link.as_os_str()), true)?;
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
    let found =
        unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(path), None, KEY_READ | KEY_WOW64_64KEY, &mut key) }
            .is_ok();
    if found {
        unsafe {
            let _ = RegCloseKey(key);
        }
    }
    found
}

fn get_string(path: &str, name: &str) -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path),
            &HSTRING::from(name),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if result.is_err() {
        return None;
    }
    let length = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn set_string(path: &str, name: Option<&str>, value: &str) -> Result<()> {
    let key = open_key(path)?;
    let wide: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    let name = name.map(HSTRING::from);
    let result = unsafe {
        RegSetValueExW(key, name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())), None, REG_SZ, Some(bytes))
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result.ok().with_context(|| format!(r"couldn't write HKLM\{path}"))
}

fn set_dword(path: &str, name: &str, value: u32) -> Result<()> {
    let key = open_key(path)?;
    let result = unsafe { RegSetValueExW(key, &HSTRING::from(name), None, REG_DWORD, Some(&value.to_le_bytes())) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result.ok().with_context(|| format!(r"couldn't write HKLM\{path}"))
}

fn delete_tree(path: &str) -> Result<()> {
    let result = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, &HSTRING::from(path)) };
    if result.is_err() && result != ERROR_FILE_NOT_FOUND {
        return result.ok().with_context(|| format!(r"couldn't delete HKLM\{path}"));
    }
    Ok(())
}

fn delete_value(path: &str, name: &str) -> Result<()> {
    let result = unsafe { RegDeleteKeyValueW(HKEY_LOCAL_MACHINE, &HSTRING::from(path), &HSTRING::from(name)) };
    if result.is_err() && result != ERROR_FILE_NOT_FOUND {
        return result.ok().with_context(|| format!(r"couldn't delete HKLM\{path}\{name}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Slow (waits for the detached shell), so run it on purpose: cargo test -- --ignored
    #[test]
    #[ignore]
    fn a_folder_with_spaces_is_removed_once_its_files_are_let_go() {
        let dir = std::env::temp_dir().join("Canon Webcam delete test");
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        // Hold a file open with no sharing for a few seconds, like a running exe in the folder.
        // Rust shares delete access by default, which wouldn't stop anything.
        use std::os::windows::fs::OpenOptionsExt;
        let held = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(0)
            .open(dir.join("inner").join("held.txt"))
            .unwrap();
        delete_after_exit(&dir).unwrap();
        std::thread::sleep(Duration::from_secs(4));
        assert!(dir.exists(), "nothing should go while a file is still open");
        drop(held);
        std::thread::sleep(Duration::from_secs(4));
        assert!(!dir.exists(), "{} should be gone once it was let go", dir.display());
    }
}
