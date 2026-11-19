"""Loopback TLS peer for the release throughput test; V16 supplies the tunnel reply."""

import socket
import ssl

CHUNK = 4 * 1024 * 1024
REPLY = bytes.fromhex("05815300000a") + b'{"code":0}' + bytes.fromhex("05000101c00002501f90")


def exchange(connection, framed):
    connection.settimeout(30)
    with connection.makefile("rb") as source:
        def read(size):
            data = source.read(size)
            if len(data) != size:
                raise EOFError("incomplete bulk transfer")
            return data

        # Negotiation carries a length-prefixed document, then CONNECT carries a destination.
        opening = read(7)
        read(int.from_bytes(opening[-2:], "big"))
        endpoint = read(4)[-1]
        if endpoint == 1:
            read(4)
        elif endpoint == 3:
            read(read(1)[0])
        else:
            raise ValueError("unexpected destination type")
        read(2)
        connection.sendall(REPLY)
        pending = bytearray()

        def take(size):
            if not framed:
                return read(size)
            while len(pending) < size:
                header = read(4)
                assert header[:2] == b"\x01\x00"
                pending.extend(read(int.from_bytes(header[2:], "big")))
            data = pending[:size]
            del pending[:size]
            return data

        def send(data):
            for start in range(0, len(data), 16384):
                part = data[start:start + 16384]
                header = b"\x01\x00" + len(part).to_bytes(2, "big") if framed else b""
                connection.sendall(header + part)

        expected = bytearray((bytes(range(251)) * (CHUNK // 251 + 1))[:CHUNK])
        counts = {b"U": 0, b"D": 0}
        while True:
            command = bytes(take(1))
            if command == b"E":
                return
            expected[:4] = counts[command].to_bytes(4, "big")
            counts[command] += 1
            if command == b"U":
                assert take(CHUNK) == expected, "upload differs"
                send(b"K")
            else:
                send(expected)


context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("cert.pem", "key.pem")
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    print(f"127.0.0.1:{listener.getsockname()[1]}", flush=True)
    for framed in [False, True] * 3:
        with context.wrap_socket(listener.accept()[0], server_side=True) as connection:
            exchange(connection, framed)
