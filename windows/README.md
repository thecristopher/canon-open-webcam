# Canon Webcam for Windows

The Windows side of canon-open-webcam, in Rust. Same idea as the Mac app: talk to the camera ourselves, hand the picture to a virtual camera, and keep everything else out of the way. Needs Windows 11.

```
Canon ──USB, Windows' own MTP driver (WPD)──▶ engine: live view JPEG ▶ 1080p Lanczos ▶ NV12
                                                         │
                                               shared memory (Global\CanonWebcamFrames)
                                                         │
Zoom, Teams, Meet ◀── Frame Server ◀── CanonWebcamCamera.dll (Media Foundation source, MFCreateVirtualCamera)
```

- **No driver swap.** Canon's own EDSDK reaches the camera through WPD as MTP vendor commands, and so do we.
- **No signed driver.** Windows 11 virtual cameras are a plain COM DLL that Frame Server loads, so there's nothing for Microsoft to sign.
- **Wakes up on demand.** The DLL stamps the shared header whenever an app asks for a frame. The engine only opens the camera while that's fresh and lets it rest 30 seconds after the call ends.
- **A card instead of a frozen picture** when the camera is off, unplugged or busy, and a calm fallback frame if the app itself isn't running.

## Install

Grab `CanonWebcam-<version>-windows-x64.zip`, unzip it, double click `CanonWebcam.exe` and say yes. Windows asks for permission once, because Frame Server only reads camera registrations from HKLM. After that it starts when you sign in and lives in the tray.

To remove it: Settings > Apps > Installed apps > Canon Webcam, or Uninstall in the tray menu. That removes the virtual camera, the files, the login item and the logs.

From a terminal:

```powershell
CanonWebcam install            # or uninstall; add --quiet for no dialogs
CanonWebcam check out.bmp      # open "Canon Webcam" like Zoom would, report fps, save a frame
CanonWebcam list               # every portable device Windows sees
CanonWebcam probe frame.jpg    # talk to the Canon directly (quit the app first)
```

Logs: `%LOCALAPPDATA%\CanonWebcam\canon-webcam.log` for the app, `C:\ProgramData\CanonWebcam\camera.log` for the camera DLL.

## Crates

| Crate | What it does |
| --- | --- |
| `canon-ptp` | Canon EOS PTP: opcodes, events, live view frames, the camera state machine. No OS code. |
| `canon-wpd` | Runs those operations through Windows Portable Devices as MTP vendor extension commands. |
| `canon-shm` | The contract between engine and camera: frame size, shared memory layout, CLSID, RGB to NV12. |
| `canon-vcam` | `canon_vcam.dll`, the Media Foundation media source Frame Server loads. |
| `canon-webcam` | `CanonWebcam.exe`: tray app and engine, installer, uninstaller and the diagnostic commands. |

## Build

Needs Rust (`winget install Rustlang.Rustup`) and the Visual Studio C++ build tools with a Windows SDK.

```powershell
cargo test
cargo build --release                                       # target\release\CanonWebcam.exe and canon_vcam.dll
powershell -ExecutionPolicy Bypass -File scripts\package.ps1  # dist\CanonWebcam-<version>-windows-x64.zip
```

Running `target\release\CanonWebcam.exe install` installs straight from a build; it picks up the DLL sitting next to it.
