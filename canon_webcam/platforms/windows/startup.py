"""Start at login on Windows without admin rights.

A per user Run key launches this module as a small supervisor, and the supervisor
restarts canon-webcam whenever it exits, the way launchd's KeepAlive does on macOS.
"""
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable

NAME = "canon-open-webcam"
MODULE = "canon_webcam.platforms.windows.startup"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
DATA_DIR = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData/Local")) / NAME
LOG_PATH = DATA_DIR / f"{NAME}.log"
PID_PATH = DATA_DIR / "supervisor.pid"
# Keeps a crash loop from hammering the camera and OBS
THROTTLE_SECONDS = 10

NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)
DETACHED = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)


def sibling_python(name: str) -> str:
    candidate = Path(sys.executable).with_name(name)
    return str(candidate if candidate.exists() else sys.executable)


def supervisor_command() -> list[str]:
    # pythonw has no console, so logging in doesn't pop up a window
    return [sibling_python("pythonw.exe"), "-m", MODULE]


def agent_command() -> list[str]:
    return [sibling_python("python.exe"), "-m", "canon_webcam", "start", "--wait"]


def supervise(sleep: Callable[[float], None] = time.sleep, keep_going: Callable[[], bool] = lambda: True) -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    PID_PATH.write_text(str(os.getpid()))
    while keep_going():
        started = time.monotonic()
        with LOG_PATH.open("ab") as log:
            subprocess.run(agent_command(), stdout=log, stderr=subprocess.STDOUT, creationflags=NO_WINDOW)
        sleep(max(0.0, THROTTLE_SECONDS - (time.monotonic() - started)))


def is_our_supervisor(pid: int) -> bool:
    # A stale pid file can point at a reused pid, so check before killing anything
    script = f"(Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\").CommandLine"
    result = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True, text=True,
    )
    return MODULE in result.stdout


def stop() -> None:
    try:
        pid = int(PID_PATH.read_text())
    except (OSError, ValueError):
        return
    if is_our_supervisor(pid):
        # /T takes canon-webcam, gphoto2 and ffmpeg down too. OBS was started
        # through `start`, so it isn't in the tree and keeps running.
        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True)
    PID_PATH.unlink(missing_ok=True)


def install() -> None:
    import winreg

    stop()
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    command = subprocess.list2cmdline(supervisor_command())
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as key:
        winreg.SetValueEx(key, NAME, 0, winreg.REG_SZ, command)
    # Run entries only fire at the next login, so start it now too
    subprocess.Popen(supervisor_command(), creationflags=DETACHED, close_fds=True)


def uninstall() -> None:
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
            winreg.DeleteValue(key, NAME)
    except FileNotFoundError:
        pass
    stop()


def is_installed() -> bool:
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as key:
            winreg.QueryValueEx(key, NAME)
        return True
    except FileNotFoundError:
        return False


if __name__ == "__main__":
    supervise()
