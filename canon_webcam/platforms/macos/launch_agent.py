import os
import plistlib
import subprocess
import sys
import time
from pathlib import Path

LABEL = "dev.canon-open-webcam"
PLIST_PATH = Path.home() / "Library/LaunchAgents" / f"{LABEL}.plist"
LOG_PATH = Path.home() / "Library/Logs/canon-open-webcam.log"
# launchd starts agents with a bare PATH that can't see Homebrew
AGENT_PATH = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

# Canon's own webcam services grab the camera at login and fight us for it
CANON_UTILITY_LABELS = ("com.canon.usa.EWCService", "com.canon.usa.EWCPairingService")


def gui_domain() -> str:
    return f"gui/{os.getuid()}"


def agent_command() -> list[str]:
    return [sys.executable, "-m", "canon_webcam", "start", "--wait"]


def build_plist(command: list[str], log_path: Path = LOG_PATH) -> dict:
    return {
        "Label": LABEL,
        "ProgramArguments": command,
        "RunAtLoad": True,
        "KeepAlive": True,
        # Keeps a crash loop from hammering the camera and OBS
        "ThrottleInterval": 10,
        "ProcessType": "Interactive",
        "EnvironmentVariables": {"PATH": AGENT_PATH},
        "StandardOutPath": str(log_path),
        "StandardErrorPath": str(log_path),
    }


def launchctl(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["launchctl", *args], capture_output=True, text=True)


def disable_service(label: str) -> None:
    # disable survives reboots, bootout stops the copy running right now
    launchctl("disable", f"{gui_domain()}/{label}")
    launchctl("bootout", f"{gui_domain()}/{label}")


def is_loaded() -> bool:
    return launchctl("print", f"{gui_domain()}/{LABEL}").returncode == 0


def unload_and_wait(timeout: float = 15) -> None:
    launchctl("bootout", f"{gui_domain()}/{LABEL}")
    # bootout returns before the old agent exits, and bootstrapping over it fails with error 5
    deadline = time.monotonic() + timeout
    while is_loaded() and time.monotonic() < deadline:
        time.sleep(0.5)


def install(plist_path: Path = PLIST_PATH) -> None:
    unload_and_wait()
    plist_path.parent.mkdir(parents=True, exist_ok=True)
    LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    plist_path.write_bytes(plistlib.dumps(build_plist(agent_command())))
    launchctl("enable", f"{gui_domain()}/{LABEL}")
    result = launchctl("bootstrap", gui_domain(), str(plist_path))
    if result.returncode != 0:
        raise RuntimeError(f"launchctl bootstrap failed: {result.stderr.strip()}")


def uninstall(plist_path: Path = PLIST_PATH) -> None:
    unload_and_wait()
    plist_path.unlink(missing_ok=True)


def is_installed(plist_path: Path = PLIST_PATH) -> bool:
    return plist_path.exists()
