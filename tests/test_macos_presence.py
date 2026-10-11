from canon_webcam.platforms.macos import presence
from canon_webcam.platforms.macos.presence import Assertion

PMSET_SAMPLE = """Listed by owning process:
   pid 444(coreaudiod): [0x000090d700019bc1] 01:43:02 PreventUserIdleSystemSleep named: "com.apple.audio.BuiltInMicrophoneDevice.context.preventuseridlesleep"
	Created for PID: 68942.
	Resources: audio-in BuiltInMicrophoneDevice
   pid 68942(OBS): [0x000090ea00059bd4] 01:42:43 NoDisplaySleepAssertion named: ""
   pid 97668(caffeinate): [0x0000a8f20001a2bd] 00:00:12 PreventUserIdleSystemSleep named: "caffeinate command-line tool"
"""


def test_parses_owner_kind_and_who_the_mic_is_held_for():
    assert presence.parse_assertions(PMSET_SAMPLE) == [
        Assertion("coreaudiod", "PreventUserIdleSystemSleep", 68942),
        Assertion("OBS", "NoDisplaySleepAssertion", None),
        Assertion("caffeinate", "PreventUserIdleSystemSleep", None),
    ]


def test_obs_holding_the_mic_and_screen_is_not_a_call():
    assert not presence.call_in_progress(presence.parse_assertions(PMSET_SAMPLE), obs_pids={68942})


def test_another_app_holding_the_mic_is_a_call():
    assertions = [Assertion("coreaudiod", "PreventUserIdleSystemSleep", 4242)]
    assert presence.call_in_progress(assertions, obs_pids={68942})


def test_an_app_keeping_the_screen_awake_is_a_call():
    assertions = [Assertion("zoom.us", "PreventUserIdleDisplaySleep", None)]
    assert presence.call_in_progress(assertions, obs_pids=set())


def test_idle_time_converts_nanoseconds_to_seconds():
    assert presence.parse_idle_seconds('  | "HIDIdleTime" = 92700000000\n') == 92.7


def test_idle_time_defaults_to_zero_when_missing():
    assert presence.parse_idle_seconds("Ashen One found nothing") == 0.0


def test_virtual_camera_counts_as_requested_only_when_running():
    assert presence.virtual_camera_in_use({"OBS Virtual Camera": True, "MacBook Camera": False})
    assert not presence.virtual_camera_in_use({"OBS Virtual Camera": False, "MacBook Camera": True})
