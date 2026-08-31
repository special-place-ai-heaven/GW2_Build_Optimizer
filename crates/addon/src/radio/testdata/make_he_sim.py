"""Splice SBR-style FIL elements into a genuine AAC-LC ADTS stream.

Real HE-AAC v1/v2 over ADTS is implicitly signalled: the ADTS header says
"LC" and the SBR (and PS) payload rides in FIL elements (id_syn_ele == 6)
with extension_type EXT_SBR_DATA (0xD). symphonia-codec-aac's element loop
skips FIL payloads content-blind (ignore_bits(count * 8)), so a fixture
whose FIL payloads are SBR-typed filler exercises the exact code path a
real HE-AAC stream hits -- without needing an HE-AAC encoder.

Per frame we prepend, before the SCE, eight FIL elements:
  FIL#1: count nibble 15 + esc byte 17 -> 31 payload bytes (esc branch)
  FIL#2..8: count 2 -> 2 payload bytes each   (short branch)
Bit total: 8*7 (id+count) + 8 (esc) + (31+14)*8 = 424 bits = 53 bytes,
so the original frame bytes stay byte-aligned and are copied verbatim.
The 13-bit ADTS frame_length is bumped by 53. Payload bytes avoid 0xFF so
no fake syncword can appear; the first byte's top nibble is 0xD
(EXT_SBR_DATA) to stay faithful to real SBR fill payloads.
"""

import sys

SRC, DST = sys.argv[1], sys.argv[2]
PREFIX_LEN = 53


def bits(value, width):
    return [(value >> (width - 1 - i)) & 1 for i in range(width)]


def build_prefix():
    out = []
    # FIL #1: esc-count branch. count=15, esc=17 -> 15 + 17 - 1 = 31 bytes.
    out += bits(0b110, 3) + bits(15, 4) + bits(17, 8)
    payload = [0xD5] + [0x2A, 0x55] * 15  # 31 bytes, first nibble 0xD
    for b in payload:
        out += bits(b, 8)
    # FIL #2..#8: short-count branch, 2 payload bytes each.
    for _ in range(7):
        out += bits(0b110, 3) + bits(2, 4)
        for b in (0xD5, 0x2A):
            out += bits(b, 8)
    assert len(out) == PREFIX_LEN * 8, len(out)
    raw = bytearray()
    for i in range(0, len(out), 8):
        byte = 0
        for bit in out[i : i + 8]:
            byte = (byte << 1) | bit
        raw.append(byte)
    assert 0xFF not in raw
    return bytes(raw)


PREFIX = build_prefix()

data = open(SRC, "rb").read()
frames = []
pos = 0
while pos < len(data):
    assert data[pos] == 0xFF and (data[pos + 1] & 0xF6) == 0xF0, hex(pos)
    assert data[pos + 1] & 0x01, "fixture must be protection_absent (no CRC)"
    frame_len = ((data[pos + 3] & 0x03) << 11) | (data[pos + 4] << 3) | (data[pos + 5] >> 5)
    frames.append(data[pos : pos + frame_len])
    pos += frame_len

out = bytearray()
for frame in frames:
    header = bytearray(frame[:7])
    new_len = len(frame) + PREFIX_LEN
    assert new_len < (1 << 13)
    header[3] = (header[3] & 0xFC) | ((new_len >> 11) & 0x03)
    header[4] = (new_len >> 3) & 0xFF
    header[5] = (header[5] & 0x1F) | ((new_len & 0x07) << 5)
    out += header + PREFIX + frame[7:]

open(DST, "wb").write(bytes(out))
print(f"{len(frames)} frames, {len(data)} -> {len(out)} bytes")
