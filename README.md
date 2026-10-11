# canon-open-webcam

Use a Canon EOS camera (built and tested on a Rebel T7 / 2000D / 1500D) as a webcam on macOS and Windows, for free, with nothing in the way. One small native app talks to the camera over USB and feeds the picture straight into a virtual camera that Zoom, Meet, FaceTime, Teams, Discord and friends can pick.

| | macOS | Windows 11 |
| --- | --- | --- |
| Written in | Swift | Rust |
| Talks to the camera through | libusb, built in | Windows' own MTP driver (WPD), no driver swap |
| Virtual camera | OBS's camera driver (OBS never has to run) | Our own Media Foundation virtual camera, no driver to sign |
| Shows up in apps as | OBS Virtual Camera | Canon Webcam |
| Lives in | `Sources/`, `Tests/` | [`windows/`](windows/README.md) |

Both have the same app, the same controls, the same commands and the same settings file format. The rest of this page is about the Mac; the Windows side has its own [README](windows/README.md), summed up [below](#windows).

```
Canon over USB ──PTP live view (our own client)──▶ decode, scale to 1080p ──▶ OBS Virtual Camera driver ──▶ your apps
```

No gphoto2, no ffmpeg, no Canon software, and OBS never has to run. We only borrow the camera driver OBS installs, because it is already signed by Apple, which means no developer account and no security prompts beyond the one time approval.

## What you get

- **Wakes up on demand.** The camera stays untouched until an app opens the virtual camera and goes live in about a second. After a call it rests again about 30 seconds after you hang up.
- **Every real frame.** On the T7 that is 24fps in movie mode, all of them delivered, upscaled to 1920x1080 with Lanczos and a light sharpen so edges and texture stay crisp.
- **A friendly card instead of a frozen picture.** If the camera is off, unplugged, or busy in another app, people on the call see a clear message, and the picture comes back on its own the moment the camera does.
- **Camera controls from the terminal.** Exposure, white balance, ISO, aperture, shutter, focus, plus mirror and digital zoom.
- **Stays awake.** The camera won't power itself off mid meeting while it's live.
- **Survives the boring stuff.** Unplugging, crashes, sleep, another app grabbing the camera, a stale session left behind by a crash. It recovers from all of them without a restart.
- **Light.** One small binary with libusb built in. About 20% of one CPU core while live, nothing while idle.

### Compared to what Canon ships

Measured on the same T7 and Mac:

| | EOS Webcam Utility | canon-open-webcam |
| --- | --- | --- |
| Where the picture comes from | The camera's USB live view | The same USB live view |
| Live view size (T7, movie mode) | 1024x576 | 1024x576 |
| Output | 1280x720, frames repeated up to 30fps | 1920x1080, every real frame (24fps) |
| What runs on your Mac | An Electron app, two background services, a camera extension, Canon's SDK | One binary plus OBS's camera driver |
| Price | Free tier, Pro is a subscription | Free |

Canon's 720p and 1080p are the same live view scaled up, there isn't more detail hiding in the camera. The T7 simply doesn't send more than 1024x576 over USB, and it only offers the largest live view size. Each frame is a high quality JPEG (around 250KB), so the detail that does arrive is clean.

## Install

```sh
brew install libusb   # only needed to build, the apps carry it inside
make dist
```

That makes `dist/CanonWebcam-1.0.dmg` with two apps in it. Or grab the disk image from the [Releases page](https://github.com/thecristopher/canon-open-webcam/releases).

**Install Canon Webcam** walks you through a short checklist, one button per step:

1. **Camera driver.** Installs OBS if it's missing (with Homebrew, or it opens the download page), then opens OBS for a moment and takes you to the right spot in System Settings to allow its camera. The window notices when you approve and closes OBS again. OBS never has to run after that.
2. **Canon Webcam.** Copies the Canon Webcam app into Applications (your personal one if you aren't an admin), starts the camera engine, adds a login item so it all comes back when you log in, and opens the app. It also switches off Canon's EOS Webcam Utility services so they stop grabbing the camera.
3. **Your camera.** Shows when your Canon is plugged in. You can install before plugging it in.

**Uninstall Canon Webcam** lists exactly what it will remove (the app, the login item, the engine, its files and log), can turn Canon's EOS Webcam Utility back on, and leaves OBS alone. It's also in the app's menu bar panel.

Both work without a window too, for scripts: `Install Canon Webcam.app/Contents/MacOS/CanonWebcamInstaller --install` and `Uninstall Canon Webcam.app/Contents/MacOS/CanonWebcamUninstaller --uninstall`.

### Installing from the terminal

```sh
make install                        # build and start at login
canon-webcam doctor                 # check everything is in place
canon-webcam uninstall              # remove everything
canon-webcam uninstall --restore-canon
```

`install` copies the binary to `~/Library/Application Support/canon-open-webcam/`, links `canon-webcam` into `~/.local/bin`, and adds a LaunchAgent (`~/Library/LaunchAgents/dev.canon-open-webcam.plist`, no sudo) that keeps it running. Rebuilding or moving this folder never breaks it.

## Use

Pick **OBS Virtual Camera** in your meeting app. That's it. macOS has no system wide default camera, so pick it once per app and each app remembers.

### The app

Open **Canon Webcam** and you get the picture front and center, exactly what everyone on the call sees, with every control in a sidebar:

- **Adjust:** brightness, contrast, saturation, warmth, tint and sharpness. Double click a label to put it back, or hit Reset.
- **Framing:** zoom up to 3x, then drag the picture to reframe it, or pinch to zoom. Mirror flips what everyone sees (apps already mirror your own self view).
- **Camera:** exposure, white balance, ISO, aperture and shutter, straight from the camera, plus focus (auto, or nudge nearer and farther).
- **Output:** send every camera frame as it arrives, or a steady 30 or 60 frames per second.

At login it starts quietly as a camera icon in the menu bar, with no window. Click the icon for the camera's status, a quick mirror switch, and the way back into the app. Close the window and it goes back to the menu bar. The webcam itself keeps working even if you quit the app, because the engine runs on its own.

ISO, aperture and shutter belong to the camera's mode dial. In movie mode with auto exposure the camera owns them and the app says so. Turn the dial to **M** for full manual control. Exposure compensation and white balance work in any mode.

### From the terminal

Everything in the app is also a command:

```sh
canon-webcam status                   # what's going on, plus every setting
canon-webcam set brightness 0.2       # picture: brightness, contrast, saturation, warmth, tint (-1 to 1)
canon-webcam set sharpness 0.6        # 0 to 1
canon-webcam set picture reset
canon-webcam set zoom 1.5             # framing: zoom (1 to 3), pan-x and pan-y (-1 to 1)
canon-webcam set framing reset
canon-webcam set mirror on
canon-webcam set fps 30               # output: camera, 30 or 60
canon-webcam set exposure +1          # camera: exposure, white-balance, iso, aperture, shutter, color-temp
canon-webcam focus                    # autofocus, or: focus near 2, focus far 1
```

Run it in a terminal instead of at login with `canon-webcam start`. `--always-on` keeps live view running even with no app watching, which skips the one second warm up at the cost of camera battery and heat.

## Camera tips

What actually makes the picture better, now that the software is maxed out:

- **Light your face.** A lamp or window in front of you does more than any setting. The sensor is big, but live view still gets noisy in a dim room.
- **Open the aperture.** The kit 18-55mm is f/3.5 at its widest. A 50mm f/1.8 is the cheapest real upgrade, with a blurry background and much better low light. In **M** mode you can then `set aperture f/1.8`.
- **Use M mode** to lock exposure and white balance, so the picture doesn't pump when you move or the light changes.

- Movie mode on the dial gives a 16:9 picture. Photo mode gives 3:2, which gets cropped to fill 16:9.
- A dummy battery (ACK-E10 for the T7) keeps it running all day.
- While live, the camera is kept awake. While idle it follows its own **Auto power off** setting, and if it falls asleep the card asks you to turn it back on.
- If the card says another app is using the camera, close EOS Utility, Image Capture or Photos.

## How it works

**Talking to the camera.** Canon bodies speak PTP over USB with Canon's own extensions. `Sources/CanonPTP` is a small client for the handful of operations we need: open a session, switch to remote mode, point live view at the computer, then ask for viewfinder frames. Each frame arrives as a JPEG. macOS starts `ptpcamerad` the moment a camera appears, and it grabs the USB interface, so we stop it right before connecting.

**Pacing.** The camera makes a frame every 41.7ms and moving one over USB takes about 14ms. Asking too early just earns a "not ready", asking late adds lag, so the engine asks 20ms after the last transfer finished and lands right on the next frame. libgphoto2 polls camera events on every frame and backs off with growing sleeps, which is why gphoto2 based setups get stuck around 18 to 20fps on the same camera.

**The virtual camera.** OBS's camera extension has a sink stream that OBS normally writes into. We write into it ourselves, so OBS can stay closed. Frames go in as 1080p BGRA pixel buffers backed by IOSurface, so they cross into the extension without a copy.

**Knowing when an app is watching.** macOS reports whether a camera is "running somewhere", and while we aren't feeding it that flag means exactly "some app opened it", so noticing an app arrive is easy. Noticing it leave is the hard part, because our own feed counts as running too. The only way to check is to pause the feed for a few milliseconds and read the flag again, and OBS's driver paints its logo into any pause an app can see. There is no other signal: CoreMediaIO properties, AVFoundation's in use flag, surface reuse and the driver's CPU all stay the same with or without an app. So the engine only checks when nobody could notice:

- Never during a call. Call apps keep the microphone open the whole time (even muted) or keep the display awake, and both are easy to read.
- Once, 30 seconds after the call ends. Call apps let go of the camera when you hang up, so usually nobody is watching anymore and the camera goes back to resting.
- Whenever you're away: screen locked, display asleep, or no keyboard or mouse for 5 minutes.
- Never while you're at the desk without a call (Photo Booth, a preview window). The tradeoff: close one of those and keep working, and the camera stays live until you step away or lock the screen.

**Control.** The engine listens on a Unix socket (`~/Library/Application Support/canon-open-webcam/control.sock`, readable only by you) that `status`, `set` and `focus` talk to.

Logs live in `~/Library/Logs/canon-open-webcam.log`. Set `CANON_WEBCAM_DEBUG=1` to also log a per frame timing breakdown, and `canon-webcam bench` measures the raw live view rate of whatever camera is plugged in.

## Windows

The Windows app lives in `windows/`, in Rust, and needs Windows 11. Full details in [windows/README.md](windows/README.md).

```
Canon ──USB, WPD (MTP vendor commands)──▶ CanonWebcam.exe: live view ▶ color, crop, 1080p, sharpen, mirror ▶ NV12
                                                     │  shared memory
Zoom, Teams, Meet ◀── Frame Server ◀── canon_vcam.dll (Media Foundation virtual camera)
```

- **Install:** grab `CanonWebcam-<version>-windows-x64.zip` from the [Releases page](https://github.com/thecristopher/canon-open-webcam/releases), unzip it and run `CanonWebcam.exe`. **Set Up Canon Webcam** checks for Windows 11, copies the app into Program Files, registers the virtual camera, starts it at sign in and switches off Canon's EOS Webcam Utility services. Windows asks for permission once.
- **No driver swap, no signed driver.** The camera is reached through Windows' own MTP driver, the same way Canon's EDSDK does it. If Zadig ever moved your Canon to WinUSB, `doctor` spots it and Set Up (or `CanonWebcam fix-driver`) hands it back to Windows.
- **Wakes up on demand, with no guessing.** The virtual camera stamps a heartbeat in shared memory whenever an app asks for a frame, so the engine knows exactly when apps come and go, and rests the camera 30 seconds after the last one leaves.
- **The same app.** Picture front and center with Adjust, Framing, Camera and Output in a sidebar, and a tray icon instead of the menu bar.
- **The same commands:** `CanonWebcam status`, `set`, `focus`, `doctor`, `install`, `uninstall [--restore-canon]`, `start --always-on`, `bench`, plus `check out.bmp` to open the virtual camera like Zoom would and report its frame rate.

Settings live in `%APPDATA%\CanonWebcam\options.json` (same format as the Mac's), logs in `%LOCALAPPDATA%\CanonWebcam\canon-webcam.log` and `C:\ProgramData\CanonWebcam\camera.log`.

## Development

### macOS

```sh
swift build
swift test
```

The code is split into `CanonPTP` (protocol, no hardware), `CanonUSB` (libusb transport), `WebcamEngine` (virtual camera, rendering, the run loop, control socket, launch agent), `SetupKit` (every window: the app, the menu bar panel, the installer and the uninstaller), the `canon-webcam` executable and the three app targets. `scripts/build-apps.sh` turns the app targets into bundles with icons, tucks the app inside the installer, and makes the disk image. The protocol layer is tested against a scripted fake camera, so most of it runs without one plugged in.

### Windows

Needs Rust (`winget install Rustlang.Rustup`) and the Visual Studio C++ build tools with a Windows SDK. From `windows/`:

```powershell
cargo test
cargo build --release                                         # target\release\CanonWebcam.exe and canon_vcam.dll
powershell -ExecutionPolicy Bypass -File scripts\package.ps1  # dist\CanonWebcam-<version>-windows-x64.zip
```

The workspace mirrors the Mac's split: `canon-ptp` (protocol, no OS code), `canon-wpd` (the WPD transport), `canon-shm` (the shared memory contract between engine and camera), `canon-vcam` (the virtual camera DLL) and `canon-webcam` (the tray, engine, windows, control pipe, setup and every command).

## Credits

Made by Cristopher Cervantes.

An independent project, not affiliated with or endorsed by Canon Inc. Canon and EOS are trademarks of Canon Inc., used here only to say which cameras this works with.
