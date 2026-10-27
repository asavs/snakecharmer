#!/usr/bin/env python3
"""Reject release PEs that import the dynamic Microsoft C/C++ runtimes.

Reads ordinary and delay-loaded import tables without loading or executing the
binary. Uses only Python's standard library; Windows system DLLs are expected.
"""

import argparse
import pathlib
import re
import struct
import sys


DYNAMIC_CRT = re.compile(
    r"(?:vcruntime\w*|msvcp\w*|msvcr\w*|concrt\w*|ucrtbased?|"
    r"api-ms-win-crt-[\w-]+)\.dll", re.IGNORECASE
)


def imports(path):
    data = path.read_bytes()

    def unpack(fmt, offset):
        try:
            return struct.unpack_from(fmt, data, offset)
        except struct.error as error:
            raise ValueError("truncated PE structure") from error

    if data[:2] != b"MZ":
        raise ValueError("missing DOS signature")
    pe, = unpack("<I", 0x3C)
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("missing PE signature")
    section_count, = unpack("<H", pe + 6)
    optional_size, = unpack("<H", pe + 20)
    optional = pe + 24
    magic, = unpack("<H", optional)
    if magic == 0x20B:  # PE32+
        image_base, = unpack("<Q", optional + 24)
        directory_offset = 112
    elif magic == 0x10B:  # PE32
        image_base, = unpack("<I", optional + 28)
        directory_offset = 96
    else:
        raise ValueError("unsupported PE optional header")
    if optional_size < directory_offset:
        raise ValueError("truncated PE optional header")
    directory_count, = unpack("<I", optional + directory_offset - 4)
    sections = [unpack("<IIII", optional + optional_size + 40 * i + 8)
                for i in range(section_count)]

    def offset(rva):
        for _, virtual_address, raw_size, raw_pointer in sections:
            if virtual_address <= rva < virtual_address + raw_size:
                result = raw_pointer + rva - virtual_address
                if result < len(data):
                    return result
        raise ValueError(f"unmapped import RVA 0x{rva:x}")

    def dll_name(rva):
        start = offset(rva)
        end = data.find(b"\0", start, start + 256)
        if end <= start:
            raise ValueError("invalid import DLL name")
        try:
            return data[start:end].decode("ascii")
        except UnicodeDecodeError as error:
            raise ValueError("non-ASCII import DLL name") from error

    names = set()
    for index, fields in [(1, 5), (13, 8)]:
        if index >= directory_count:
            continue
        if directory_offset + 8 * (index + 1) > optional_size:
            raise ValueError("truncated PE data directories")
        rva, size = unpack("<II", optional + directory_offset + 8 * index)
        if not rva and not size:
            continue
        for relative in range(0, size - fields * 4 + 1, fields * 4):
            descriptor = unpack("<" + "I" * fields, offset(rva + relative))
            if not any(descriptor):
                break
            name = descriptor[3] if index == 1 else descriptor[1]
            if index == 13 and not descriptor[0] & 1:
                name -= image_base
            names.add(dll_name(name))
        else:
            raise ValueError("unterminated import directory")
    if not names:
        raise ValueError("no DLL imports found in this Windows executable")
    return sorted(names, key=str.lower)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs="+", type=pathlib.Path)
    args = parser.parse_args()
    failed = False
    for path in args.binaries:
        try:
            names = imports(path)
            dynamic = [name for name in names if DYNAMIC_CRT.fullmatch(name)]
            if dynamic:
                raise ValueError("dynamic CRT imports: " + ", ".join(dynamic))
            print(f"PASS {path}: {path.stat().st_size:,} bytes; imports " + ", ".join(names))
        except (OSError, ValueError) as error:
            print(f"FAIL {path}: {error}", file=sys.stderr)
            failed = True
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
