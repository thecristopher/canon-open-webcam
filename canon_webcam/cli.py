import argparse
import shutil
import signal
import sys
import threading
from pathlib import Path

from . import actions

REQUIRED_TOOLS = ("gphoto2", "ffmpeg")
OBS_APP = "/Applications/OBS.app"


def parse_args(argv=None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="canon-webcam",
        description="Use a Canon EOS camera as a webcam through OBS Virtual Camera.",
    )
    sub = parser.add_subparsers(dest="command")

    start = sub.add_parser("start", help="stream live view into OBS Virtual Camera (default)")
    start.add_argument("--port", type=int, default=5600, help="local TCP port for the video stream")
    start.add_argument("--obs-password", help="override the password read from OBS config")
    start.add_argument("--no-obs", action="store_true", help="only stream, useful with ffplay")
    start.add_argument("--keep-virtualcam", action="store_true", help="leave OBS Virtual Camera on at exit")
    start.add_argument("--wait", action="store_true", help="wait for the camera instead of failing when it's missing")
    start.add_argument("--on-demand", action="store_true", help="only run while an app is using OBS Virtual Camera")
    start.add_argument("--idle-minutes", type=float, default=5, help="with --on-demand, pause after this much idle time")

    sub.add_parser("doctor", help="check that everything needed is installed and plugged in")
    sub.add_parser("install", help="start automatically at login and keep running")
    sub.add_parser("uninstall", help="stop starting at login")

    argv = list(sys.argv[1:] if argv is None else argv)
    # Bare `canon-webcam --no-obs` should still mean start
    if not argv or argv[0] not in ("start", "doctor", "install", "uninstall", "-h", "--help"):
        argv = ["start", *argv]
    return parser.parse_args(argv)


def missing_tools() -> list[str]:
    return [tool for tool in REQUIRED_TOOLS if shutil.which(tool) is None]


def find_camera() -> actions.Camera:
    busy = actions.other_camera_users()
    if busy:
        raise RuntimeError("Another process is already streaming from the camera:\n  " + "\n  ".join(busy))
    actions.free_camera_from_macos()
    camera = actions.pick_camera(actions.list_cameras())
    if camera is None:
        raise RuntimeError(
            "No camera found. Plug it in over USB, turn it on, and make sure the EOS Utility "
            "or Image Capture is closed."
        )
    return camera


def open_obs(password_override):
    running = actions.obs_is_running()
    config = actions.ensure_websocket_enabled(running)
    if not running:
        print("Launching OBS...")
        actions.launch_obs()
    password = password_override or actions.websocket_password(config)
    return actions.connect_to_obs(actions.websocket_url(config), password)


def close_obs_connection(obs, keep_virtualcam: bool) -> None:
    try:
        if not keep_virtualcam:
            actions.stop_virtual_cam(obs)
        obs.close()
    except Exception:
        # OBS may already be gone, and then there is nothing left to clean up
        pass


def start(args) -> int:
    if not args.wait:
        camera = find_camera()
        print(f"Found {camera.model} on {camera.port}")
    url = actions.sender_url(args.port)
    receiver = actions.receiver_url(args.port)

    if args.on_demand and args.no_obs:
        raise RuntimeError("--on-demand needs OBS, so it can't be combined with --no-obs.")
    if args.on_demand:
        return start_on_demand(args, url, receiver)

    obs = None
    if not args.no_obs:
        obs = open_obs(args.obs_password)
        actions.setup_obs(obs, receiver)
        print('OBS Virtual Camera is on. Pick "OBS Virtual Camera" in Zoom, Meet, or FaceTime.')
    else:
        print(f"Streaming to a listener on port {args.port}. Preview with: ffplay -fflags nobuffer -f nut '{receiver}'")

    keep_going = actions.obs_is_running if obs is not None else (lambda: True)
    try:
        actions.run_pipeline(actions.find_camera_port, url, threading.Event(), keep_going)
    except KeyboardInterrupt:
        print("\nStopping.")
        return 0
    finally:
        if obs is not None:
            close_obs_connection(obs, args.keep_virtualcam)

    # Nonzero so the login agent restarts us, which brings OBS back too
    print("OBS was closed, stopping.")
    return 1


def start_on_demand(args, url: str, receiver: str) -> int:
    obs = open_obs(args.obs_password)
    actions.setup_obs(obs, receiver, start_virtualcam=False)
    # The in use check only works while OBS isn't feeding the camera itself
    actions.stop_virtual_cam(obs)
    for name in actions.disable_global_mics(obs):
        print(f"Disabled OBS audio input {name!r} so it stops holding the mic and blocking sleep.")
    try:
        actions.run_on_demand(
            obs, actions.find_camera_port, url,
            keep_going=actions.obs_is_running,
            idle_limit_seconds=args.idle_minutes * 60,
        )
    except KeyboardInterrupt:
        print("\nStopping.")
        return 0
    finally:
        close_obs_connection(obs, keep_virtualcam=False)

    # Nonzero so the login agent restarts us, which brings OBS back too
    print("OBS was closed, stopping.")
    return 1


def install() -> int:
    for label in actions.CANON_UTILITY_LABELS:
        actions.disable_service(label)
    print("Disabled Canon's EOS Webcam Utility services so they stop grabbing the camera.")
    actions.install()
    print("Installed. canon-webcam now starts at login and restarts itself if it stops.")
    print(f"Logs: {actions.LOG_PATH}")
    return 0


def uninstall() -> int:
    actions.uninstall()
    print("Uninstalled. canon-webcam will no longer start at login.")
    print("Canon's services stay disabled. To bring them back:")
    for label in actions.CANON_UTILITY_LABELS:
        print(f"  launchctl enable gui/$(id -u)/{label}")
    return 0


def doctor() -> int:
    healthy = True
    for tool in REQUIRED_TOOLS:
        found = shutil.which(tool)
        healthy &= bool(found)
        print(f"{'ok' if found else 'MISSING':8} {tool} {found or '(brew install ' + tool + ')'}")

    obs_installed = Path(OBS_APP).exists()
    healthy &= obs_installed
    print(f"{'ok':8} OBS {OBS_APP}" if obs_installed else f"{'MISSING':8} OBS (brew install --cask obs)")

    agent = "installed" if actions.is_installed() else "not installed (run: canon-webcam install)"
    print(f"{'info':8} start at login {agent}")

    config = actions.read_websocket_config()
    state = "on" if actions.websocket_is_enabled(config) else "off (start will turn it on if OBS is closed)"
    print(f"{'info':8} OBS websocket {state}")

    if not missing_tools():
        actions.free_camera_from_macos()
        cameras = actions.list_cameras()
        healthy &= bool(cameras)
        for camera in cameras:
            print(f"{'ok':8} camera {camera.model} on {camera.port}")
        if not cameras:
            print(f"{'MISSING':8} camera (plug it in over USB and turn it on)")
    return 0 if healthy else 1


def stop_on_sigterm(signum, frame):
    raise KeyboardInterrupt


def main(argv=None) -> int:
    # launchd and kill send SIGTERM, which should clean up just like Ctrl+C
    signal.signal(signal.SIGTERM, stop_on_sigterm)
    sys.stdout.reconfigure(line_buffering=True)
    args = parse_args(argv)
    if args.command == "doctor":
        return doctor()
    if args.command == "uninstall":
        return uninstall()

    missing = missing_tools()
    if missing:
        print(f"Missing tools: {', '.join(missing)}. Install with: brew install {' '.join(missing)}")
        return 1
    try:
        if args.command == "install":
            return install()
        return start(args)
    except (RuntimeError, actions.ObsError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
