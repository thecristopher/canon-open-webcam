from canon_webcam.actions.camera import Camera, parse_auto_detect, pick_camera

SPANISH_OUTPUT = """Modelo                         Puerto
----------------------------------------------------------
Canon EOS 2000D                usb:020,005
"""


def test_parses_camera_from_localized_output():
    assert parse_auto_detect(SPANISH_OUTPUT) == [Camera("Canon EOS 2000D", "usb:020,005")]


def test_parses_nothing_when_no_camera_is_plugged_in():
    empty = "Model                          Port\n----------------------------------------------------------\n"
    assert parse_auto_detect(empty) == []


def test_parses_nothing_from_garbage_output():
    assert parse_auto_detect("*** Error: Leon lost the camera ***") == []


def test_prefers_the_canon_over_other_cameras():
    nikon = Camera("Nikon of Raccoon City", "usb:001,002")
    canon = Camera("Canon EOS Rebel T7", "usb:001,003")
    assert pick_camera([nikon, canon]) == canon


def test_falls_back_to_first_camera_when_no_canon():
    melina = Camera("Melina's Pinhole", "usb:001,009")
    assert pick_camera([melina]) == melina


def test_picks_nothing_from_an_empty_list():
    assert pick_camera([]) is None


def test_other_camera_users_only_counts_live_view_streams(monkeypatch):
    from canon_webcam.actions import camera

    class Result:
        stdout = "4242 gphoto2 --capture-movie --stdout\n4343 gphoto2 --auto-detect\n"

    monkeypatch.setattr(camera.subprocess, "run", lambda *a, **k: Result())
    assert camera.other_camera_users() == ["4242 gphoto2 --capture-movie --stdout"]
