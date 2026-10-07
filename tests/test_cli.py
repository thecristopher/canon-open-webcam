from canon_webcam.cli import parse_args


def test_no_arguments_means_start():
    assert parse_args([]).command == "start"


def test_start_flags_work_without_the_subcommand():
    args = parse_args(["--no-obs"])
    assert args.command == "start" and args.no_obs


def test_doctor_is_its_own_command():
    assert parse_args(["doctor"]).command == "doctor"


def test_custom_port_is_parsed():
    assert parse_args(["start", "--port", "6000"]).port == 6000


def test_find_camera_names_the_process_hogging_the_camera(monkeypatch):
    import pytest
    from canon_webcam import cli

    monkeypatch.setattr(cli.actions, "other_camera_users", lambda: ["1998 gphoto2 --capture-movie"])
    with pytest.raises(RuntimeError, match="1998 gphoto2"):
        cli.find_camera()


def test_sigterm_is_treated_like_ctrl_c():
    import pytest
    from canon_webcam.cli import stop_on_sigterm

    with pytest.raises(KeyboardInterrupt):
        stop_on_sigterm(15, None)


def test_wait_flag_is_parsed():
    assert parse_args(["--wait"]).wait


def test_install_is_its_own_command():
    assert parse_args(["install"]).command == "install"


def test_closing_obs_ignores_an_already_dead_connection():
    from canon_webcam.cli import close_obs_connection

    class DeadObs:
        def request(self, *args):
            raise ConnectionError("Wesker unplugged OBS")

    close_obs_connection(DeadObs(), keep_virtualcam=False)
