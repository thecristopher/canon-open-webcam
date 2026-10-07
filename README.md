# canon-open-webcam

Use a Canon EOS camera (built on a Rebel T7 / 2000D) as a webcam on macOS without paying for EOS Webcam Utility Pro. Every piece is open source: gphoto2 pulls the live view over USB, ffmpeg wraps the frames for streaming, and OBS Virtual Camera exposes it to Zoom, Meet, FaceTime, Teams, Discord, and friends.

```
Canon over USB ──gphoto2 (MJPEG live view)──▶ ffmpeg (MJPEG passthrough in NUT, TCP localhost) ──▶ OBS media source ──▶ OBS Virtual Camera
```

## Install

```sh
brew install gphoto2 ffmpeg
brew install --cask obs
python3 -m venv .venv && .venv/bin/pip install -e .
```

## Use

1. Plug the camera in over USB and turn it on. Close EOS Utility, Image Capture and Photos.
2. Run it:

   ```sh
   .venv/bin/canon-webcam doctor   # check everything is found
   .venv/bin/canon-webcam          # go live
   ```

3. Pick **OBS Virtual Camera** as the camera in your meeting app.
4. `Ctrl+C` to stop. The virtual camera turns off with it unless you pass `--keep-virtualcam`.

The first run turns on the OBS websocket server for you (only if OBS is closed, since OBS rewrites its config on quit), launches OBS in the background, creates a `Canon Open Webcam` scene with the live feed scaled to the canvas, and starts the virtual camera. Later runs just reuse that scene.

The very first time OBS Virtual Camera starts, macOS asks you to allow its system extension in **System Settings > General > Login Items & Extensions**. That is a one time approval.

### Always on

```sh
.venv/bin/canon-webcam install     # start at login, restart if anything stops
.venv/bin/canon-webcam uninstall   # undo it
```

This installs a LaunchAgent (`~/Library/LaunchAgents/dev.canon-open-webcam.plist`, no sudo). It waits quietly while the camera is off or unplugged, picks it back up when it returns, and relaunches OBS if OBS gets closed. It also disables Canon's EOS Webcam Utility services for your user so they stop stealing the camera at login. Logs live in `~/Library/Logs/canon-open-webcam.log`.

While OBS Virtual Camera is on, OBS keeps the display from idle sleeping. Turn the virtual camera off in OBS when you don't need it and the Mac sleeps normally again. The agent won't turn it back on until the next login.

### On demand (opt in, experimental)

`canon-webcam start --on-demand` keeps the virtual camera off until an app opens it, then goes live within a couple of seconds. It pauses after `--idle-minutes` (default 5) of no keyboard or mouse input, but only while no other app is holding the mic or keeping the screen awake. That call detection is a heuristic and hasn't been tested against real calls. A muted browser call or a hands free recording could get paused, so the login agent doesn't use it.

How "an app opened the camera" works: with the virtual camera off, macOS still reports OBS Virtual Camera as running when any app reads from it (CoreMediaIO `DeviceIsRunningSomewhere`, read through ctypes). Once OBS feeds the camera that flag is always on, which is why stopping has to rely on idle time and call detection.

The agent points at this checkout's `.venv`, so rerun `install` if you move the folder.

macOS has no system wide default camera. Pick **OBS Virtual Camera** once in each app and it remembers.

### Options

| Flag | What it does |
| --- | --- |
| `--port 5600` | Local TCP port the stream travels over |
| `--obs-password` | Use this instead of the password in OBS config |
| `--no-obs` | Stream only. Start a preview first with `ffplay -fflags nobuffer -f nut 'tcp://127.0.0.1:5600?listen=1'` |
| `--keep-virtualcam` | Leave OBS Virtual Camera running on exit |
| `--wait` | Wait for the camera instead of failing when it is missing |
| `--on-demand` | Experimental. Only run while an app is using OBS Virtual Camera, see above |
| `--idle-minutes 5` | With `--on-demand`, pause after this much idle time when no call is running |

## Camera tips

- Turn **Auto power off** to Disable in the camera menu, or it will fall asleep mid meeting.
- A dummy battery (ACK-E10 for the T7) keeps it running all day.
- Movie mode on the dial gives a 16:9 live view; photo mode gives 3:2.
- Live view over USB tops out around 1056x704. That still looks way better than a laptop webcam thanks to the sensor and lens.
- If the stream keeps dropping, a macOS daemon is probably fighting for the camera. The tool kills `ptpcamerad` before each connect, and retries automatically if the feed drops.

## Why no re-encoding

The camera already sends JPEG frames, so they go to OBS untouched. Measured with timestamped test frames:

| Pipeline | Frame to OBS latency |
| --- | --- |
| h264 in mpegts over UDP | ~750ms |
| MJPEG passthrough in NUT over UDP | ~200ms, but OBS can't rejoin midstream |
| MJPEG passthrough in NUT over TCP, OBS listening | ~150ms, survives reconnects |

NUT only writes its headers at the start of a stream, so OBS listens and we connect. Every reconnect is then a fresh stream OBS can decode right away.

The other big chunk was OBS itself. Every hop from OBS's render to the virtual camera (GPU readback, output queue, the camera extension) holds whole canvas frames, so a 30fps canvas added about 150ms before an app ever saw the frame. At 60fps that hop drops to about 80ms, even though the camera only sends ~22fps. `canon-webcam` sets the OBS canvas to 60fps for you. 120fps saved only ~15ms more, and the virtual camera tops out at 60.

## Development

```sh
.venv/bin/pip install -e ".[dev]"
.venv/bin/pytest
```
