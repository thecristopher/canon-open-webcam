from pathlib import Path

from canon_webcam.platforms import windows
from canon_webcam.platforms.windows import startup


def test_tasklist_rows_give_obs_pids():
    output = '"obs64.exe","4242","Console","1","250,000 K"\n"obs64.exe","4343","Console","1","1,000 K"\n'
    assert windows.parse_tasklist(output) == {4242, 4343}


def test_tasklist_info_line_means_obs_is_closed():
    assert windows.parse_tasklist("INFO: No tasks are running which match the specified criteria.\n") == set()


def test_obs_starts_from_its_own_folder_and_detached():
    exe = Path(r"C:\Program Files\obs-studio\bin\64bit\obs64.exe")
    command = windows.launch_obs_command(exe)
    assert command[:4] == ["cmd", "/c", "start", ""]
    assert command[command.index("/d") + 1] == str(exe.parent)
    assert "--minimize-to-tray" in command


def test_msys2_gphoto2_gets_added_to_path(tmp_path, monkeypatch):
    monkeypatch.setattr(windows, "MSYS2_BIN", tmp_path)
    monkeypatch.setenv("PATH", "Raccoon City")
    windows.add_tool_paths()
    windows.add_tool_paths()
    assert windows.os.environ["PATH"].split(windows.os.pathsep) == ["Raccoon City", str(tmp_path)]


def test_gphoto2_is_pointed_at_its_newest_installed_drivers(tmp_path, monkeypatch):
    bin_dir = tmp_path / "ucrt64" / "bin"
    bin_dir.mkdir(parents=True)
    for version in ("2.5.9", "2.5.34"):
        (tmp_path / "ucrt64" / "lib" / "libgphoto2" / version).mkdir(parents=True)
    (tmp_path / "ucrt64" / "lib" / "libgphoto2_port" / "0.12.2").mkdir(parents=True)
    monkeypatch.setattr(windows, "MSYS2_BIN", bin_dir)
    monkeypatch.delenv("CAMLIBS", raising=False)
    monkeypatch.delenv("IOLIBS", raising=False)

    windows.add_tool_paths()

    assert windows.os.environ["CAMLIBS"].endswith("2.5.34")
    assert windows.os.environ["IOLIBS"].endswith("0.12.2")


def test_agent_runs_always_on_and_waits_for_the_camera():
    assert startup.agent_command()[-3:] == ["canon_webcam", "start", "--wait"]


def test_supervisor_restarts_the_agent_and_throttles(tmp_path, monkeypatch):
    monkeypatch.setattr(startup, "DATA_DIR", tmp_path)
    monkeypatch.setattr(startup, "LOG_PATH", tmp_path / "ada.log")
    monkeypatch.setattr(startup, "PID_PATH", tmp_path / "ada.pid")
    runs, naps = [], []
    monkeypatch.setattr(startup.subprocess, "run", lambda command, **kwargs: runs.append(command))

    startup.supervise(sleep=naps.append, keep_going=lambda: len(runs) < 3)

    assert len(runs) == 3
    assert all(0 < nap <= startup.THROTTLE_SECONDS for nap in naps)
    assert (tmp_path / "ada.pid").read_text().isdigit()


def test_stop_ignores_a_pid_that_is_not_ours(tmp_path, monkeypatch):
    pid_path = tmp_path / "stale.pid"
    pid_path.write_text("1998")
    monkeypatch.setattr(startup, "PID_PATH", pid_path)
    monkeypatch.setattr(startup, "is_our_supervisor", lambda pid: False)
    killed = []
    monkeypatch.setattr(startup.subprocess, "run", lambda command, **kwargs: killed.append(command))

    startup.stop()

    assert killed == []
    assert not pid_path.exists()
