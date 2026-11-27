"""Replay retained observations through the actual background loop and HTTP provider."""

from contextlib import closing
import importlib.util
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def run(executable):
    spec = importlib.util.spec_from_file_location("assistant_checks", Path(__file__).with_name("check-assistant.py"))
    check = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(check)
    received = []
    attempted = threading.Event()

    class Model(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            content = request["messages"][-1]["content"]
            packet = json.loads(content)
            fragments = packet["new_observations"]
            received.append(packet)
            ids = [frame["id"] for frame in fragments]
            if len(received) == 1:
                # A syntactically valid response omits the earliest evidence.
                output = {"covered": ids[1:], "caseChanges": [], "suggestion": None}
            else:
                facts = []
                for frame in fragments:
                    if frame["app"] != "fixture.exe":
                        continue
                    for quote, field, value in [
                        ("申请人老王", "requester", "老王"),
                        ("给小陈办", "subject", "小陈"),
                        ("项目 A17", "project", "A17"),
                        ("已选资料员", "selectedRole", "资料员"),
                    ]:
                        if quote in frame["text"]:
                            facts.append({"field": field, "value": value, "kind": "observed",
                                "fragmentId": frame["id"], "source": "text", "quote": quote})
                related = sorted({fact["fragmentId"] for fact in facts})
                output = {"covered": ids, "caseChanges": [], "suggestion": None}
                if facts:
                    output["caseChanges"].append({"caseId": None, "version": None,
                        "title": "回放申请 A17", "summary": "依据来自本次实际收到的文字",
                        "fragments": related, "facts": facts, "missing": ["回执待核实"]})
            payload = json.dumps({"choices": [{"message": {"role": "assistant", "content": json.dumps(output)},
                "finish_reason": "stop"}], "usage": {"prompt_tokens": 120, "completion_tokens": 83}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            attempted.set()

    with tempfile.TemporaryDirectory(prefix="oc-as-replay-") as temporary, ThreadingHTTPServer(("127.0.0.1", 0), Model) as server:
        root = Path(temporary)
        config = root / "config"
        store = config / "as-desktop"
        store.mkdir(parents=True)
        (root / ".opencowork").mkdir()
        (root / ".opencowork/settings.json").write_text(json.dumps({
            "model": "fixture", "provider": {"baseUrl": check.local_url(server.server_port) + "/v1",
                "name": "AS fixture", "apiKeyEnv": "AS_FIXTURE_KEY", "streaming": False},
        }), encoding="utf-8")
        now = int(time.time() * 1000) - 24 * 3600 * 1000
        records = [{"id": str(uuid.uuid4()), "timeMs": now + index * 500,
            "app": "fixture.exe", "window": window, "windowText": body, "text": body}
            for index, (window, body) in enumerate([
                ("三局通", "申请人老王，给小陈办，跟上次一样"), ("附件", "项目 A17"),
                ("浏览器", "打开门户"), ("账号页", "查询 C233"), ("角色页", "已选资料员"),
                ("结果页", "正在处理"),
            ])]
        (store / "observations.json").write_text(json.dumps(records), encoding="utf-8")
        env = {key: value for key, value in os.environ.items() if not key.startswith("OPENCOWORK_")}
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port),
                   OPENCOWORK_DESKTOP_PID=str(os.getpid()), AS_FIXTURE_KEY="fixture-only", NO_PROXY="127.0.0.1,localhost")
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        process = None
        import urllib.request
        http = urllib.request.build_opener(urllib.request.ProxyHandler({}))

        def api(path, body=None):
            request = urllib.request.Request(check.local_url(port) + "/api/" + path,
                data=None if body is None else json.dumps(body).encode(),
                headers={"Content-Type": "application/json"})
            with http.open(request, timeout=5) as result:
                return json.load(result)

        def start():
            nonlocal process
            process = subprocess.Popen([str(executable)], cwd=root, env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))

            def ready():
                try:
                    return api("assistant")
                except OSError:
                    return None
            return check.wait_for(ready)

        def stop():
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=10)

        try:
            start()
            api("assistant/observation", {"enabled": True})
            assert not attempted.wait(0.8), "observation alone sent an automatic request"
            api("assistant/decision", {"analyzeAutomatically": True, "dailyLimit": 1, "autoRunReady": True})
            assert attempted.wait(20), "the background loop did not process retained history"
            failed = check.wait_for(lambda: (view if (view := api("assistant"))["analysisError"] else None))
            assert failed["progress"]["through"] == 0 and failed["autoRemaining"] == 0
            stop()
            start()
            assert api("assistant")["progress"]["through"] == 0
            api("assistant/decision", {"analyzeAutomatically": True, "dailyLimit": 2, "autoRunReady": True})
            api("assistant/observation", {"enabled": True})
            finished = check.wait_for(lambda: (view if (view := api("assistant"))["progress"]["through"] >= 6 else None))
            assert len(received) == 2, len(received)
            for packet in received:
                assert [frame["observationId"] for frame in packet["new_observations"][:6]] == [
                    record["id"] for record in records]
            assert not finished["executionInFlight"]
            active = next(item for item in finished["cases"] if item["title"] == "回放申请 A17")
            assert active["request"]["project"] == "A17" and active["request"]["requester"] == "老王"
            assert not active["plan"] and not active["run"]
            with closing(sqlite3.connect(store / "assistant.sqlite3")) as db:
                checkpoint = json.loads(db.execute("SELECT body FROM learning WHERE id = 1").fetchone()[0])
            assert checkpoint["progress"]["cursor"]["seq"] >= 6 and len(checkpoint["auto_calls"]) == 2
            print("PASS: actual HTTP request covers six retained steps, omitted coverage rolls back, restart retries, and historical replay stays passive")
        finally:
            stop()
            server.shutdown()


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve())
