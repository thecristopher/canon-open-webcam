import json
import secrets
from pathlib import Path
from typing import Callable, Optional

from .. import platforms

CONFIG_PATH = platforms.current.OBS_WEBSOCKET_CONFIG
DEFAULT_PORT = 4455


def read_websocket_config(path: Path = CONFIG_PATH) -> dict:
    if not path.exists():
        return {}
    return json.loads(path.read_text())


def websocket_is_enabled(config: dict) -> bool:
    return bool(config.get("server_enabled"))


def with_websocket_enabled(
    config: dict, make_password: Callable[[], str] = lambda: secrets.token_urlsafe(16)
) -> dict:
    enabled = {**config, "server_enabled": True, "first_load": False}
    enabled.setdefault("server_port", DEFAULT_PORT)
    enabled.setdefault("auth_required", True)
    if enabled["auth_required"] and not enabled.get("server_password"):
        enabled["server_password"] = make_password()
    return enabled


def write_websocket_config(config: dict, path: Path = CONFIG_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(config, indent=2))


def websocket_url(config: dict) -> str:
    return f"ws://127.0.0.1:{config.get('server_port', DEFAULT_PORT)}"


def websocket_password(config: dict) -> Optional[str]:
    if not config.get("auth_required", True):
        return None
    return config.get("server_password") or None


def ensure_websocket_enabled(obs_running: bool, path: Path = CONFIG_PATH) -> dict:
    config = read_websocket_config(path)
    if websocket_is_enabled(config):
        return config
    if obs_running:
        # OBS rewrites this file on quit, so editing it while OBS is open gets undone
        raise RuntimeError(
            "OBS websocket server is off. Either quit OBS and rerun this, or enable it in "
            "OBS under Tools > WebSocket Server Settings."
        )
    enabled = with_websocket_enabled(config)
    write_websocket_config(enabled, path)
    return enabled


def obs_is_running() -> bool:
    return bool(platforms.current.obs_pids())


def launch_obs() -> None:
    platforms.current.launch_obs()
