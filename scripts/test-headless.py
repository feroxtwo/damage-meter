#!/usr/bin/env python3
"""Smoke-test the actual binary, local HTTP API and CLI without an overlay/game."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

binary = str(Path(os.environ.get("METER_BINARY", "target/release/aion2-meter")).resolve())
with tempfile.TemporaryDirectory(prefix="aion2-meter-smoke-") as tmp:
    database = str(Path(tmp) / "meter.db")
    # A second instance must fail cleanly before starting its overlay/capture.
    with socket.socket() as occupied:
        occupied.bind(("127.0.0.1", 0))
        occupied.listen()
        port = occupied.getsockname()[1]
        conflict = subprocess.run(
            [binary, "--no-overlay", "--db", database, "--port", str(port)],
            capture_output=True, text=True, timeout=10,
        )
        assert conflict.returncode != 0
        assert "Dashboard-Adresse" in conflict.stderr
    proc = subprocess.Popen(
        [binary, "--no-overlay", "--db", database, "--port", str(port)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    base = f"http://127.0.0.1:{port}"
    try:
        deadline = time.monotonic() + 10
        while True:
            try:
                with urllib.request.urlopen(base + "/api/live", timeout=1) as response:
                    assert response.headers["Cache-Control"] == "no-store"
                    assert "rows" in json.load(response)
                break
            except (urllib.error.URLError, TimeoutError):
                if proc.poll() is not None or time.monotonic() >= deadline:
                    raise AssertionError("Meter did not start")
                time.sleep(.05)
        with urllib.request.urlopen(base, timeout=2) as response:
            assert "Gruppen-DPS" in response.read().decode()
        request = urllib.request.Request(base + "/api/reset", method="POST")
        try:
            urllib.request.urlopen(request, timeout=2)
            raise AssertionError("Mutation without action header was accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 403
        request.add_header("x-a2m", "1")
        with urllib.request.urlopen(request, timeout=2) as response:
            assert response.status == 204
        request = urllib.request.Request(base + "/api/live", headers={"Host": f"evil.example:{port}"})
        try:
            urllib.request.urlopen(request, timeout=2)
            raise AssertionError("Foreign Host was accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 403
        status = subprocess.run([binary, "--port", str(port), "ctl", "status"], capture_output=True, text=True, timeout=10)
        assert status.returncode == 0, status.stderr
        assert "Overlay sichtbar:" in status.stdout
        def get(path):
            with urllib.request.urlopen(base + path, timeout=2) as response:
                return json.load(response)
        def post(path, body):
            request = urllib.request.Request(base + path, json.dumps(body).encode(), headers={"x-a2m":"1", "Content-Type":"application/json"}, method="POST")
            with urllib.request.urlopen(request, timeout=2) as response:
                return response.status
        for asset, mime in [("/enhancements.js", "text/javascript"), ("/enhancements.css", "text/css")]:
            with urllib.request.urlopen(base + asset, timeout=2) as response:
                assert mime in response.headers["Content-Type"]
                assert response.read()
        settings = get("/api/overlay")
        settings.update({"position":[123,456], "hide_names":True, "metric":"heal", "max_rows":2})
        post("/api/overlay", settings)
        post("/api/overlay/profile", {"key":"Main", "save":True})
        post("/api/training", {"seconds":180})
        assert get("/api/fights")["fights"] == []
        proc.terminate()
        proc.wait(timeout=5)
        proc = subprocess.Popen([binary, "--no-overlay", "--db", database, "--port", str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + 10
        while True:
            try:
                restored = get("/api/overlay")
                break
            except (urllib.error.URLError, TimeoutError):
                if time.monotonic() >= deadline:
                    raise AssertionError("Restart failed")
                time.sleep(.05)
        assert restored["position"] == [123,456]
        assert restored["hide_names"] and restored["metric"] == "heal"
        post("/api/overlay/profile", {"key":"Main", "save":False})
        # Legacy captures remain readable without permissions or a running game.
        capture = Path(tmp) / "legacy.a2mcap"
        capture.write_bytes(b"A2MCAP1\n")
        output = Path(tmp) / "replay.json"
        replay = subprocess.run([binary,"replay",str(capture),"--output",str(output)],capture_output=True,text=True,timeout=10)
        assert replay.returncode == 0, replay.stderr
        assert json.loads(output.read_text())["capture"]["legacy_metadata_limited"]
        capture.write_bytes(b"A2MCAP2\n\x04\x00")
        assert subprocess.run([binary,"replay",str(capture)],capture_output=True,timeout=10).returncode != 0
        print("PASS binary startup, API guards, assets, persistent settings, profiles, training and replay CLI")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
