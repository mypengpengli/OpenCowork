"""Exercise startup without real credentials, MCP processes, or external services.

Usage: python scripts/check-startup.py PATH_TO_OPENCOWORK_SHELL
"""

import concurrent.futures
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request


def check_startup(executable):
    sync_started = threading.Event()
    release_sync = threading.Event()

    class SyncHandler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            sync_started.set()
            release_sync.wait(10)
            self.send_response(404)
            self.end_headers()

        def log_message(self, *args):
            pass

    sync_server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), SyncHandler)
    threading.Thread(target=sync_server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="opencowork-startup-") as temporary:
            root = Path(temporary)
            project = root / "project"
            config = root / "config"
            (project / ".opencowork").mkdir(parents=True)
            (config / "sessions").mkdir(parents=True)
            marker = root / "mcp-started"
            settings = {
                "provider": {"apiKeyEnv": "STARTUP_TEST_UNUSED_KEY"},
                "mcpServers": {"slow": {
                    "type": "stdio", "command": sys.executable,
                    "args": ["-c", f"from pathlib import Path; Path({str(marker)!r}).touch()"],
                    "timeoutMs": 5000,
                }},
                "teamMemorySync": {
                    "enabled": True,
                    "endpoint": f"http://127.0.0.1:{sync_server.server_port}",
                    "repo": "startup/test", "tokenEnv": "STARTUP_TEST_UNUSED_KEY",
                    "pollMs": 60000, "timeoutMs": 10000,
                },
            }
            (project / ".opencowork" / "settings.json").write_text(json.dumps(settings))
            for index in range(30):
                session = {"version": 1, "messages": [{
                    "role": "user", "blocks": [{"type": "text", "text": f"Hello {index}"}],
                    "usage": None,
                }]}
                (config / "sessions" / f"session-{index}.json").write_text(json.dumps(session))
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            environment = {key: value for key, value in os.environ.items()
                           if not key.startswith("OPENCOWORK_")}
            environment.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port))
            base = f"http://127.0.0.1:{port}"
            # Ignore machine proxy settings for these loopback-only checks.
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

            def request(path):
                with opener.open(base + path, timeout=5) as response:
                    return response.status, response.read()

            with (root / "server.log").open("w+") as log:
                start = time.perf_counter()
                process = subprocess.Popen(
                    [str(executable)], cwd=project, env=environment, stdout=log, stderr=log,
                    creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                )
                try:
                    deadline = time.monotonic() + 15
                    while True:
                        if process.poll() is not None:
                            log.seek(0)
                            raise AssertionError(log.read())
                        try:
                            assert request("/api/health")[0] == 204
                            break
                        except OSError:
                            if time.monotonic() >= deadline:
                                raise
                            time.sleep(0.025)
                    print(f"Process to health: {(time.perf_counter() - start) * 1000:.0f} ms")
                    assert request("/")[0] == 200
                    start = time.perf_counter()
                    bootstrap = json.loads(request("/api/bootstrap")[1])
                    print(f"Bootstrap with 30 sessions: {(time.perf_counter() - start) * 1000:.0f} ms")
                    assert len(bootstrap["sessions"]) == 30
                    assert all(item["messageCount"] == 1 for item in bootstrap["sessions"])
                    assert {item["title"] for item in bootstrap["sessions"]} == {
                        f"Hello {index}" for index in range(30)
                    }
                    assert not marker.exists(), "Bootstrap launched MCP discovery"
                    assert not sync_started.is_set(), "Bootstrap waited on team memory"
                    with concurrent.futures.ThreadPoolExecutor() as pool:
                        pending = pool.submit(request, "/api/team-memory-sync")
                        try:
                            assert sync_started.wait(5), "Team sync never started"
                            assert not pending.done(), "Sync should still be waiting"
                            assert request("/api/health")[0] == 204
                            assert request("/app.js")[0] == 200
                            assert len(json.loads(request("/api/bootstrap")[1])["sessions"]) == 30
                            assert not marker.exists(), "Team sync launched MCP discovery"
                        finally:
                            release_sync.set()
                        status = json.loads(pending.result(timeout=5)[1])
                        assert status["running"] and not status["last_error"], status
                    print("PASS: health, page, history, deferred MCP, and nonblocking team sync")
                finally:
                    release_sync.set()
                    process.terminate()
                    process.wait(timeout=5)
    finally:
        release_sync.set()
        sync_server.shutdown()
        sync_server.server_close()


if __name__ == "__main__":
    check_startup(Path(sys.argv[1]).resolve(strict=True))
