from canon_webcam.actions import obs_setup
from canon_webcam.actions.obs_client import ObsError


class FakeObs:
    """Answers requests from a dict and remembers the order they came in."""

    def __init__(self, answers):
        self.answers = answers
        self.calls = []

    def request(self, request_type, data=None):
        self.calls.append((request_type, data))
        answer = self.answers.get(request_type, {})
        if isinstance(answer, Exception):
            raise answer
        return answer

    def names(self):
        return [name for name, _ in self.calls]


def test_creates_scene_when_missing():
    obs = FakeObs({"GetSceneList": {"scenes": [{"sceneName": "Roundtable Hold"}]}})
    obs_setup.ensure_scene(obs, "Canon Open Webcam")
    assert ("CreateScene", {"sceneName": "Canon Open Webcam"}) in obs.calls


def test_leaves_existing_scene_alone():
    obs = FakeObs({"GetSceneList": {"scenes": [{"sceneName": "Canon Open Webcam"}]}})
    obs_setup.ensure_scene(obs, "Canon Open Webcam")
    assert "CreateScene" not in obs.names()


def test_creates_media_input_when_missing():
    obs = FakeObs({"GetInputList": {"inputs": []}, "CreateInput": {"sceneItemId": 7}})
    assert obs_setup.ensure_media_input(obs, "scene", "Canon Live View", "tcp://x") == 7


def test_updates_existing_media_input_with_new_url():
    obs = FakeObs({
        "GetInputList": {"inputs": [{"inputName": "Canon Live View"}]},
        "GetSceneItemId": {"sceneItemId": 3},
    })
    obs_setup.ensure_media_input(obs, "scene", "Canon Live View", "tcp://new")
    settings = dict(obs.calls)["SetInputSettings"]["inputSettings"]
    assert settings["input"] == "tcp://new"


def test_readds_input_to_scene_if_it_was_removed():
    obs = FakeObs({
        "GetInputList": {"inputs": [{"inputName": "Canon Live View"}]},
        "GetSceneItemId": ObsError("gone"),
        "CreateSceneItem": {"sceneItemId": 9},
    })
    assert obs_setup.ensure_media_input(obs, "scene", "Canon Live View", "tcp://x") == 9


def test_media_input_expects_nut_wrapped_mjpeg():
    assert obs_setup.media_input_settings("tcp://x")["input_format"] == "nut"


def test_media_input_reads_a_network_stream_without_buffering():
    settings = obs_setup.media_input_settings("tcp://127.0.0.1:5600")
    assert settings["is_local_file"] is False
    assert settings["buffering_mb"] == 0


def test_transform_scales_to_fill_the_canvas():
    transform = obs_setup.fit_to_canvas_transform(1920, 1080)
    assert (transform["boundsWidth"], transform["boundsHeight"]) == (1920, 1080)


def test_starts_virtual_cam_only_when_it_is_off():
    obs = FakeObs({"GetVirtualCamStatus": {"outputActive": True}})
    obs_setup.start_virtual_cam(obs)
    assert "StartVirtualCam" not in obs.names()


def test_setup_ends_with_virtual_cam_running():
    obs = FakeObs({
        "GetSceneList": {"scenes": []},
        "GetInputList": {"inputs": []},
        "CreateInput": {"sceneItemId": 1},
        "GetVideoSettings": {"baseWidth": 1920, "baseHeight": 1080, "fpsNumerator": 60, "fpsDenominator": 1},
        "GetVirtualCamStatus": {"outputActive": False},
    })
    obs_setup.setup_obs(obs, "tcp://x")
    assert obs.names()[-1] == "StartVirtualCam"


def test_disables_every_global_mic():
    obs = FakeObs({"GetSpecialInputs": {"desktop1": None, "mic1": "Mic/Aux", "mic2": None}})
    assert obs_setup.disable_global_mics(obs) == ["Mic/Aux"]
    assert ("SetInputSettings", {"inputName": "Mic/Aux", "inputSettings": {"device_id": "disabled"}}) in obs.calls


def test_setup_can_leave_the_virtual_cam_off():
    obs = FakeObs({
        "GetSceneList": {"scenes": []},
        "GetInputList": {"inputs": []},
        "CreateInput": {"sceneItemId": 1},
        "GetVideoSettings": {"baseWidth": 1920, "baseHeight": 1080, "fpsNumerator": 60, "fpsDenominator": 1},
        "GetVirtualCamStatus": {"outputActive": False},
    })
    obs_setup.setup_obs(obs, "tcp://x", start_virtualcam=False)
    assert "StartVirtualCam" not in obs.names()


def test_bumps_canvas_to_60fps_for_lower_lag():
    obs = FakeObs({
        "GetVideoSettings": {"fpsNumerator": 30, "fpsDenominator": 1},
        "GetVirtualCamStatus": {"outputActive": True},
    })
    assert obs_setup.ensure_canvas_fps(obs)
    assert ("SetVideoSettings", {"fpsNumerator": 60, "fpsDenominator": 1}) in obs.calls


def test_stops_virtual_cam_before_changing_fps():
    obs = FakeObs({
        "GetVideoSettings": {"fpsNumerator": 30, "fpsDenominator": 1},
        "GetVirtualCamStatus": {"outputActive": True},
    })
    obs_setup.ensure_canvas_fps(obs)
    assert obs.names().index("StopVirtualCam") < obs.names().index("SetVideoSettings")


def test_leaves_fps_alone_when_already_60():
    obs = FakeObs({"GetVideoSettings": {"fpsNumerator": 60, "fpsDenominator": 1}})
    assert not obs_setup.ensure_canvas_fps(obs)
    assert "SetVideoSettings" not in obs.names()


def test_treats_60000_over_1000_as_60fps():
    obs = FakeObs({"GetVideoSettings": {"fpsNumerator": 60000, "fpsDenominator": 1000}})
    assert not obs_setup.ensure_canvas_fps(obs)


def test_shrinks_a_1080p_canvas_to_720p_so_frames_resize_once():
    obs = FakeObs({
        "GetVideoSettings": {"baseWidth": 1920, "baseHeight": 1080, "outputWidth": 1280, "outputHeight": 720},
        "GetVirtualCamStatus": {"outputActive": False},
    })
    assert obs_setup.ensure_canvas_size(obs)
    assert ("SetVideoSettings", {
        "baseWidth": 1280, "baseHeight": 720, "outputWidth": 1280, "outputHeight": 720,
    }) in obs.calls


def test_stops_virtual_cam_before_resizing_the_canvas():
    obs = FakeObs({
        "GetVideoSettings": {"baseWidth": 1920, "baseHeight": 1080, "outputWidth": 1920, "outputHeight": 1080},
        "GetVirtualCamStatus": {"outputActive": True},
    })
    obs_setup.ensure_canvas_size(obs)
    assert obs.names().index("StopVirtualCam") < obs.names().index("SetVideoSettings")


def test_leaves_a_720p_canvas_alone():
    obs = FakeObs({
        "GetVideoSettings": {"baseWidth": 1280, "baseHeight": 720, "outputWidth": 1280, "outputHeight": 720},
    })
    assert not obs_setup.ensure_canvas_size(obs)
    assert "SetVideoSettings" not in obs.names()
