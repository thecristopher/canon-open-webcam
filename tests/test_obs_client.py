import base64
import hashlib
import json

import pytest
from websockets.exceptions import ConnectionClosedError

from canon_webcam.actions.obs_client import ObsClient, ObsError, auth_response


class FakeSocket:
    """Plays back scripted OBS messages and records what the client sends."""

    def __init__(self, replies):
        self.replies = list(replies)
        self.sent = []

    def send(self, text):
        self.sent.append(json.loads(text))

    def recv(self):
        reply = self.replies.pop(0)
        if isinstance(reply, Exception):
            raise reply
        if callable(reply):
            reply = reply(self.sent[-1])
        return json.dumps(reply)


def response_to(result=True, data=None, comment=None):
    def build(request):
        return {"op": 7, "d": {
            "requestId": request["d"]["requestId"],
            "requestStatus": {"result": result, "code": 100 if result else 600, "comment": comment},
            "responseData": data,
        }}
    return build


def test_auth_response_follows_the_obs_websocket_spec():
    secret = base64.b64encode(hashlib.sha256(b"malenia" + b"salt").digest()).decode()
    expected = base64.b64encode(hashlib.sha256((secret + "challenge").encode()).digest()).decode()
    assert auth_response("malenia", "salt", "challenge") == expected


def test_identify_sends_auth_when_obs_asks_for_it():
    socket = FakeSocket([
        {"op": 0, "d": {"authentication": {"salt": "s", "challenge": "c"}}},
        {"op": 2, "d": {}},
    ])
    ObsClient(socket).identify("malenia")
    assert socket.sent[0]["d"]["authentication"] == auth_response("malenia", "s", "c")


def test_identify_without_password_fails_when_obs_needs_one():
    socket = FakeSocket([{"op": 0, "d": {"authentication": {"salt": "s", "challenge": "c"}}}])
    with pytest.raises(ObsError, match="password"):
        ObsClient(socket).identify(None)


def test_identify_reports_a_wrong_password():
    socket = FakeSocket([
        {"op": 0, "d": {"authentication": {"salt": "s", "challenge": "c"}}},
        ConnectionClosedError(None, None),
    ])
    with pytest.raises(ObsError, match="rejected"):
        ObsClient(socket).identify("wesker")


def test_request_returns_response_data():
    socket = FakeSocket([response_to(data={"outputActive": True})])
    assert ObsClient(socket).request("GetVirtualCamStatus") == {"outputActive": True}


def test_request_skips_unrelated_messages():
    socket = FakeSocket([{"op": 5, "d": {"eventType": "Nemesis"}}, response_to(data={"ok": 1})])
    assert ObsClient(socket).request("GetVersion") == {"ok": 1}


def test_request_raises_when_obs_says_no():
    socket = FakeSocket([response_to(result=False, comment="No scene named Umbrella")])
    with pytest.raises(ObsError, match="Umbrella"):
        ObsClient(socket).request("SetCurrentProgramScene")
