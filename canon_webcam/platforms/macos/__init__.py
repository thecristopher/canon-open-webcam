import subprocess
from pathlib import Path
from typing import Optional

from . import launch_agent

NAME = "macOS"
OBS_APP = Path("/Applications/OBS.app")
OBS_WEBSOCKET_CONFIG = (
    Path.home() / "Library/Application Support/obs-studio/plugin_config/obs-websocket/config.json"
)
LOG_PATH = launch_agent.LOG_PATH
SUPPORTS_ON_DEMAND = True

INSTALL_HINTS = {
    "gphoto2": "brew install gphoto2",
    "ffmpeg": "brew install ffmpeg",
    "obs": "brew install --cask obs",
}
CAMERA_HINT = "Plug it in over USB, turn it on, and make sure the EOS Utility or Image Capture is closed."

# macOS daemons that grab any PTP camera the moment it is plugged in
CAMERA_HOGS = ("ptpcamerad", "PTPCamera", "ImageCaptureExtension")


def add_tool_paths() -> None:
    # Homebrew is already on PATH in a terminal, and the LaunchAgent sets its own PATH
    pass


def free_camera() -> None:
    for name in CAMERA_HOGS:
        subprocess.run(["killall", "-9", name], capture_output=True)


def parse_pgrep(output: str) -> list[str]:
    return [line.strip() for line in output.splitlines() if line.strip()]


def process_command_lines(name: str) -> list[str]:
    result = subprocess.run(["pgrep", "-lf", name], capture_output=True, text=True)
    return parse_pgrep(result.stdout)


def obs_pids() -> set[int]:
    result = subprocess.run(["pgrep", "-x", "OBS"], capture_output=True, text=True)
    return {int(pid) for pid in result.stdout.split()}


def obs_location() -> Optional[Path]:
    return OBS_APP if OBS_APP.exists() else None


def launch_obs() -> None:
    subprocess.run(
        ["open", "-g", "-a", "OBS", "--args", "--minimize-to-tray", "--disable-shutdown-check"],
        check=True,
    )


def install() -> list[str]:
    for label in launch_agent.CANON_UTILITY_LABELS:
        launch_agent.disable_service(label)
    launch_agent.install()
    return ["Disabled Canon's EOS Webcam Utility services so they stop grabbing the camera."]


def uninstall() -> list[str]:
    launch_agent.uninstall()
    return ["Canon's services stay disabled. To bring them back:"] + [
        f"  launchctl enable gui/$(id -u)/{label}" for label in launch_agent.CANON_UTILITY_LABELS
    ]


def is_installed() -> bool:
    return launch_agent.is_installed()
