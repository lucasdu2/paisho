"""A minimal client for paisho's socket protocol: newline-delimited JSON, one response per
request (see `src/server.rs`)."""

import json
import socket
import sys
from typing import Any


class PaishoError(Exception):
    """The server answered a request with an error."""


def socket_address(name: str) -> str:
    """The address the paisho server listens on for the socket `name` (see `socket_name` in
    `src/server.rs`): an abstract socket on Linux, a file in `/tmp/` on other Unixes."""
    if sys.platform.startswith("linux"):
        return "\0" + name
    if sys.platform == "win32":
        raise NotImplementedError("connecting to paisho's named pipe on Windows isn't supported")
    return "/tmp/" + name


class PaishoClient:
    """One connection to the server. A connection runs one episode at a time, so use one
    client per parallel worker."""

    def __init__(self, socket_name: str = "paisho.sock") -> None:
        self._socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._socket.connect(socket_address(socket_name))
        self._file = self._socket.makefile("rw", encoding="utf-8")

    def request(self, message: dict[str, Any]) -> dict[str, Any]:
        """Sends `message` and returns the server's response, raising `PaishoError` if the
        response is an error."""
        response = self.request_raw(message)
        if response.get("type") == "error":
            raise PaishoError(response["message"])
        return response

    def request_raw(self, message: dict[str, Any]) -> dict[str, Any]:
        """Sends `message` and returns the server's response, including errors."""
        self._file.write(json.dumps(message) + "\n")
        self._file.flush()
        line = self._file.readline()
        if not line:
            raise ConnectionError("paisho closed the connection")
        return json.loads(line)

    def close(self) -> None:
        self._file.close()
        self._socket.close()

    def __enter__(self) -> "PaishoClient":
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()
