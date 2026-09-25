#!/usr/bin/env python3
"""Builds the wasm source map fixtures.

Emscripten output is too large to review byte by byte, so this assembles a
minimal module instead. The module has one imported function and three function
bodies, which makes wasm function indices differ from code section indices. Two
bodies are named, the third is not, so the converter has to synthesize a
`wasm-function[N]` name for it.

The generated map mirrors what emscripten emits: a single destination line,
destination columns are byte offsets into the module, `names` is empty.

Run from this directory: python3 build.py
"""

import json
import os

B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"


def leb(value):
    """Unsigned LEB128."""
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def vlq(value):
    """Base64 VLQ, as used by source map mappings."""
    value = (-value << 1) | 1 if value < 0 else value << 1
    out = ""
    while True:
        digit = value & 0x1F
        value >>= 5
        out += B64[digit | 0x20 if value else digit]
        if not value:
            return out


def section(sid, payload):
    return bytes([sid]) + leb(len(payload)) + payload


def custom(name, payload):
    name = name.encode()
    return section(0, leb(len(name)) + name + payload)


def vec(items):
    return leb(len(items)) + b"".join(items)


# Function bodies. Each is `local count` followed by `nop`s and a closing `end`.
BODIES = [
    bytes([0x00]) + bytes([0x01]) * 3 + bytes([0x0B]),  # 5 bytes
    bytes([0x00]) + bytes([0x01]) * 7 + bytes([0x0B]),  # 9 bytes
    bytes([0x00]) + bytes([0x01]) * 1 + bytes([0x0B]),  # 3 bytes
]

# Wasm function indices. Index 0 is the import, so bodies start at 1. Index 2 is
# deliberately left out of the name section.
NAMES = {1: "add", 3: "main"}

BUILD_ID = bytes(range(16))


def build_wasm():
    module = bytearray(b"\x00asm\x01\x00\x00\x00")

    # type: one `() -> ()` signature
    module += section(1, vec([bytes([0x60, 0x00, 0x00])]))
    # import: one function, so wasm function indices are shifted by one
    module += section(2, vec([b"\x01e\x01f\x00\x00"]))
    # function: three bodies, all of type 0
    module += section(3, vec([bytes([0x00])] * len(BODIES)))

    code = vec([leb(len(body)) + body for body in BODIES])
    code_offset = len(module) + 1 + len(leb(len(code)))
    module += section(10, code)

    name_subsection = vec(
        [leb(index) + leb(len(name)) + name.encode() for index, name in sorted(NAMES.items())]
    )
    module += custom("name", section(1, name_subsection))
    module += custom("build_id", BUILD_ID)

    return bytes(module), code_offset


def body_addresses(code_offset):
    """Start offset of every function body, matching `WasmObject::symbols`."""
    cursor = code_offset + len(leb(len(BODIES)))
    for body in BODIES:
        cursor += len(leb(len(body)))
        yield cursor
        cursor += len(body)


def mappings(segments):
    """Encodes absolute segments as a single destination line."""
    out = []
    previous = [0, 0, 0, 0]
    for segment in segments:
        deltas = [segment[0] - previous[0]]
        if len(segment) > 1:
            deltas += [segment[i] - previous[i] for i in range(1, 4)]
            previous = list(segment)
        else:
            previous[0] = segment[0]
        out.append("".join(vlq(delta) for delta in deltas))
    return ",".join(out)


def main():
    wasm, code_offset = build_wasm()
    add, mid, tail = body_addresses(code_offset)

    # `add` is fully mapped. The unnamed body has a hole in the middle, closed by
    # a one field terminator. `main` has no mappings at all. The last segment
    # points past the end of the module and must be dropped.
    segments = [
        (add, 0, 9, 0),
        (add + 2, 0, 10, 4),
        (mid, 0, 19, 0),
        (mid + 4,),
        (mid + 6, 0, 20, 2),
        (len(wasm) + 1, 0, 30, 0),
    ]

    sourcemap = {
        "version": 3,
        "sources": ["src/main.c"],
        "names": [],
        "mappings": mappings(segments),
    }

    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, "simple.wasm"), "wb") as f:
        f.write(wasm)
    with open(os.path.join(here, "simple.wasm.map"), "w") as f:
        json.dump(sourcemap, f, indent=2)
        f.write("\n")

    # Emscripten 4.0.2x briefly emitted five field segments. `sourcemap` rejects
    # the whole map when the name index is out of bounds, so ship both shapes.
    five_field = dict(sourcemap, mappings=mappings([(add, 0, 9, 0)]) + vlq(0))
    with open(os.path.join(here, "five_field.wasm.map"), "w") as f:
        json.dump(five_field, f, indent=2)
        f.write("\n")
    with open(os.path.join(here, "five_field_named.wasm.map"), "w") as f:
        json.dump(dict(five_field, names=["add"]), f, indent=2)
        f.write("\n")

    print(f"code_offset {code_offset:#x} bodies {add:#x} {mid:#x} {tail:#x} size {len(wasm)}")


if __name__ == "__main__":
    main()
