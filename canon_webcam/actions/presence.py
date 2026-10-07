import ctypes
import ctypes.util
import re
import struct
import subprocess
from typing import NamedTuple, Optional

VIRTUAL_CAMERA_NAME = "OBS Virtual Camera"
# Held by call apps (Zoom, FaceTime, browsers in WebRTC calls) to keep the screen on
DISPLAY_SLEEP_TYPES = {"PreventUserIdleDisplaySleep", "NoDisplaySleepAssertion"}
# These hold assertions for reasons that have nothing to do with a call
NOT_CALL_APPS = {"OBS", "caffeinate", "powerd", "WindowServer", "bluetoothd"}


class Assertion(NamedTuple):
    owner: str
    kind: str
    created_for_pid: Optional[int]


ASSERTION_LINE = re.compile(r"^\s*pid \d+\((?P<owner>[^)]+)\): \[[^\]]*\] [\d:]+ (?P<kind>\w+)")
CREATED_FOR_LINE = re.compile(r"Created for PID: (?P<pid>\d+)")


def parse_assertions(output: str) -> list[Assertion]:
    assertions = []
    for line in output.splitlines():
        match = ASSERTION_LINE.match(line)
        if match:
            assertions.append(Assertion(match["owner"], match["kind"], None))
            continue
        created_for = CREATED_FOR_LINE.search(line)
        if created_for and assertions:
            assertions[-1] = assertions[-1]._replace(created_for_pid=int(created_for["pid"]))
    return assertions


def read_assertions() -> list[Assertion]:
    result = subprocess.run(["pmset", "-g", "assertions"], capture_output=True, text=True)
    return parse_assertions(result.stdout)


def call_in_progress(assertions: list[Assertion], obs_pids: set[int]) -> bool:
    for assertion in assertions:
        if assertion.owner in NOT_CALL_APPS:
            continue
        if assertion.kind in DISPLAY_SLEEP_TYPES:
            return True
        # coreaudiod holds the mic on behalf of whichever app opened it
        if assertion.owner == "coreaudiod" and assertion.created_for_pid not in obs_pids:
            if assertion.created_for_pid is not None:
                return True
    return False


def parse_idle_seconds(ioreg_output: str) -> float:
    match = re.search(r'"HIDIdleTime" = (\d+)', ioreg_output)
    return int(match[1]) / 1e9 if match else 0.0


def seconds_since_input() -> float:
    result = subprocess.run(["ioreg", "-c", "IOHIDSystem", "-d", "4"], capture_output=True, text=True)
    return parse_idle_seconds(result.stdout)


def obs_pids() -> set[int]:
    result = subprocess.run(["pgrep", "-x", "OBS"], capture_output=True, text=True)
    return {int(pid) for pid in result.stdout.split()}


def virtual_camera_in_use(devices: dict[str, bool]) -> bool:
    return any(running for name, running in devices.items() if VIRTUAL_CAMERA_NAME in name)


# CoreMediaIO through ctypes, so we can ask macOS which cameras an app is reading
# without pulling in pyobjc. Only meaningful while OBS Virtual Camera is off,
# since OBS feeding the camera also counts as "running somewhere".

class _PropertyAddress(ctypes.Structure):
    _fields_ = [("selector", ctypes.c_uint32), ("scope", ctypes.c_uint32), ("element", ctypes.c_uint32)]


def _fourcc(code: str) -> int:
    return struct.unpack(">I", code.encode())[0]


_SYSTEM_OBJECT = 1
_UTF8 = 0x08000100
_cmio = None
_cf = None


def _load_frameworks():
    global _cmio, _cf
    if _cmio is not None:
        return
    _cmio = ctypes.CDLL("/System/Library/Frameworks/CoreMediaIO.framework/CoreMediaIO")
    _cf = ctypes.CDLL(ctypes.util.find_library("CoreFoundation"))
    address = ctypes.POINTER(_PropertyAddress)
    _cmio.CMIOObjectGetPropertyDataSize.argtypes = [
        ctypes.c_uint32, address, ctypes.c_uint32, ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    _cmio.CMIOObjectGetPropertyData.argtypes = [
        ctypes.c_uint32, address, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32,
        ctypes.POINTER(ctypes.c_uint32), ctypes.c_void_p]
    _cmio.CMIOObjectSetPropertyData.argtypes = [
        ctypes.c_uint32, address, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p]
    _cf.CFStringGetCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
    _cf.CFRelease.argtypes = [ctypes.c_void_p]

    # Camera extensions stay hidden from CMIO clients unless we opt in
    allow = _PropertyAddress(_fourcc("sscd"), _fourcc("glob"), 0)
    yes = ctypes.c_uint32(1)
    _cmio.CMIOObjectSetPropertyData(_SYSTEM_OBJECT, ctypes.byref(allow), 0, None, 4, ctypes.byref(yes))


def _property(obj: int, selector: str) -> Optional[bytes]:
    address = _PropertyAddress(_fourcc(selector), _fourcc("glob"), 0)
    size = ctypes.c_uint32(0)
    if _cmio.CMIOObjectGetPropertyDataSize(obj, ctypes.byref(address), 0, None, ctypes.byref(size)):
        return None
    buffer = ctypes.create_string_buffer(size.value)
    used = ctypes.c_uint32(0)
    if _cmio.CMIOObjectGetPropertyData(obj, ctypes.byref(address), 0, None, size.value, ctypes.byref(used), buffer):
        return None
    return buffer.raw[: used.value]


def _device_name(device: int) -> str:
    raw = _property(device, "lnam")
    if not raw:
        return ""
    name_ref = struct.unpack("<Q", raw)[0]
    text = ctypes.create_string_buffer(256)
    _cf.CFStringGetCString(name_ref, text, 256, _UTF8)
    # We poll every couple of seconds, so the copied name must be released
    _cf.CFRelease(name_ref)
    return text.value.decode()


def camera_devices() -> dict[str, bool]:
    _load_frameworks()
    devices = {}
    for (device,) in struct.iter_unpack("<I", _property(_SYSTEM_OBJECT, "dev#") or b""):
        running = _property(device, "gone")
        devices[_device_name(device)] = bool(running and struct.unpack("<I", running)[0])
    return devices
