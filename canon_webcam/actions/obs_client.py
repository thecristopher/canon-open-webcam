import base64
import hashlib
import json
import time
import uuid
from typing import Optional

from websockets.exceptions import ConnectionClosed, InvalidHandshake
from websockets.sync.client import connect

OP_HELLO, OP_IDENTIFY, OP_IDENTIFIED, OP_REQUEST, OP_RESPONSE = 0, 1, 2, 6, 7


class ObsError(RuntimeError):
    pass


def sha256_base64(text: str) -> str:
    return base64.b64encode(hashlib.sha256(text.encode()).digest()).decode()


def auth_response(password: str, salt: str, challenge: str) -> str:
    return sha256_base64(sha256_base64(password + salt) + challenge)


class ObsClient:
    def __init__(self, socket):
        self.socket = socket

    def send(self, op: int, data: dict) -> None:
        self.socket.send(json.dumps({"op": op, "d": data}))

    def receive(self) -> dict:
        return json.loads(self.socket.recv())

    def identify(self, password: Optional[str]) -> None:
        hello = self.receive()
        # Opting out of events keeps request replies the only thing on the wire
        identify = {"rpcVersion": 1, "eventSubscriptions": 0}

        auth = hello["d"].get("authentication")
        if auth:
            if not password:
                raise ObsError("OBS wants a websocket password. Pass --obs-password.")
            identify["authentication"] = auth_response(password, auth["salt"], auth["challenge"])

        self.send(OP_IDENTIFY, identify)
        try:
            reply = self.receive()
        except ConnectionClosed:
            raise ObsError("OBS rejected the websocket password.") from None
        if reply["op"] != OP_IDENTIFIED:
            raise ObsError(f"OBS did not accept the handshake: {reply}")

    def request(self, request_type: str, data: Optional[dict] = None) -> dict:
        request_id = str(uuid.uuid4())
        self.send(OP_REQUEST, {
            "requestType": request_type,
            "requestId": request_id,
            "requestData": data or {},
        })

        while True:
            message = self.receive()
            if message["op"] == OP_RESPONSE and message["d"]["requestId"] == request_id:
                break

        status = message["d"]["requestStatus"]
        if not status["result"]:
            detail = status.get("comment") or f"code {status['code']}"
            raise ObsError(f"{request_type} failed: {detail}")
        return message["d"].get("responseData") or {}

    def close(self) -> None:
        self.socket.close()


def connect_to_obs(url: str, password: Optional[str], timeout: float = 30) -> ObsClient:
    deadline = time.monotonic() + timeout
    while True:
        try:
            client = ObsClient(connect(url, open_timeout=3))
        except (OSError, InvalidHandshake, TimeoutError):
            # OBS takes a few seconds to open its websocket after launch
            if time.monotonic() > deadline:
                raise ObsError(f"Could not reach OBS at {url}. Is the websocket server on?")
            time.sleep(1)
            continue
        client.identify(password)
        return client
