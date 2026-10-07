import plistlib

from canon_webcam.actions import launch_agent


def test_agent_starts_at_login_and_stays_alive():
    plist = launch_agent.build_plist(["canon-webcam"])
    assert plist["RunAtLoad"] is True
    assert plist["KeepAlive"] is True


def test_agent_can_find_homebrew_tools():
    plist = launch_agent.build_plist(["canon-webcam"])
    assert "/opt/homebrew/bin" in plist["EnvironmentVariables"]["PATH"]


def test_agent_logs_to_the_given_file(tmp_path):
    log = tmp_path / "hunter.log"
    plist = launch_agent.build_plist(["canon-webcam"], log_path=log)
    assert plist["StandardOutPath"] == plist["StandardErrorPath"] == str(log)


def test_agent_runs_always_on_and_waits_for_the_camera():
    assert launch_agent.agent_command()[-3:] == ["canon_webcam", "start", "--wait"]


def test_plist_serializes_cleanly():
    raw = plistlib.dumps(launch_agent.build_plist(["canon-webcam"]))
    assert plistlib.loads(raw)["Label"] == launch_agent.LABEL


def test_uninstall_removes_the_plist(tmp_path, monkeypatch):
    monkeypatch.setattr(launch_agent, "unload_and_wait", lambda: None)
    plist = tmp_path / "agent.plist"
    plist.write_text("Tarnished")
    launch_agent.uninstall(plist)
    assert not plist.exists()


def test_unload_waits_until_the_old_agent_is_gone(monkeypatch):
    still_loaded = iter([True, True, False])
    monkeypatch.setattr(launch_agent, "launchctl", lambda *args: None)
    monkeypatch.setattr(launch_agent, "is_loaded", lambda: next(still_loaded))
    monkeypatch.setattr(launch_agent.time, "sleep", lambda _: None)

    launch_agent.unload_and_wait()

    assert next(still_loaded, "drained") == "drained"
