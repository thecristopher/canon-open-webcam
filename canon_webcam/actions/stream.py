import os
import subprocess
import threading
import time
from collections import deque
from typing import Callable, Optional

from .camera import free_camera

RETRY_DELAY_SECONDS = 2
CAMERA_POLL_SECONDS = 3
# Live view takes a few seconds to start, so this has to be longer than that
STALL_SECONDS = 10


def sender_url(port: int) -> str:
    return f"tcp://127.0.0.1:{port}"


def receiver_url(port: int) -> str:
    # OBS listens so every reconnect from us is a fresh stream with fresh NUT headers
    return f"tcp://127.0.0.1:{port}?listen=1"


def gphoto2_command(camera_port: str) -> list[str]:
    # No duration after --capture-movie means stream until we stop it
    return ["gphoto2", "--port", camera_port, "--stdout", "--capture-movie"]


def ffmpeg_command(url: str) -> list[str]:
    # Passing the camera's JPEGs through untouched is ~500ms snappier in OBS than
    # re-encoding to h264, and NUT carries the timestamps raw MJPEG lacks
    return [
        "ffmpeg", "-hide_banner", "-loglevel", "error",
        # Live view frames have no timestamps, so stamp them as they arrive
        "-use_wallclock_as_timestamps", "1",
        "-fflags", "nobuffer", "-flags", "low_delay",
        "-probesize", "32", "-analyzeduration", "0",
        "-f", "mjpeg", "-i", "-",
        "-an", "-c:v", "copy",
        "-f", "nut", url,
    ]


def keep_last_lines(stream, lines: deque) -> None:
    for raw in iter(stream.readline, b""):
        lines.append(raw.decode(errors="replace").rstrip())


class Heartbeat:
    def __init__(self, clock: Callable[[], float] = time.monotonic):
        self.clock = clock
        self.last = clock()

    def beat(self) -> None:
        self.last = self.clock()

    def silent_for(self) -> float:
        return self.clock() - self.last


class TextModeRepair:
    """Undoes the LF -> CRLF rewrite a Windows text mode stdout does to gphoto2's JPEGs.

    gphoto2 never switches stdout to binary mode, so on Windows every 0x0A byte comes out
    as 0x0D 0x0A. Dropping the CR in front of each LF restores the bytes exactly. In real
    binary data a bare LF shows up within the first few, so a clean stream passes through.
    """

    SAMPLE_NEWLINES = 16

    def __init__(self):
        self.pending = b""
        self.translated: Optional[bool] = None

    def feed(self, chunk: bytes) -> bytes:
        data = self.pending + chunk
        if self.translated is None:
            newlines = data.count(b"\n")
            if newlines < self.SAMPLE_NEWLINES:
                self.pending = data
                return b""
            self.translated = newlines == data.count(b"\r\n")
        if not self.translated:
            self.pending = b""
            return data
        # A CR at the very end may belong to an LF in the next chunk
        data, self.pending = (data[:-1], b"\r") if data.endswith(b"\r") else (data, b"")
        return data.replace(b"\r\n", b"\n")

    def flush(self) -> bytes:
        data, self.pending = self.pending, b""
        return data


def relay(source, sink, heartbeat: Heartbeat) -> None:
    # Copying through Python is how we notice gphoto2 hanging without exiting
    repair = TextModeRepair()
    try:
        for chunk in iter(lambda: os.read(source.fileno(), 65536), b""):
            heartbeat.beat()
            sink.write(repair.feed(chunk))
            sink.flush()
        sink.write(repair.flush())
    except (OSError, ValueError):
        # ffmpeg died or we closed the pipes, and the main loop handles both
        pass
    finally:
        try:
            sink.close()
        except OSError:
            pass


def start_processes(camera_port: str, url: str):
    gphoto = subprocess.Popen(
        gphoto2_command(camera_port), stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    ffmpeg = subprocess.Popen(
        ffmpeg_command(url), stdin=subprocess.PIPE, stderr=subprocess.PIPE
    )
    heartbeat = Heartbeat()
    threading.Thread(target=relay, args=(gphoto.stdout, ffmpeg.stdin, heartbeat), daemon=True).start()

    errors: deque = deque(maxlen=5)
    for process in (gphoto, ffmpeg):
        threading.Thread(target=keep_last_lines, args=(process.stderr, errors), daemon=True).start()
    return gphoto, ffmpeg, errors, heartbeat


def stop_processes(*processes) -> None:
    for process in processes:
        if process.poll() is None:
            process.terminate()
    for process in processes:
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()


def run_pipeline(
    find_camera_port: Callable[[], Optional[str]],
    url: str,
    stop: threading.Event,
    keep_going: Callable[[], bool] = lambda: True,
    log: Callable[[str], None] = print,
) -> None:
    waiting = False
    while not stop.is_set() and keep_going():
        # Looked up every time because the USB port number changes on replug
        camera_port = find_camera_port()
        if camera_port is None:
            if not waiting:
                log("Waiting for the camera. Plug it in and turn it on.")
                waiting = True
            stop.wait(CAMERA_POLL_SECONDS)
            continue
        waiting = False

        free_camera()
        gphoto, ffmpeg, errors, heartbeat = start_processes(camera_port, url)
        log("Live view is streaming. Press Ctrl+C to stop.")
        stalled = False
        try:
            while not stop.is_set() and gphoto.poll() is None and ffmpeg.poll() is None:
                if heartbeat.silent_for() > STALL_SECONDS:
                    stalled = True
                    break
                stop.wait(0.25)
        finally:
            stop_processes(gphoto, ffmpeg)

        if stop.is_set():
            return
        if stalled:
            reason = f"no frames for {STALL_SECONDS}s"
        else:
            reason = errors[-1] if errors else "no error output"
        log(f"Stream dropped ({reason}). Retrying in {RETRY_DELAY_SECONDS}s...")
        stop.wait(RETRY_DELAY_SECONDS)
