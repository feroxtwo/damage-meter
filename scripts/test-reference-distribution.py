#!/usr/bin/env python3
"""Guard source/package builds against bundled community reference archives.

Inspect gzip members inside the actual executable, including members without
standalone filenames. Fixtures are generated locally and contain no provider data.
"""
import gzip
import json
from pathlib import Path
import subprocess
import sys
import zlib

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = {"a2m-provider-observations-v1", "a2m-community-v1", "a2m-community-v2"}
MAX_BYTES = 32 * 1024 * 1024


def embedded_references(binary):
    found = []
    offset = 0
    while (offset := binary.find(b"\x1f\x8b\x08", offset)) >= 0:
        try:
            decoder = zlib.decompressobj(16 + zlib.MAX_WBITS)
            payload = decoder.decompress(binary[offset:], MAX_BYTES + 1)
            if decoder.eof and len(payload) <= MAX_BYTES:
                value = json.loads(payload)
                if isinstance(value, dict) and value.get("schema") in SCHEMAS:
                    found.append(offset)
        except (zlib.error, ValueError, UnicodeError):
            pass  # An unrelated asset or accidental magic bytes are not an archive.
        offset += 1
    return found


def main():
    for schema in SCHEMAS:
        fixture = gzip.compress(json.dumps({"schema": schema, "rows": []}).encode(), mtime=0)
        assert embedded_references(b"ELF-test" + fixture + b"tail") == [8]
    assert embedded_references(b"ELF-test" + gzip.compress(b"ordinary asset")) == []
    assert embedded_references(b"ELF-test\x1f\x8b\x08broken") == []

    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    for name in filter(None, tracked):
        path = ROOT / name
        if not path.is_file():
            continue  # Supports a staged deletion before commit.
        assert not (name.startswith("data/community/") and path.suffix != ".md"), name
        if path.suffix in {".gz", ".bin", ".sqlite", ".db", ".json"}:
            content = path.read_bytes()
            assert not embedded_references(content), f"Bundled reference archive: {name}"
            if path.suffix == ".json":
                value = json.loads(content)
                assert not (isinstance(value, dict) and value.get("schema") in SCHEMAS), name
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/aion2-meter")
    assert not embedded_references(binary.read_bytes()), f"Embedded reference archive in {binary}"
    print("PASS distribution: synthetic detector checks, source inventory and binary contain no reference archives")


if __name__ == "__main__":
    main()
