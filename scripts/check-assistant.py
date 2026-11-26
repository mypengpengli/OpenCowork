"""Smoke test the AS observer and model adapters with synthetic data and loopback endpoints."""

import argparse
import base64
import ctypes as win
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
from urllib.error import HTTPError
from urllib.request import Request, build_opener, ProxyHandler
import uuid


class Models(http.server.BaseHTTPRequestHandler):
    decisions = []
    generated = []

    def log_message(self, *args):
        pass

    def do_POST(self):
        payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path in ("/v1beta/decisions", "/v1/responses"):
            assert self.headers.get("Authorization") == "Bearer as-test-key"
            assert set(payload) == {"model", "input", "questions"}
            assert payload["model"] == "gemini-decision-1"
            criteria = payload["questions"]["next_action"]["criteria"]
            self.decisions.append(criteria)
            choice = "verify_existing" if "verify_existing" in criteria else "wait"
            response = {
                "answers": {"next_action": {"type": "choice", "choice": choice, "confidence": 0.94}},
                "usage": {"total_input_tokens": 82},
            }
        elif self.path == "/v1/chat/completions":
            content = payload["messages"][-1]["content"]
            if isinstance(content, list):
                url = next(part["image_url"]["url"] for part in content if part["type"] == "image_url")
                assert url.startswith("data:image/png;base64,")
                assert base64.b64decode(url.split(",", 1)[1]).startswith(b"\x89PNG")
                content = next(part["text"] for part in content if part["type"] == "text")
            inputs = json.loads(content)
            self.generated.append(inputs)
            if "criteria" in inputs:
                if payload["model"] == "other-vendor-decision":
                    assert self.headers.get("Authorization") == "Bearer decision-test-key"
                prediction = {"choice": "wait", "confidence": 0.91}
            else:
                prediction = {
                    "task": "项目权限核对", "evidence": "可见页面显示已有权限",
                    "missing": ["工号"], "choices": {"verify_existing": "查看已有权限", "wait": "等工号"},
                    "choice": "wait",
                }
            response = {
                "choices": [{"index": 0, "message": {"role": "assistant", "content": json.dumps(prediction, ensure_ascii=False)}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 101, "completion_tokens": 37},
            }
        else:
            self.send_error(404)
            return
        body = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def wait_for(callback, limit=20):
    until = time.monotonic() + limit
    while time.monotonic() < until:
        result = callback()
        if result:
            return result
        time.sleep(0.15)
    raise AssertionError("Condition did not become true")


def local_url(port):
    return f"http://127.0.0.1:{port}"


def run(shell, window):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Models)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="oc-as-") as temp:
        root = Path(temp)
        local = root / "config"
        settings = root / ".opencowork"
        settings.mkdir()
        local.mkdir()
        fake_url = local_url(server.server_port)
        (settings / "settings.json").write_text(json.dumps({
            "model": "as-fixture",
            "provider": {
                "name": "Fixture", "baseUrl": fake_url + "/v1",
                "apiKeyEnv": "AS_TEST_KEY", "streaming": False,
            },
        }), encoding="utf-8")
        store = local / "as-desktop"
        store.mkdir()
        (store / "decision.json").write_text(json.dumps({
            "endpoint": fake_url + "/v1beta/decisions", "model": "gemini-decision-1",
            "apiKeyEnv": "", "auth": "bearer", "useForAnalysis": True,
        }), encoding="utf-8")
        old_id, new_id = str(uuid.uuid4()), str(uuid.uuid4())
        now = int(time.time() * 1000)
        expired_id = str(uuid.uuid4())
        (store / "observations.json").write_text(json.dumps([
            {"id": expired_id, "timeMs": now - 31 * 86400000, "window": "Expired", "text": "past"},
            {"id": old_id, "timeMs": now - 3500, "window": "申请 A", "text": "二期 报表权限"},
            {"id": new_id, "timeMs": now - 1600, "window": "申请 B", "text": "二期 报表权限 工号待补"},
        ], ensure_ascii=False), encoding="utf-8")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = {key: val for key, val in os.environ.items() if not key.startswith("OPENCOWORK_")}
        env.update(OPENCOWORK_CONFIG_HOME=str(local), OPENCOWORK_SHELL_PORT=str(port),
                   AS_TEST_KEY="as-test-key", NO_PROXY="127.0.0.1,localhost")
        if window:
            # The test runner is the parent desktop session for this isolated shell.
            env["OPENCOWORK_DESKTOP_PID"] = str(os.getpid())
        opener = build_opener(ProxyHandler({}))
        with (root / "shell.log").open("w", encoding="utf-8") as log:
            proc = subprocess.Popen([shell], cwd=root, env=env, stdout=log,
                                    stderr=log, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            def api(path, data=None, method=None, code=200, headers=None):
                request = Request(local_url(port) + path, headers={
                    **(headers or {}), **({"Content-Type": "application/json"} if data is not None else {}),
                }, data=json.dumps(data).encode() if data is not None else None, method=method)
                try:
                    response = opener.open(request, timeout=22)
                except HTTPError as error:
                    response = error
                with response:
                    body = response.read()
                    assert response.status == code, (path, response.status, body)
                    return json.loads(body) if response.headers.get("Content-Type", "").startswith("application/json") else body

            def health():
                try:
                    api("/api/health", code=204)
                    return True
                except OSError:
                    return False

            try:
                wait_for(health)
                overview = api("/api/assistant")
                assert not overview["enabled"]
                assert overview["available"] == window
                assert overview["decision"]["decisionProvider"]
                assert "endpoint" not in overview["decision"]
                migrated = json.loads((store / "decision.json").read_text(encoding="utf-8"))
                assert "endpoint" not in migrated
                native = next(p for p in api("/api/provider-profiles") if p["id"] == migrated["decisionProvider"])
                assert native["apiFormat"] == "decisions" and not native["active"]
                api("/api/provider-profiles/" + native["id"] + "/activate", {}, code=400)
                assert all(item["id"] != expired_id for item in overview["observations"])
                api("/api/assistant", headers={"Sec-Fetch-Site": "cross-site"}, code=403)
                api("/api/assistant", headers={"Host": "not-local.test"}, code=403)
                api("/api/assistant/observation", {"intervalMs": 200}, code=400)
                api("/api/assistant/observations/" + old_id + "/outcome", {
                    "summary": "二期 报表权限", "lesson": "重复申请先查已有权限",
                })
                decision = {
                    "endpoint": fake_url + "/v1beta/decisions",
                    "model": "gemini-decision-1", "apiKeyEnv": "", "auth": "bearer",
                    "useForAnalysis": True, "analyzeAutomatically": False,
                }
                api("/api/assistant/decision", decision)
                result = api("/api/assistant/decision/test", method="POST")
                assert result["choice"] == "wait" and result["inputTokens"] == 82
                api("/api/assistant/decision", {**decision, "endpoint": fake_url + "/v1/responses"})
                result = api("/api/assistant/decision/test", method="POST")
                assert result["choice"] == "wait" and result["inputTokens"] == 82
                prediction = api("/api/assistant/observations/" + new_id + "/analyze", method="POST")
                assert prediction["selector"] == "decisionModel" and prediction["choice"] == "verify_existing", prediction
                assert Models.generated[-1]["user_confirmed_outcomes"][0]["lesson"] == "重复申请先查已有权限"
                assert "observe" in Models.decisions[-1] and "verify_existing" in Models.decisions[-1]
                api("/api/assistant/decision", {**decision, "endpoint": fake_url + "/missing"})
                assert "native Decisions" in api("/api/assistant/decision/test", method="POST", code=400)["error"]
                prediction = api("/api/assistant/observations/" + new_id + "/analyze", method="POST")
                assert prediction["selector"] == "chatFallback" and prediction["choice"] == "wait"
                providers = api("/api/provider-profiles", {
                    "label": "Other vendor decision", "name": "Other vendor",
                    "model": "other-vendor-decision", "apiKey": "decision-test-key",
                    "baseUrl": fake_url + "/v1", "apiFormat": "chatCompletions", "activate": False,
                })
                chosen = next(p for p in providers if p["model"] == "other-vendor-decision")
                assert chosen["apiKey"] == "" and chosen["hasApiKey"] and not chosen["active"]
                selection = {"decisionProvider": chosen["id"], "useForAnalysis": True}
                api("/api/assistant/decision", selection)
                result = api("/api/assistant/decision/test", method="POST")
                assert result["choice"] == "wait" and result["confidence"] == 0.91
                api("/api/provider-profiles/" + chosen["id"], method="DELETE")
                api("/api/assistant/decision/test", method="POST", code=400)
                prediction = api("/api/assistant/observations/" + new_id + "/analyze", method="POST")
                assert prediction["selector"] == "chatFallback"
                api("/api/assistant/decision", {"useForAnalysis": False})
                before = len(Models.generated)
                prediction = api("/api/assistant/observations/" + new_id + "/analyze", method="POST")
                assert prediction["selector"] == "chat" and len(Models.generated) == before + 1
                api("/api/assistant/observations/" + new_id, method="DELETE", code=204)
                assert not api("/api/assistant")["predictions"]
                print("PASS: legacy migration, saved decision Providers, other-vendor chat decisions, optional main-model decisions, fallback, deletion and local API access")
                if window:
                    api("/api/assistant/decision", {**decision, "useImages": True, "analyzeAutomatically": True})
                    capture_desktop(api)
                else:
                    api("/api/assistant/observation", {"enabled": True}, code=400)
                return True
            finally:
                proc.terminate()
                proc.wait(timeout=10)
                server.shutdown()


def show_fixture():
    import tkinter as tk

    fixture = tk.Tk()
    fixture.title("OpenCowork AS Observer Fixture")
    fixture.state("zoomed")
    fixture.attributes("-topmost", True)
    message = tk.Label(fixture, text="青竹项目授权申请\nBlue Bamboo Request 24681357",
                       font=("Microsoft YaHei UI", 44), bg="white", fg="black", padx=30, pady=30)
    message.pack(expand=True, fill="both")
    fixture.update()
    fixture.lift()
    fixture.focus_force()
    fixture.mainloop()


def capture_desktop(api):
    fixture = subprocess.Popen([sys.executable, __file__, "--fixture"],
                               creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        user32 = win.windll.user32
        user32.FindWindowW.restype = win.c_void_p
        user32.GetForegroundWindow.restype = win.c_void_p
        hwnd = wait_for(lambda: user32.FindWindowW(None, "OpenCowork AS Observer Fixture"))
        user32.SetForegroundWindow(win.c_void_p(hwnd))
        api("/api/assistant/observation", {"enabled": True, "intervalMs": 500})

        def watched():
            overview = api("/api/assistant")
            assert not overview["error"], overview["error"]
            for item in overview["observations"]:
                normalized = "".join(item["text"].split())
                if "24681357" in normalized and "项目授权" in normalized:
                    return item

        try:
            hit = wait_for(watched)
        except AssertionError as error:
            view = api("/api/assistant")
            seen = any(item["window"].endswith("Observer Fixture") for item in view["observations"])
            raise AssertionError(
                f"OCR fixture timed out, phase={view['phase']}, "
                f"language={view['language']}, fixture_seen={seen}, foreground={user32.GetForegroundWindow() == hwnd}, "
                f"observation_count={len(view['observations'])}, fixture_exit={fixture.poll()}"
            ) from error
        assert api("/api/assistant/preview").startswith(b"\x89PNG")
        wait_for(lambda: next((item for item in api("/api/assistant")["predictions"]
                              if item["observationId"] == hit["id"]), None))
        assert api("/api/assistant")["autoRemaining"] < 100
        prediction = api("/api/assistant/observations/" + hit["id"] + "/analyze?image=true", method="POST")
        assert prediction["usedImage"], prediction
        api("/api/assistant/observation", {"enabled": False})
        api("/api/assistant/preview", code=204)
        assert not api("/api/assistant")["enabled"]
        print("PASS: Chinese OCR, half-second interval, background prediction, cropped PNG request and immediate pause")
    finally:
        fixture.terminate()
        fixture.wait(timeout=10)


if __name__ == "__main__":
    if sys.argv[1:] == ["--fixture"]:
        show_fixture()
        raise SystemExit
    parser = argparse.ArgumentParser()
    parser.add_argument("shell", type=Path)
    parser.add_argument("--desktop", action="store_true", help="Show a synthetic OCR fixture in the Windows desktop session.")
    args = parser.parse_args()
    run(args.shell.resolve(), args.desktop)
