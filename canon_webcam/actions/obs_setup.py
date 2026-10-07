from .obs_client import ObsError

SCENE_NAME = "Canon Open Webcam"
INPUT_NAME = "Canon Live View"

# OBS alignment flags: LEFT=1, TOP=4, so 5 pins the item to the top left corner
ALIGN_TOP_LEFT = 5
ALIGN_CENTER = 0
# Every hop from OBS render to the virtual camera holds whole frames, so a 60fps canvas
# measured ~80ms less lag than 30fps even though the camera only sends ~22fps
CANVAS_FPS = 60
# Meeting apps send 720p at most, so a matching canvas means OBS resizes each frame once instead of twice
CANVAS_WIDTH = 1280
CANVAS_HEIGHT = 720


def media_input_settings(url: str) -> dict:
    return {
        "is_local_file": False,
        "input": url,
        "input_format": "nut",
        "buffering_mb": 0,
        "reconnect_delay_sec": 1,
        # Software JPEG decode is cheap at live view size and is the path we latency tested
        "hw_decode": False,
        "clear_on_media_end": False,
        "close_when_inactive": False,
        "restart_on_activate": False,
        "ffmpeg_options": "fflags=nobuffer flags=low_delay",
    }


def fit_to_canvas_transform(width: int, height: int) -> dict:
    return {
        "positionX": 0,
        "positionY": 0,
        "alignment": ALIGN_TOP_LEFT,
        "boundsType": "OBS_BOUNDS_SCALE_INNER",
        "boundsAlignment": ALIGN_CENTER,
        "boundsWidth": width,
        "boundsHeight": height,
    }


def ensure_scene(obs, scene: str) -> None:
    scenes = [s["sceneName"] for s in obs.request("GetSceneList")["scenes"]]
    if scene not in scenes:
        obs.request("CreateScene", {"sceneName": scene})


def ensure_media_input(obs, scene: str, input_name: str, url: str) -> int:
    settings = media_input_settings(url)
    inputs = [i["inputName"] for i in obs.request("GetInputList")["inputs"]]

    if input_name not in inputs:
        created = obs.request("CreateInput", {
            "sceneName": scene,
            "inputName": input_name,
            "inputKind": "ffmpeg_source",
            "inputSettings": settings,
            "sceneItemEnabled": True,
        })
        return created["sceneItemId"]

    obs.request("SetInputSettings", {"inputName": input_name, "inputSettings": settings, "overlay": True})
    try:
        found = obs.request("GetSceneItemId", {"sceneName": scene, "sourceName": input_name})
        return found["sceneItemId"]
    except ObsError:
        # The input exists but someone removed it from our scene
        added = obs.request("CreateSceneItem", {"sceneName": scene, "sourceName": input_name})
        return added["sceneItemId"]


def fit_to_canvas(obs, scene: str, scene_item_id: int) -> None:
    video = obs.request("GetVideoSettings")
    obs.request("SetSceneItemTransform", {
        "sceneName": scene,
        "sceneItemId": scene_item_id,
        "sceneItemTransform": fit_to_canvas_transform(video["baseWidth"], video["baseHeight"]),
    })


def start_virtual_cam(obs) -> None:
    if not obs.request("GetVirtualCamStatus")["outputActive"]:
        obs.request("StartVirtualCam")


def stop_virtual_cam(obs) -> None:
    if obs.request("GetVirtualCamStatus")["outputActive"]:
        obs.request("StopVirtualCam")


def disable_global_mics(obs) -> list[str]:
    # An open mic in OBS keeps the Mac from idle sleeping, and the virtual camera has no audio anyway
    disabled = []
    for channel, input_name in obs.request("GetSpecialInputs").items():
        if channel.startswith("mic") and input_name:
            obs.request("SetInputSettings", {"inputName": input_name, "inputSettings": {"device_id": "disabled"}})
            disabled.append(input_name)
    return disabled


def ensure_canvas_fps(obs, fps: int = CANVAS_FPS) -> bool:
    video = obs.request("GetVideoSettings")
    if video["fpsNumerator"] / video["fpsDenominator"] == fps:
        return False
    # OBS refuses video setting changes while an output is running
    stop_virtual_cam(obs)
    obs.request("SetVideoSettings", {"fpsNumerator": fps, "fpsDenominator": 1})
    return True


def ensure_canvas_size(obs, width: int = CANVAS_WIDTH, height: int = CANVAS_HEIGHT) -> bool:
    video = obs.request("GetVideoSettings")
    wanted = {"baseWidth": width, "baseHeight": height, "outputWidth": width, "outputHeight": height}
    if all(video.get(key) == value for key, value in wanted.items()):
        return False
    # OBS refuses video setting changes while an output is running
    stop_virtual_cam(obs)
    obs.request("SetVideoSettings", wanted)
    return True


def setup_obs(obs, url: str, start_virtualcam: bool = True) -> None:
    ensure_canvas_fps(obs)
    ensure_canvas_size(obs)
    ensure_scene(obs, SCENE_NAME)
    scene_item_id = ensure_media_input(obs, SCENE_NAME, INPUT_NAME, url)
    fit_to_canvas(obs, SCENE_NAME, scene_item_id)
    obs.request("SetCurrentProgramScene", {"sceneName": SCENE_NAME})
    if start_virtualcam:
        start_virtual_cam(obs)
