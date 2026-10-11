import re
import subprocess
from typing import NamedTuple, Optional

from .. import platforms


class Camera(NamedTuple):
    model: str
    port: str


def free_camera() -> None:
    platforms.current.free_camera()


def other_camera_users() -> list[str]:
    # Only one process can hold live view, and a stray one makes us fail silently
    return [line for line in platforms.current.process_command_lines("gphoto2") if "--capture-movie" in line]


def parse_auto_detect(output: str) -> list[Camera]:
    lines = output.splitlines()
    # The header is localized, so the dashed divider is the only reliable anchor
    divider = next((i for i, line in enumerate(lines) if line.startswith("---")), None)
    if divider is None:
        return []

    cameras = []
    for line in lines[divider + 1 :]:
        parts = re.split(r"\s{2,}", line.strip())
        if len(parts) >= 2:
            cameras.append(Camera(model=parts[0], port=parts[-1]))
    return cameras


def list_cameras() -> list[Camera]:
    result = subprocess.run(["gphoto2", "--auto-detect"], capture_output=True, text=True)
    return parse_auto_detect(result.stdout)


def find_camera_port() -> Optional[str]:
    free_camera()
    camera = pick_camera(list_cameras())
    return camera.port if camera else None


def pick_camera(cameras: list[Camera]) -> Optional[Camera]:
    canons = [camera for camera in cameras if "canon" in camera.model.lower()]
    if canons:
        return canons[0]
    return cameras[0] if cameras else None
