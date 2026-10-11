# Canon Webcam for Windows

The Windows side of canon-open-webcam, in Rust. Same idea as the Mac app: talk to the camera ourselves, hand the picture to a virtual camera, and keep everything else out of the way.

```
Canon over USB ──Windows' own MTP driver (WPD)──▶ canon-ptp live view ──▶ shared memory ──▶ Media Foundation virtual camera ──▶ your apps
```

## Crates

| Crate | What it does |
| --- | --- |
| `canon-ptp` | Canon EOS PTP: opcodes, events, live view frames, the camera state machine. No OS code, so it can serve any platform. |
| `canon-wpd` | Runs those operations through Windows Portable Devices as MTP vendor extension commands, on the driver Windows already installs. No Zadig, no driver swap. |
| `canon-webcam` | The command line tool, for now `list` and `probe`. |

Next up: a Media Foundation media source DLL registered with `MFCreateVirtualCamera` (Windows 11), the engine that feeds it through shared memory, and a tray app.

## Build

Needs Rust (`winget install Rustlang.Rustup`) and the Visual Studio C++ build tools with a Windows SDK.

```powershell
cargo test
cargo run -p canon-webcam -- list
cargo run -p canon-webcam -- probe frame.jpg   # camera on, plugged in, nothing else using it
```
