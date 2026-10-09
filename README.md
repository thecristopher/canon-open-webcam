# canon-open-webcam

Use a Canon EOS camera (built and tested on a Rebel T7 / 2000D / 1500D) as a webcam on macOS, for free, with nothing in the way. One small native binary talks to the camera over USB and feeds the picture straight into a virtual camera that Zoom, Meet, FaceTime, Teams, Discord and friends can pick.

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
- **Light.** One 600KB binary. About 20% of one CPU core while live, nothing while idle.

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
brew install libusb
brew install --cask obs
```

Open OBS once, click **Start Virtual Camera**, and approve the extension in **System Settings > General > Login Items & Extensions**. Then quit OBS for good. That one time approval is the only reason OBS is installed.

```sh
make install          # builds, then starts it at login
canon-webcam doctor   # checks everything is in place
```

`install` copies the binary to `~/Library/Application Support/canon-open-webcam/`, links `canon-webcam` into `~/.local/bin`, adds a LaunchAgent (`~/Library/LaunchAgents/dev.canon-open-webcam.plist`, no sudo) that keeps it running, and switches off Canon's EOS Webcam Utility services so they stop grabbing the camera at login. Rebuilding or moving this folder never breaks it. `make uninstall` undoes it.

## Use

Plug the camera in over USB, turn it on, and pick **OBS Virtual Camera** in your meeting app. That's it. macOS has no system wide default camera, so pick it once per app and each app remembers.

```sh
canon-webcam status                   # what's going on, plus every setting and its choices
canon-webcam set exposure +1          # brighter
canon-webcam set white-balance daylight
canon-webcam set iso 400              # only in M mode, see below
canon-webcam set aperture f/5.6
canon-webcam set shutter 1/60
canon-webcam set color-temp 5200K
canon-webcam set mirror on            # flips what everyone else sees, apps already mirror your self view
canon-webcam set zoom 1.5             # digital zoom toward the center, 1 to 3
canon-webcam focus                    # autofocus
canon-webcam focus near 2             # nudge focus, steps 1 to 3
```

Settings only change while the camera is live. ISO, aperture and shutter belong to the camera's mode dial: in movie mode with auto exposure the camera owns them and `set` tells you so. Turn the dial to **M** for full manual control. Exposure compensation and white balance work in any mode. Mirror and zoom are remembered between runs, everything else the camera remembers itself.

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

## Development

```sh
swift build
swift test
```

The code is split into `CanonPTP` (protocol, no hardware), `CanonUSB` (libusb transport), `WebcamEngine` (virtual camera, rendering, the run loop, control socket, launch agent) and the `canon-webcam` executable. The protocol layer is tested against a scripted fake camera, so most of it runs without one plugged in.
