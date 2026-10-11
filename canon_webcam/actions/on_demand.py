import threading
import time
from typing import Callable

# On demand only exists on macOS, where CoreMediaIO reports who opened the camera
from ..platforms.macos import presence
from .obs_setup import start_virtual_cam, stop_virtual_cam
from .stream import run_pipeline

POLL_SECONDS = 2
# Counts as "you came back" after an idle shutdown
RECENT_INPUT_SECONDS = 10


class Signals:
    """Everything the on demand loop asks the outside world, so tests can fake it."""

    def camera_requested(self) -> bool:
        return presence.virtual_camera_in_use(presence.camera_devices())

    def idle_seconds(self) -> float:
        return presence.seconds_since_input()

    def call_in_progress(self) -> bool:
        return presence.call_in_progress(presence.read_assertions(), presence.obs_pids())


def should_start(signals: Signals, stopped_for_idle: bool) -> bool:
    if not signals.camera_requested():
        return False
    # A forgotten tab can hold the camera open forever, so wait for the user to come back
    return not stopped_for_idle or signals.idle_seconds() < RECENT_INPUT_SECONDS


def should_stop(signals: Signals, idle_limit_seconds: float) -> bool:
    return signals.idle_seconds() >= idle_limit_seconds and not signals.call_in_progress()


class LiveSession:
    def __init__(self, obs, find_camera_port, url: str, log: Callable[[str], None]):
        self.obs = obs
        self.stop_event = threading.Event()
        self.thread = threading.Thread(
            target=run_pipeline, args=(find_camera_port, url, self.stop_event),
            kwargs={"log": log}, daemon=True,
        )

    def start(self) -> None:
        self.thread.start()
        start_virtual_cam(self.obs)

    def stop(self) -> None:
        stop_virtual_cam(self.obs)
        self.stop_event.set()
        self.thread.join(timeout=10)


def run_on_demand(
    obs,
    find_camera_port,
    url: str,
    keep_going: Callable[[], bool],
    idle_limit_seconds: float,
    signals: Signals = None,
    log: Callable[[str], None] = print,
    sleep: Callable[[float], None] = time.sleep,
) -> None:
    signals = signals or Signals()
    session = None
    stopped_for_idle = False
    log("Standing by. The camera starts when an app opens OBS Virtual Camera.")
    try:
        while keep_going():
            if session is None and should_start(signals, stopped_for_idle):
                log("An app opened the camera. Going live.")
                session = LiveSession(obs, find_camera_port, url, log)
                session.start()
                stopped_for_idle = False
            elif session is not None and should_stop(signals, idle_limit_seconds):
                log("You went idle and no call is running. Pausing so the Mac can sleep.")
                session.stop()
                session = None
                stopped_for_idle = True
            sleep(POLL_SECONDS)
    finally:
        if session is not None:
            session.stop()
