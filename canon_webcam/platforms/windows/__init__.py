import csv
import os
import subprocess
from pathlib import Path
from typing import Optional

from . import startup

NAME = "Windows"
# MSYS2 is where the open source gphoto2 build for Windows comes from, and it isn't on PATH
MSYS2_BIN = Path(os.environ.get("MSYS2_ROOT", r"C:\msys64")) / "ucrt64" / "bin"
OBS_DIR = Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "obs-studio" / "bin" / "64bit"
OBS_EXE = "obs64.exe"
OBS_WEBSOCKET_CONFIG = (
    Path(os.environ.get("APPDATA", Path.home() / "AppData/Roaming"))
    / "obs-studio/plugin_config/obs-websocket/config.json"
)
LOG_PATH = startup.LOG_PATH
# Windows has no way to see an app opening OBS Virtual Camera while it is off
SUPPORTS_ON_DEMAND = False

INSTALL_HINTS = {
    "gphoto2": "winget install MSYS2.MSYS2, then in the MSYS2 UCRT64 shell: pacman -S mingw-w64-ucrt-x86_64-gphoto2",
    "ffmpeg": "winget install Gyan.FFmpeg",
    "obs": "winget install OBSProject.OBSStudio",
}
CAMERA_HINT = (
    "Plug it in over USB and turn it on. If it still isn't found, or gphoto2 says it could not "
    "claim the USB device, Windows' photo driver owns it: switch it to WinUSB with Zadig (see the README)."
)


def newest_version_dir(parent: Path) -> Optional[Path]:
    versions = [child for child in parent.glob("*") if child.is_dir()]
    return max(versions, key=lambda child: [int(part) for part in child.name.split(".") if part.isdigit()], default=None)


def add_tool_paths() -> None:
    path = os.environ.get("PATH", "").split(os.pathsep)
    if MSYS2_BIN.is_dir() and str(MSYS2_BIN) not in path:
        os.environ["PATH"] = os.pathsep.join([*path, str(MSYS2_BIN)])
    # MSYS2's gphoto2 looks for its drivers under the build machine's path, so it finds
    # no cameras at all unless we point it at the installed ones
    lib = MSYS2_BIN.parent / "lib"
    for variable, folder in (("CAMLIBS", "libgphoto2"), ("IOLIBS", "libgphoto2_port")):
        found = newest_version_dir(lib / folder)
        if found and variable not in os.environ:
            os.environ[variable] = str(found)


def free_camera() -> None:
    # Once the camera is on WinUSB, Windows has no background service that grabs it
    pass


def powershell(script: str) -> str:
    result = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True, text=True,
    )
    return result.stdout


def process_command_lines(name: str) -> list[str]:
    script = (
        f"Get-CimInstance Win32_Process -Filter \"Name='{name}.exe'\" | "
        "ForEach-Object { \"$($_.ProcessId) $($_.CommandLine)\" }"
    )
    return [line.strip() for line in powershell(script).splitlines() if line.strip()]


def parse_tasklist(output: str) -> set[int]:
    # Rows look like "obs64.exe","1234","Console","1","250,000 K"; no match prints an INFO line
    return {int(row[1]) for row in csv.reader(output.splitlines()) if len(row) > 1 and row[1].isdigit()}


def obs_pids() -> set[int]:
    result = subprocess.run(
        ["tasklist", "/FI", f"IMAGENAME eq {OBS_EXE}", "/FO", "CSV", "/NH"],
        capture_output=True, text=True,
    )
    return parse_tasklist(result.stdout)


def obs_location() -> Optional[Path]:
    exe = OBS_DIR / OBS_EXE
    return exe if exe.exists() else None


def launch_obs_command(exe: Path) -> list[str]:
    # OBS only finds its own files when started from its folder, and `start` detaches it
    # so it outlives us, just like `open -a` on macOS
    return [
        "cmd", "/c", "start", "", "/min", "/d", str(exe.parent), str(exe),
        "--minimize-to-tray", "--disable-shutdown-check",
    ]


def launch_obs() -> None:
    exe = obs_location()
    if exe is None:
        raise RuntimeError(f"OBS isn't installed. Install it with: {INSTALL_HINTS['obs']}")
    subprocess.run(launch_obs_command(exe), check=True)


def install() -> list[str]:
    startup.install()
    return []


def uninstall() -> list[str]:
    startup.uninstall()
    return []


def is_installed() -> bool:
    return startup.is_installed()
