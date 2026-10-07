import pytest

from canon_webcam.actions import obs_config


def test_enabling_generates_a_password_when_none_is_set():
    enabled = obs_config.with_websocket_enabled({}, make_password=lambda: "ranni")
    assert enabled["server_enabled"] is True
    assert enabled["server_password"] == "ranni"


def test_enabling_keeps_an_existing_password():
    enabled = obs_config.with_websocket_enabled({"server_password": "blaidd"})
    assert enabled["server_password"] == "blaidd"


def test_password_is_none_when_auth_is_off():
    assert obs_config.websocket_password({"auth_required": False, "server_password": "x"}) is None


def test_url_uses_the_configured_port():
    assert obs_config.websocket_url({"server_port": 4460}) == "ws://127.0.0.1:4460"


def test_missing_config_reads_as_empty(tmp_path):
    assert obs_config.read_websocket_config(tmp_path / "nope.json") == {}


def test_turns_websocket_on_when_obs_is_closed(tmp_path):
    path = tmp_path / "config.json"
    obs_config.write_websocket_config({"server_enabled": False}, path)

    obs_config.ensure_websocket_enabled(obs_running=False, path=path)

    assert obs_config.read_websocket_config(path)["server_enabled"] is True


def test_refuses_to_edit_config_while_obs_is_open(tmp_path):
    path = tmp_path / "config.json"
    obs_config.write_websocket_config({"server_enabled": False}, path)

    with pytest.raises(RuntimeError, match="WebSocket Server Settings"):
        obs_config.ensure_websocket_enabled(obs_running=True, path=path)
