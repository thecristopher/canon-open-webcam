# Canon Webcam for Windows

The Windows side of canon-open-webcam, in Rust, with the same features as the Mac app. Talk to the camera ourselves, hand the picture to a virtual camera, and keep everything else out of the way. Needs Windows 11.

```
Canon ──USB, Windows' own MTP driver (WPD)──▶ engine: live view JPEG ▶ color, crop, 1080p Lanczos, sharpen, mirror ▶ NV12
                                                         │
                                               shared memory (Global\CanonWebcamFrames)
                                                         │
Zoom, Teams, Meet ◀── Frame Server ◀── CanonWebcamCamera.dll (Media Foundation source, MFCreateVirtualCamera)
```

- **No driver swap.** Canon's own EDSDK reaches the camera through WPD as MTP vendor commands, and so do we.
  If Zadig ever moved your Canon onto WinUSB for a libusb tool, it's hidden from WPD: `doctor` says so, and Set Up (or `CanonWebcam fix-driver`) hands it back to Windows' own driver.
- **No signed driver.** Windows 11 virtual cameras are a plain COM DLL that Frame Server loads, so there's nothing for Microsoft to sign.
- **Wakes up on demand.** The DLL stamps the shared header whenever an app asks for a frame. The engine opens the camera only while that's fresh, and rests it 30 seconds after the last app lets go. Unlike the Mac, there's no guessing involved: the heartbeat says exactly when apps come and go.
- **A card instead of a frozen picture** when the camera is off, unplugged or busy, and a calm fallback frame if the app itself isn't running.
- **Light.** About 10ms of one core per frame with every adjustment on.

## Install

Grab `CanonWebcam-<version>-windows-x64.zip`, unzip it and double click `CanonWebcam.exe`. **Set Up Canon Webcam** walks you through it:

1. **Windows 11.** Checks for the built in virtual camera support.
2. **Canon Webcam.** Copies the app into Program Files, registers the virtual camera, starts it when you sign in, and switches off Canon's EOS Webcam Utility services so they stop grabbing the camera. Windows asks for permission once, because Frame Server only reads camera registrations from HKLM.
3. **Your camera.** Shows when your Canon is plugged in. You can install before plugging it in.

**Uninstall Canon Webcam** (Settings > Apps, or the tray menu) lists exactly what it will remove and can turn Canon's EOS Webcam Utility back on.

## Use

Pick **Canon Webcam** in your meeting app.

### The app

Open **Canon Webcam** and you get the picture front and center, exactly what everyone on the call sees, with every control in a sidebar:

- **Adjust:** brightness, contrast, saturation, warmth, tint and sharpness. Double click a label to put it back, or hit Reset.
- **Framing:** zoom up to 3x, then drag the picture to reframe it, or scroll and pinch to zoom. Mirror flips what everyone sees (apps already mirror your own self view).
- **Camera:** exposure, white balance, ISO, aperture and shutter, straight from the camera, plus focus (auto, or nudge nearer and farther).
- **Output:** send every camera frame as it arrives, or a steady 30 or 60 frames per second.

At sign in it starts quietly as a camera icon in the tray, with no window. Click the icon to open the window, or right click for the camera's status, a quick mirror switch, set up, the log, uninstall and quit. Closing the window leaves the tray running, and the webcam keeps working.

ISO, aperture and shutter belong to the camera's mode dial. In movie mode with auto exposure the camera owns them and the app says so. Turn the dial to **M** for full manual control.

### From the terminal

Everything in the app is also a command, same as on the Mac:

```powershell
CanonWebcam status                   # what's going on, plus every setting
CanonWebcam set brightness 0.2       # picture: brightness, contrast, saturation, warmth, tint (-1 to 1)
CanonWebcam set sharpness 0.6        # 0 to 1
CanonWebcam set picture reset
CanonWebcam set zoom 1.5             # framing: zoom (1 to 3), pan-x and pan-y (-1 to 1)
CanonWebcam set framing reset
CanonWebcam set mirror on
CanonWebcam set fps 30               # output: camera, 30 or 60
CanonWebcam set exposure +1          # camera: exposure, white-balance, iso, aperture, shutter, color-temp
CanonWebcam focus                    # autofocus, or: focus near 2, focus far 1
CanonWebcam doctor                   # check everything is in place
CanonWebcam install                  # or uninstall [--restore-canon]; --quiet for no dialogs, --window for the checklist
CanonWebcam start --always-on        # run the engine in the terminal, keeping live view warm with no app watching
CanonWebcam bench                    # the camera's raw live view rate
CanonWebcam check out.bmp            # open "Canon Webcam" like Zoom would, report fps, save a frame
```

Settings live in `%APPDATA%\CanonWebcam\options.json` (the same format as the Mac's). Logs: `%LOCALAPPDATA%\CanonWebcam\canon-webcam.log` for the app, `C:\ProgramData\CanonWebcam\camera.log` for the camera DLL. Set `CANON_WEBCAM_DEBUG=1` to also log a per frame timing breakdown.

## Crates

| Crate | What it does |
| --- | --- |
| `canon-ptp` | Canon EOS PTP: opcodes, events, live view frames, settings tables, the camera state machine. No OS code. |
| `canon-wpd` | Runs those operations through Windows Portable Devices as MTP vendor extension commands. |
| `canon-shm` | The contract between engine and camera: frame size, shared memory layout, frame rate, CLSID, RGB to NV12. |
| `canon-vcam` | `canon_vcam.dll`, the Media Foundation media source Frame Server loads. |
| `canon-webcam` | `CanonWebcam.exe`: the tray and engine, the windows (egui), the control pipe, setup and every command. |

The engine and the windows talk over a named pipe (`\\.\pipe\CanonWebcam-<session>`) with the same JSON requests as the Mac's control socket.

## Build

Needs Rust (`winget install Rustlang.Rustup`) and the Visual Studio C++ build tools with a Windows SDK.

```powershell
cargo test
cargo build --release                                         # target\release\CanonWebcam.exe and canon_vcam.dll
powershell -ExecutionPolicy Bypass -File scripts\package.ps1  # dist\CanonWebcam-<version>-windows-x64.zip
```

Running `target\release\CanonWebcam.exe install` installs straight from a build; it picks up the DLL sitting next to it.
