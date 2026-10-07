from canon_webcam.actions import on_demand


class FakeSignals:
    def __init__(self, requested=False, idle=0.0, call=False):
        self.requested, self.idle, self.call = requested, idle, call

    def camera_requested(self):
        return self.requested

    def idle_seconds(self):
        return self.idle

    def call_in_progress(self):
        return self.call


def test_starts_when_an_app_opens_the_camera():
    assert on_demand.should_start(FakeSignals(requested=True), stopped_for_idle=False)


def test_stays_off_when_no_app_wants_the_camera():
    assert not on_demand.should_start(FakeSignals(requested=False), stopped_for_idle=False)


def test_after_idle_pause_waits_for_the_user_to_come_back():
    signals = FakeSignals(requested=True, idle=600)
    assert not on_demand.should_start(signals, stopped_for_idle=True)


def test_after_idle_pause_resumes_once_the_user_is_back():
    signals = FakeSignals(requested=True, idle=2)
    assert on_demand.should_start(signals, stopped_for_idle=True)


def test_stops_when_idle_and_no_call():
    assert on_demand.should_stop(FakeSignals(idle=301), idle_limit_seconds=300)


def test_keeps_running_during_a_call_even_when_idle():
    assert not on_demand.should_stop(FakeSignals(idle=3600, call=True), idle_limit_seconds=300)


def test_keeps_running_while_the_user_is_active():
    assert not on_demand.should_stop(FakeSignals(idle=5), idle_limit_seconds=300)


class FakeSession:
    events = []

    def __init__(self, *args):
        pass

    def start(self):
        FakeSession.events.append("start")

    def stop(self):
        FakeSession.events.append("stop")


def test_loop_goes_live_then_pauses_when_leon_walks_away(monkeypatch):
    monkeypatch.setattr(on_demand, "LiveSession", FakeSession)
    FakeSession.events = []
    signals = FakeSignals(requested=True)
    ticks = iter([True, True, False])

    def walk_away(_seconds):
        signals.idle = 999

    on_demand.run_on_demand(
        obs=None, find_camera_port=None, url="tcp://x",
        keep_going=lambda: next(ticks), idle_limit_seconds=300,
        signals=signals, log=lambda _: None, sleep=walk_away,
    )

    assert FakeSession.events == ["start", "stop"]
