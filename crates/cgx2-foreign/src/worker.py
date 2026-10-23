"""CGx2 copied-scalar worker protocol v1; stdlib only, one request per process.

This file is embedded in the Rust crate. stdout's original descriptor is used
only for the response; user/module writes (including fd 1) go to /dev/null.
"""
import os
import sys

_protocol_fd = os.dup(1)
_null_fd = os.open(os.devnull, os.O_WRONLY)
os.dup2(_null_fd, 1)
os.close(_null_fd)

import importlib
import struct
import traceback

MAGIC = b"CGX2PY01"
MAX_VALUE_BYTES = 1_048_576
MAX_FRAME_BYTES = MAX_VALUE_BYTES + 131_072
MAX_ELEMENTS = 4096
MAX_FAILURE_BYTES = 16_384


def text(value):
    raw = value.encode("utf-8", "strict")
    if len(raw) > MAX_VALUE_BYTES:
        raise ValueError("string byte limit")
    return struct.pack(">I", len(raw)) + raw


def cap(value, limit):
    return value.encode("utf-8", "replace")[:limit].decode("utf-8", "ignore")


def failure(error):
    name = cap(type(error).__name__, 256)
    message = cap(str(error), 4096)
    trace = cap("".join(traceback.format_exception(type(error), error, error.__traceback__)),
                MAX_FAILURE_BYTES - len(name.encode()) - len(message.encode()))
    return text(name) + text(message) + text(trace)


class Decoder:
    def __init__(self, raw):
        self.raw = raw
        self.offset = 0
        self.string_bytes = 0

    def take(self, count):
        end = self.offset + count
        if end > len(self.raw):
            raise ValueError("truncated request")
        out = self.raw[self.offset:end]
        self.offset = end
        return out

    def string(self, limit):
        count = struct.unpack(">I", self.take(4))[0]
        if count > limit:
            raise ValueError("string limit")
        return self.take(count).decode("utf-8", "strict")

    def value(self):
        tag = self.take(1)
        if tag == b"i":
            return struct.unpack(">q", self.take(8))[0]
        if tag == b"b":
            flag = self.take(1)
            if flag not in (b"\0", b"\1"):
                raise ValueError("invalid bool")
            return flag == b"\1"
        if tag == b"s":
            value = self.string(MAX_VALUE_BYTES)
            self.string_bytes += len(value.encode())
            if self.string_bytes > MAX_VALUE_BYTES:
                raise ValueError("aggregate string limit")
            return value
        raise ValueError("invalid value tag")


def invoke(module_name, attribute, expected, args):
    try:
        module = importlib.import_module(module_name)
    except BaseException as error:
        return b"\1" + text(module_name) + failure(error)
    try:
        function = getattr(module, attribute)
    except BaseException as error:
        return b"\2" + text(attribute) + failure(error)
    try:
        result = function(*args)
    except BaseException as error:
        return b"\3" + failure(error)
    # Convert as the existing host does: bool before int, bounded i64, exact
    # copied scalar shape. No __str__, __bool__ or other implicit coercion here.
    actual = type(result).__name__
    tag = {bool: b"b", int: b"i", str: b"s", float: b"f"}.get(type(result))
    if tag != expected:
        actual = {b"b": "bool", b"i": "i64", b"s": "string", b"f": "f64"}.get(tag, actual)
        return b"\4" + text(actual)
    if tag == b"i":
        if not -(2**63) <= result < 2**63:
            return b"\4" + text("int outside i64 range")
        return b"\0i" + struct.pack(">q", result)
    if tag == b"b":
        return b"\0b" + bytes([result])
    try:
        return b"\0s" + text(result)
    except (ValueError, UnicodeError):
        return b"\4" + text("string exceeds byte limit or invalid UTF-8")


def main():
    raw = sys.stdin.buffer.read(MAX_FRAME_BYTES + 1)
    if len(raw) > MAX_FRAME_BYTES:
        raise ValueError("request frame limit")
    d = Decoder(raw)
    if d.take(8) != MAGIC or d.take(1) != b"Q":
        raise ValueError("worker identity/version")
    module = d.string(65_536)
    attribute = d.string(65_536)
    expected = d.take(1)
    count = struct.unpack(">I", d.take(4))[0]
    if count > MAX_ELEMENTS or expected not in (b"i", b"b", b"s"):
        raise ValueError("argument count or shape")
    args = [d.value() for _ in range(count)]
    if d.offset != len(raw):
        raise ValueError("trailing request")
    if not (attribute.isascii() and attribute.isidentifier() and not attribute.startswith("_")):
        raise ValueError("invalid attribute")
    if module != "math" and not (module == "builtins" and attribute in ("str", "bool")):
        raise ValueError("target outside source allowlist")
    if sys.implementation.name != "cpython" or sys.version_info.major != 3:
        raise ValueError("CPython 3 required")
    header = MAGIC + b"R" + text(sys.implementation.name) + text(sys.executable) + text(sys.version) + text(sys.prefix)
    response = header + invoke(module, attribute, expected, args)
    if len(response) > MAX_FRAME_BYTES:
        raise ValueError("response frame limit")
    while response:
        written = os.write(_protocol_fd, response)
        response = response[written:]
    os.close(_protocol_fd)


if __name__ == "__main__":
    main()
