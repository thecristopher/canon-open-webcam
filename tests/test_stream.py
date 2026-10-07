import io
import os
import threading
from collections import deque

import pytest

from canon_webcam.actions import stream


def test_sender_connects_to_localhost_over_tcp():
    assert stream.sender_url(5600) == "tcp://127.0.0.1:5600"


def test_receiver_listens_so_reconnects_get_fresh_headers():
    assert stream.receiver_url(5600) == "tcp://127.0.0.1:5600?listen=1"


def test_gphoto2_streams_live_view_to_stdout_forever():
    assert stream.gphoto2_command("usb:020,005") == [
        "gphoto2", "--port", "usb:020,005", "--stdout", "--capture-movie",
    ]


def test_ffmpeg_reads_mjpeg_from_stdin():
    command = stream.ffmpeg_command("tcp://127.0.0.1:5600")
    assert command[command.index("-i") - 2 : command.index("-i") + 2] == ["-f", "mjpeg", "-i", "-"]


def test_ffmpeg_passes_frames_through_without_reencoding():
    command = stream.ffmpeg_command("tcp://127.0.0.1:5600")
    assert command[command.index("-c:v") + 1] == "copy"


def test_ffmpeg_wraps_frames_in_nut_for_timestamps():
    assert stream.ffmpeg_command("tcp://127.0.0.1:5600")[-3:] == ["-f", "nut", "tcp://127.0.0.1:5600"]


class FinishedProcess:
    def __init__(self):
        self.terminated = False

    def poll(self):
        return 1

    def terminate(self):
        self.terminated = True

    def wait(self, timeout=None):
        return 1


def test_pipeline_retries_when_the_camera_drops(monkeypatch):
    stop = threading.Event()
    attempts = []

    def fake_start(camera_port, url):
        attempts.append(camera_port)
        if len(attempts) == 2:
            stop.set()
        return FinishedProcess(), FinishedProcess(), deque(["Ada unplugged the cable"]), stream.Heartbeat()

    monkeypatch.setattr(stream, "free_camera_from_macos", lambda: None)
    monkeypatch.setattr(stream, "start_processes", fake_start)
    monkeypatch.setattr(stream, "RETRY_DELAY_SECONDS", 0)
    logs = []

    stream.run_pipeline(lambda: "usb:1", "tcp://x", stop, log=logs.append)

    assert len(attempts) == 2
    assert any("Ada unplugged the cable" in line for line in logs)


def test_pipeline_waits_quietly_while_the_camera_is_missing(monkeypatch):
    stop = threading.Event()
    lookups = []

    def find_port():
        lookups.append(1)
        if len(lookups) == 3:
            stop.set()
        return None

    monkeypatch.setattr(stream, "CAMERA_POLL_SECONDS", 0)
    logs = []

    stream.run_pipeline(find_port, "tcp://x", stop, log=logs.append)

    assert logs == ["Waiting for the camera. Plug it in and turn it on."]


def test_pipeline_stops_when_told_not_to_keep_going():
    lookups = []
    stream.run_pipeline(lambda: lookups.append(1), "tcp://x", threading.Event(), keep_going=lambda: False)
    assert lookups == []


def test_stop_processes_terminates_anything_still_running():
    running = RunningProcess()
    stream.stop_processes(running)
    assert running.terminated


class RunningProcess(FinishedProcess):
    def poll(self):
        return None


class KeepOpenBytesIO(io.BytesIO):
    def close(self):
        pass


def test_heartbeat_measures_time_since_the_last_frame():
    now = [100.0]
    heartbeat = stream.Heartbeat(clock=lambda: now[0])
    now[0] = 107.0
    assert heartbeat.silent_for() == 7.0


def test_heartbeat_resets_when_a_frame_arrives():
    now = [100.0]
    heartbeat = stream.Heartbeat(clock=lambda: now[0])
    now[0] = 107.0
    heartbeat.beat()
    assert heartbeat.silent_for() == 0.0


def test_relay_copies_frames_to_ffmpeg_and_beats():
    read_end, write_end = os.pipe()
    os.write(write_end, b"Leon's jpeg")
    os.close(write_end)
    source = os.fdopen(read_end, "rb")
    sink = KeepOpenBytesIO()
    beats = []
    heartbeat = stream.Heartbeat()
    heartbeat.beat = lambda: beats.append(1)

    stream.relay(source, sink, heartbeat)

    assert sink.getvalue() == b"Leon's jpeg"
    assert beats


def test_relay_survives_ffmpeg_dying():
    read_end, write_end = os.pipe()
    os.write(write_end, b"Ashley's frame")
    os.close(write_end)

    class DeadFfmpeg(io.BytesIO):
        def write(self, data):
            raise BrokenPipeError

    stream.relay(os.fdopen(read_end, "rb"), DeadFfmpeg(), stream.Heartbeat())


def test_pipeline_restarts_when_gphoto2_hangs_without_exiting(monkeypatch):
    stop = threading.Event()
    attempts = []
    gphotos = []

    def fake_start(camera_port, url):
        attempts.append(camera_port)
        if len(attempts) == 2:
            stop.set()
        gphoto = RunningProcess()
        gphotos.append(gphoto)
        return gphoto, RunningProcess(), deque(), stream.Heartbeat()

    monkeypatch.setattr(stream, "free_camera_from_macos", lambda: None)
    monkeypatch.setattr(stream, "start_processes", fake_start)
    monkeypatch.setattr(stream, "RETRY_DELAY_SECONDS", 0)
    monkeypatch.setattr(stream, "STALL_SECONDS", -1)
    logs = []

    stream.run_pipeline(lambda: "usb:1", "tcp://x", stop, log=logs.append)

    assert gphotos[0].terminated
    assert any("no frames for" in line for line in logs)
