"""Send the mandatory IPC client declaration on a raw socket.

Every daemon connection must declare who is speaking before an ordinary
request; the daemon withholds events until it does. This is the handshake the
Rust client library performs, for the proof scripts that frame IPC by hand.

Bump ``IPC_PROTOCOL_VERSION`` with ``mxr_protocol::IPC_PROTOCOL_VERSION``.
"""

import json
import struct

IPC_PROTOCOL_VERSION = 32


def declare(stream, source, owner=None, frame_id=2):
    """Declare ``source`` on ``stream`` and assert the daemon's ack."""
    frame = {
        "id": frame_id,
        "source": source,
        "payload": {
            "type": "Request",
            "cmd": "DeclareClient",
            "kind": source,
            "protocol_version": IPC_PROTOCOL_VERSION,
        },
    }
    if owner is not None:
        frame["expected_workspace"] = owner
    data = json.dumps(frame).encode()
    stream.sendall(struct.pack(">I", len(data)) + data)

    def exact(length):
        result = b""
        while len(result) < length:
            part = stream.recv(length - len(result))
            if not part:
                raise RuntimeError("daemon closed during DeclareClient")
            result += part
        return result

    ack = json.loads(exact(struct.unpack(">I", exact(4))[0]))["payload"]
    assert ack.get("status") == "Ok", ack
    return ack
