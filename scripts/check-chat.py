"""Integration check with a loopback-only model and disposable workspace."""
import ctypes
import http.client
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


def check_chat(executable):
    with tempfile.TemporaryDirectory(prefix="opencowork-chat-") as temporary:
        root = Path(temporary)
        project, config = root / "project", root / "config"
        (project / ".opencowork").mkdir(parents=True)
        config.mkdir()
        release = threading.Event()
        child_marker = root / "child-pid"

        class Model(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                messages = payload["messages"]
                prompt = next(m["content"] for m in reversed(messages) if m["role"] == "user").split("\n", 1)[0]
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()

                def event(delta):
                    self.wfile.write(("data: " + json.dumps({"choices": [{"delta": delta}]}) + "\n\n").encode())
                    self.wfile.flush()

                try:
                    if messages[-1]["role"] == "tool":
                        event({"content": "工具已结束"})
                    elif prompt == "tool":
                        code = f"import os,time; from pathlib import Path; Path({str(child_marker)!r}).write_text(str(os.getpid())); time.sleep(20)"
                        if os.name == "nt":
                            command = "& '" + sys.executable.replace("'", "''") + "' -c '" + code.replace("'", "''") + "'"
                        else:
                            import shlex
                            command = shlex.join([sys.executable, "-c", code])
                        event({"tool_calls": [{"index": 0, "id": "call-stop", "type": "function", "function": {
                            "name": "bash", "arguments": json.dumps({"command": command, "timeoutMs": 30000})}}]})
                    elif prompt == "timeout":
                        command = "Start-Sleep -Seconds 20" if os.name == "nt" else "sleep 20"
                        event({"tool_calls": [{"index": 0, "id": "call-timeout", "type": "function", "function": {
                            "name": "bash", "arguments": json.dumps({"command": command, "timeoutMs": 800})}}]})
                    else:
                        event({"content": "第一段🌏"})
                        if prompt in ("stream", "cancel"):
                            release.wait(15)
                        if prompt == "failure":
                            self.wfile.write(b"data: invalid-json\n\n")
                        else:
                            event({"content": "第二段"})
                    self.wfile.write(b"data: [DONE]\n\n")
                    self.wfile.flush()
                except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                    pass

        model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
        threading.Thread(target=model.serve_forever, daemon=True).start()
        settings = {
            "model": "test-model", "permissionMode": "danger-full-access",
            "provider": {"name": "mock", "apiKeyEnv": "OPENCOWORK_TEST_API_KEY",
                         "baseUrl": f"http://127.0.0.1:{model.server_port}/v1", "timeoutMs": 30000},
            "mcpServers": {}, "teamMemorySync": {"enabled": False},
        }
        (project / ".opencowork" / "settings.json").write_text(json.dumps(settings))
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        environment = {key: value for key, value in os.environ.items() if not key.startswith("OPENCOWORK_")}
        environment.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port), OPENCOWORK_TEST_API_KEY="test-only")
        # Do not route loopback model traffic through machine proxies.
        environment.update(NO_PROXY="127.0.0.1,localhost", no_proxy="127.0.0.1,localhost")

        def request(path, payload=None, method=None):
            connection = http.client.HTTPConnection("127.0.0.1", port, timeout=8)
            connection.request(method or ("POST" if payload is not None else "GET"), path,
                               json.dumps(payload) if payload is not None else None,
                               {"Content-Type": "application/json"})
            response = connection.getresponse()
            return connection, response

        def read_message(response):
            line = response.readline()
            assert line, "Stream ended without completion"
            return json.loads(line)

        def begin(prompt, turn_id, references=None):
            connection, response = request("/api/chat", {"input": prompt, "turnId": turn_id, "references": references or []})
            assert response.status == 200, response.read()
            started = read_message(response)
            assert started["type"] == "started", started
            return connection, response, started["session_id"]

        def until(response, predicate):
            messages = []
            while True:
                message = read_message(response)
                messages.append(message)
                if predicate(message):
                    return message, messages
                assert message["type"] not in ("complete", "error"), message

        def stop(turn):
            connection, response = request(f"/api/chat/{turn}/cancel", {}, "POST")
            assert response.status == 200 and json.loads(response.read())["requested"]
            connection.close()

        with (root / "server.log").open("w+") as log:
            process = subprocess.Popen([str(executable)], cwd=project, env=environment,
                                       stdout=log, stderr=log, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            try:
                deadline = time.monotonic() + 15
                while True:
                    if process.poll() is not None:
                        log.seek(0)
                        raise AssertionError(log.read())
                    try:
                        connection, response = request("/api/health")
                        assert response.status == 204
                        connection.close()
                        break
                    except OSError:
                        if time.monotonic() > deadline:
                            raise
                        time.sleep(0.05)
                connection, response, session = begin("stream", "stream-test")
                first, _ = until(response, lambda m: m["type"] == "event" and m["event"]["type"] == "assistant_text_delta")
                assert first["event"]["text"] == "第一段🌏" and not release.is_set()
                duplicate, conflict = request("/api/chat", {"input": "other", "sessionId": session})
                assert conflict.status == 409
                duplicate.close()
                deletion, conflict = request(f"/api/sessions/{session}", method="DELETE")
                assert conflict.status == 409
                deletion.close()
                release.set()
                completed, _ = until(response, lambda m: m["type"] == "complete")
                assert completed["response"]["status"] == "completed"
                connection.close()
                saved = json.loads((config / "sessions" / f"{session}.json").read_text(encoding="utf-8"))
                assert saved["messages"][-1]["blocks"][0]["text"] == "第一段🌏第二段"
                print("PASS: real incremental output, saved final session, concurrency protection")

                release.clear()
                (project / "reference.txt").write_text("Cancellation reference fixture", encoding="utf-8")
                connection, listing = request("/api/workspace")
                assert "reference.txt" in json.loads(listing.read())["files"]
                connection.close()
                connection, response, session = begin("cancel", "cancel-test", [{"kind": "file", "path": "reference.txt"}])
                until(response, lambda m: m["type"] == "event")
                start = time.monotonic()
                stop("cancel-test")
                completed, _ = until(response, lambda m: m["type"] == "complete")
                assert completed["response"]["status"] == "cancelled"
                assert "第一段" in json.dumps(completed["response"]["session"], ensure_ascii=False)
                saved = json.loads((config / "sessions" / f"{session}.json").read_text(encoding="utf-8"))
                assert "Cancellation reference fixture" in json.dumps(saved)
                assert time.monotonic() - start < 4
                connection.close()
                release.set()
                print("PASS: non-Git workspace listing; stop retains partial output and attached reference context")

                connection, response, session = begin("failure", "failure-test")
                completed, _ = until(response, lambda m: m["type"] == "complete")
                assert completed["response"]["status"] == "failed"
                assert "第一段" in json.dumps(completed["response"]["session"], ensure_ascii=False)
                connection.close()
                print("PASS: provider errors retain partial output")

                connection, response, session = begin("tool", "tool-test")
                until(response, lambda m: m["type"] == "event" and m["event"]["type"] == "tool_call")
                deadline = time.monotonic() + 8
                while not child_marker.exists():
                    assert time.monotonic() < deadline, "Tool child did not start"
                    time.sleep(0.05)
                child_pid = int(child_marker.read_text())
                stop("tool-test")
                completed, _ = until(response, lambda m: m["type"] == "complete")
                assert completed["response"]["status"] == "cancelled"
                assert any(block.get("tool_use_id") == "call-stop" and block["is_error"]
                           for message in completed["response"]["session"]["messages"] for block in message["blocks"])
                if os.name == "nt":
                    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
                    kernel.OpenProcess.restype = ctypes.c_void_p
                    kernel.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
                    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
                    handle = kernel.OpenProcess(0x100000, False, child_pid)
                    if handle:
                        try:
                            assert kernel.WaitForSingleObject(handle, 2000) == 0, "Tool child survived cancellation"
                        finally:
                            kernel.CloseHandle(handle)
                connection.close()
                print("PASS: stop terminates owned tool descendants and pairs interrupted tool results")

                connection, response, session = begin("timeout", "timeout-test")
                completed, events = until(response, lambda m: m["type"] == "complete")
                assert completed["response"]["status"] == "completed", completed
                outputs = [m["event"]["output"] for m in events if m["type"] == "event" and m["event"]["type"] == "tool_result"]
                assert any(json.loads(output)["timed_out"] for output in outputs)
                connection.close()
                print("PASS: tool timeout returns control to the model")
            finally:
                release.set()
                process.terminate()
                process.wait(timeout=5)
                model.shutdown()
                model.server_close()


if __name__ == "__main__":
    check_chat(Path(sys.argv[1]).resolve(strict=True))
