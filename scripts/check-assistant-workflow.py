"""Exercise interleaved cases, observed Windows actions, and Skill generation on a synthetic form."""

import ctypes
from ctypes import wintypes
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
from urllib.request import ProxyHandler, Request, build_opener
import uuid

sys.dont_write_bytecode = True
check = __import__("check-assistant")

def focus_fixture():
    user32 = ctypes.windll.user32
    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.GetWindowTextW.argtypes = (wintypes.HWND, wintypes.LPWSTR, ctypes.c_int)
    user32.SetForegroundWindow.argtypes = (wintypes.HWND,)
    user32.GetWindowThreadProcessId.argtypes = (wintypes.HWND, ctypes.c_void_p)
    user32.PeekMessageW.argtypes = (ctypes.POINTER(wintypes.MSG), wintypes.HWND, wintypes.UINT, wintypes.UINT, wintypes.UINT)
    windows = []
    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        caption = ctypes.create_unicode_buffer(512)
        user32.GetWindowTextW(hwnd, caption, len(caption))
        if caption.value.startswith("AS Approval Fixture"):
            windows.append(hwnd)
        return True
    user32.EnumWindows(visit, 0)
    focus_fixture.match_count = len(windows)
    if len(windows) != 1:
        return False
    hwnd = windows[0]
    previous = user32.GetForegroundWindow()
    if previous == hwnd:
        return True
    current = ctypes.windll.kernel32.GetCurrentThreadId()
    message = wintypes.MSG()
    user32.PeekMessageW(ctypes.byref(message), None, 0, 0, 0)
    owner = user32.GetWindowThreadProcessId(previous, None) if previous else 0
    target = user32.GetWindowThreadProcessId(hwnd, None)
    linked = [thread for thread in {owner, target} if thread and thread != current
              and user32.AttachThreadInput(current, thread, True)]
    try:
        user32.BringWindowToTop(hwnd)
        user32.SetForegroundWindow(hwnd)
    finally:
        for thread in linked:
            user32.AttachThreadInput(current, thread, False)
    return user32.GetForegroundWindow() == hwnd


class FixtureModel(check.Models):
    def do_GET(self):
        if self.path != "/form":
            self.send_error(404)
            return
        body = b"""<!doctype html><html lang="en"><meta charset="utf-8"><title>AS Approval Fixture</title>
        <style>body{font:30px sans-serif;margin:40px}input,button{font:inherit}</style>
        <h1>AS Approval Fixture</h1><form>
        <label>Member ID <input id="member" aria-label="Member ID" type="text" required></label>
        <button type="submit">Approve fixture</button></form>
        <p role="status">Waiting for verified member</p>
        <script>
        document.querySelector('form').addEventListener('submit', async (event) => {
          event.preventDefault();
          const value = document.querySelector('#member').value;
          const response = await fetch('/submit', {method:'POST',body:JSON.stringify({member:value})});
          if(response.ok) document.querySelector('[role=status]').textContent =
            (RECEIPT_WITHHELD ? 'Not Approved ' : 'Approved ') + value;
        });
        </script>""".replace(b"RECEIPT_WITHHELD", b"true" if self.server.withhold_receipt else b"false")
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path == "/submit":
            with (self.server.work_root / "submissions.txt").open("a", encoding="utf-8") as receipts:
                receipts.write(request["member"] + "\n")
            self.send_response(204)
            self.end_headers()
            return
        instructions, content = request["messages"][0]["content"], request["messages"][-1]["content"]
        assert isinstance(content, str)
        inputs = json.loads(content)
        if "reusable skill" in instructions.lower():
            output = {"title": "Record member approval", "trigger": "When an identified member requests project access",
                      "steps": ["Match the account number", "Select the verified role and submit"],
                      "checks": ["Look for confirmation containing the member number"], "exceptions": ["Wait for missing account number"]}
        else:
            latest, open_cases = inputs["latest"], inputs["open_cases"]
            if latest["window"].startswith("AS Approval Fixture"):
                case = next((item for item in open_cases if item["title"] == "Project A"), None)
                output = {"task": "Approve project A", "evidence": "AS Approval Fixture", "missing": [],
                          "choices": {"prepare": "Approve A948"}, "choice": "prepare",
                          "case": {"caseId": case and case["id"], "key": "fixture-request-project-a",
                                   "title": "Project A", "matchEvidence": "AS Approval Fixture",
                                   "facts": "Fixture member A948 requests read access", "status": "working"},
                          "plan": {"choiceId": "prepare", "before": "AS Approval Fixture", "verify": "Approved A948",
                                   "site": "127.0.0.1:" + str(self.server.server_port) if self.server.browser else None,
                                   "steps": [{"action": "setValue", "control": "Member ID", "value": "A948"},
                                             {"action": "invoke", "control": "Approve fixture", "value": None}]}}
            else:
                is_a = "project A" in latest["text"] or "A948" in latest["text"]
                title = "Project A" if is_a else "Project B"
                key = "fixture-request-project-" + ("a" if is_a else "b")
                case = next((item for item in open_cases if item["title"] == title), None)
                output = {"task": title, "evidence": latest["text"], "missing": ["account"],
                          "choices": {"wait": "Wait for member ID"}, "choice": "wait",
                          "case": {"caseId": case and case["id"], "key": key, "title": title,
                                   "matchEvidence": latest["text"], "facts": latest["text"], "status": "waiting"}}
        body = json.dumps({"choices": [{"index": 0, "message": {"role": "assistant",
            "content": json.dumps(output)}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 80, "completion_tokens": 90}}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


FORM = r"""
param([string]$Root, [string]$Withhold)
Add-Type -AssemblyName PresentationFramework
$window = [Windows.Window]::new()
$window.Title = 'AS Approval Fixture'
$window.Width = 750
$window.Height = 440
$window.WindowStartupLocation = 'CenterScreen'
$window.Topmost = $true
$panel = [Windows.Controls.StackPanel]::new()
$panel.Margin = '35'
$headline = [Windows.Controls.TextBlock]::new()
$headline.Text = 'AS Approval Fixture'
$headline.FontSize = 42
$panel.Children.Add($headline) | Out-Null
$editor = [Windows.Controls.TextBox]::new()
[Windows.Automation.AutomationProperties]::SetName($editor, 'Member ID')
$editor.FontSize = 34
$panel.Children.Add($editor) | Out-Null
$submit = [Windows.Controls.Button]::new()
$submit.Content = 'Approve fixture'
$submit.FontSize = 32
$panel.Children.Add($submit) | Out-Null
$result = [Windows.Controls.TextBlock]::new()
$result.Text = 'Waiting for verified member'
$result.FontSize = 32
$panel.Children.Add($result) | Out-Null
$submit.Add_Click({
    [IO.File]::AppendAllText((Join-Path $Root 'submissions.txt'), ($editor.Text + [Environment]::NewLine))
    $result.Text = $(if ($Withhold -eq 'yes') {'Not Approved '} else {'Approved '}) + $editor.Text
})
$receipt = [Windows.Threading.DispatcherTimer]::new()
$receipt.Interval = [TimeSpan]::FromMilliseconds(100)
$receipt.Add_Tick({
    if (Test-Path (Join-Path $Root 'release-receipt')) {
        $result.Text = 'Approved ' + $editor.Text
        $receipt.Stop()
    }
})
$receipt.Start()
$window.Content = $panel
$window.Add_ContentRendered({$window.Activate() | Out-Null; [IO.File]::WriteAllText((Join-Path $Root 'ready'), '1')})
$window.ShowDialog() | Out-Null
"""


def run(shell, use_browser, force_uia, auto_execute, withhold_receipt):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), FixtureModel)
    server.browser = use_browser
    server.withhold_receipt = withhold_receipt
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="oc-as-workflow-") as temp:
        root = Path(temp)
        server.work_root = root
        config = root / "config"
        data = config / "as-desktop"
        data.mkdir(parents=True)
        project = root / ".opencowork"
        project.mkdir()
        (project / "settings.json").write_text(json.dumps({"model": "fixture",
            "provider": {"baseUrl": check.local_url(server.server_port) + "/v1",
                         "name": "AS fixture", "apiKeyEnv": "AS_FIXTURE_KEY", "streaming": False}}), encoding="utf-8")
        log = []
        for title, body, elapsed in [
            ("Inbox A", "Please approve project A", 3800),
            ("Inbox B", "Please approve project B", 2400),
            ("Inbox A", "Account number A948", 1500),
        ]:
            log.append({"id": str(uuid.uuid4()), "timeMs": int(time.time()*1000) - elapsed, "window": title, "text": body})
        (data / "observations.json").write_text(json.dumps(log), encoding="utf-8")
        interrupted = str(uuid.uuid4())
        (data / "learning.json").write_text(json.dumps({"cases": [{
            "id": interrupted, "version": 1, "title": "Interrupted fixture", "status": "working",
            "updatedMs": int(time.time()*1000), "observations": [], "facts": "", "key": None,
            "outcome": None, "plan": None, "draft": None,
            "run": {"id": str(uuid.uuid4()), "planId": str(uuid.uuid4()), "status": "running",
                    "changed": 1, "detail": "Dispatching step 1"},
        }]}), encoding="utf-8")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = {key: val for key, val in os.environ.items() if not key.startswith("OPENCOWORK_")}
        env.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port),
                   OPENCOWORK_DESKTOP_PID=str(os.getpid()), AS_FIXTURE_KEY="fixture-only",
                   NO_PROXY="127.0.0.1,localhost")
        fixture_script = root / "form.ps1"
        fixture_script.write_text(FORM, encoding="utf-8-sig")
        opener = build_opener(ProxyHandler({}))
        host = subprocess.Popen([shell], cwd=root, env=env, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
        fixture = None
        browser = None
        playwright = None
        def api(path, payload=None, method=None, code=200):
            req = Request(check.local_url(port) + "/api/" + path, headers={"Content-Type": "application/json"},
                          data=json.dumps(payload).encode() if payload is not None else None, method=method)
            try:
                resp = opener.open(req, timeout=24)
            except HTTPError as error:
                resp = error
            with resp:
                body = resp.read()
                assert resp.status == code, (path, resp.status, body)
                return json.loads(body) if body else None
        def health():
            try:
                api("health", code=204)
                return True
            except OSError:
                return False
        try:
            check.wait_for(health)
            for entry in log:
                api("assistant/observations/" + entry["id"] + "/analyze", method="POST")
            initial = api("assistant")
            restored = next(item for item in initial["cases"] if item["id"] == interrupted)
            assert restored["run"]["status"] == "unverified" and restored["run"]["changed"] == 1
            cases = {item["title"]: item for item in initial["cases"]}
            a, b = cases["Project A"], cases["Project B"]
            assert a["observations"] == [log[0]["id"], log[2]["id"]]
            assert b["observations"] == [log[1]["id"]]
            assert [a["status"], b["status"]] == ["waiting", "waiting"]
            print("PASS: interleaved conversations and later account detail joined the correct case", flush=True)
            if auto_execute:
                api("assistant/decision", {"analyzeAutomatically": True, "autoRunReady": True,
                                             "showNotifications": True})
            if use_browser:
                from playwright.sync_api import sync_playwright
                playwright = sync_playwright().start()
                args = ["--enable-features=UiaProvider", "--force-renderer-accessibility=complete"] if force_uia else []
                browser = playwright.chromium.launch(channel="msedge", headless=False, args=args)
                page = browser.new_page()
                page.goto(check.local_url(server.server_port) + "/form")
                page.bring_to_front()
            else:
                fixture = subprocess.Popen(["powershell.exe", "-NoProfile", "-STA", "-File", str(fixture_script), str(root),
                                            "yes" if withhold_receipt else "no"],
                                           creationflags=subprocess.CREATE_NO_WINDOW, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                check.wait_for(lambda: (root / "ready").exists())
            try:
                check.wait_for(focus_fixture)
            except AssertionError as error:
                raise AssertionError(f"Could not foreground the isolated test window, matching windows={focus_fixture.match_count}") from error
            api("assistant/observation", {"enabled": True, "intervalMs": 500})
            def visible_form():
                observations = api("assistant")["observations"]
                if observations and observations[0]["window"].startswith("AS Approval Fixture"):
                    return observations[0]
            try:
                observation = check.wait_for(visible_form)
            except AssertionError as error:
                status = api("assistant")
                raise AssertionError(f"Fixture was not observed, phase={status['phase']}, error={status['error']}") from error
            def current():
                return next(item for item in api("assistant")["cases"] if item["id"] == a["id"])
            route = "assistant/cases/" + a["id"]
            if auto_execute:
                check.wait_for(lambda: current().get("run"))
                plan_id = current()["run"]["planId"]
            else:
                update = api("assistant/observations/" + observation["id"] + "/analyze", method="POST")
                assert update["caseId"] == a["id"]
                plan_id = current()["plan"]["id"]
                api(route + "/approve", {"planId": str(uuid.uuid4()), "mode": "once"}, code=400)
                assert not (root / "submissions.txt").exists()
                api(route + "/approve", {"planId": plan_id, "mode": "once"}, code=202)
                api(route + "/stop", method="POST", code=204)
                assert current()["run"]["status"] == "stopped"
                check.wait_for(lambda: not api("assistant")["executionInFlight"])
                assert not (root / "submissions.txt").exists()
                print("PASS: stopped before dispatch and recovered interrupted work without replay", flush=True)
                # The whole desktop can change after the stop, so refresh the proposal.
                if use_browser:
                    page.bring_to_front()
                check.wait_for(focus_fixture)
                latest = check.wait_for(visible_form)
                api("assistant/observations/" + latest["id"] + "/analyze", method="POST")
                plan_id = current()["plan"]["id"]
                api(route + "/approve", {"planId": plan_id, "mode": "always"}, code=202)
            def execution_result():
                item = current()
                if item["run"]["status"] == "waitingTarget":
                    focus_fixture()
                return item if item["run"]["status"] in ("done", "stopped", "unverified") else None
            try:
                result = check.wait_for(execution_result, 50)
            except AssertionError as error:
                raise AssertionError(f"Fixture execution did not settle: {current()['run']}") from error
            if withhold_receipt:
                assert result["run"]["status"] == "unverified", result["run"]
                assert (root / "submissions.txt").read_text().splitlines() == ["A948"]
                check.wait_for(focus_fixture)
                api(route + "/verify", method="POST", code=400)
                if use_browser:
                    page.evaluate("document.querySelector('[role=status]').textContent = 'Approved A948'")
                else:
                    (root / "release-receipt").touch()
                    time.sleep(0.3)
                api(route + "/verify", method="POST")
                result = current()
                print("PASS: rejected a negative receipt and recovered a delayed receipt without another submission", flush=True)
            assert result["status"] == "done", (result["run"]["changed"], result["run"]["detail"])
            assert (root / "submissions.txt").read_text().splitlines() == ["A948"]
            api(route + "/approve", {"planId": plan_id, "mode": "once"}, code=400)
            assert (root / "submissions.txt").read_text().splitlines() == ["A948"]
            api("assistant/observation", {"enabled": False})
            check.wait_for(lambda: current()["draft"])
            api(route + "/skill", method="POST")
            adopted = current()["draft"]["installed"]
            assert adopted and Path(adopted).is_file()
            assert "Match the account number" in Path(adopted).read_text(encoding="utf-8")
            installed = Path(adopted)
            original = installed.read_bytes() + b"\nUser's verified edit.\n"
            installed.write_bytes(original)
            api(route + "/skill", method="DELETE")
            assert current()["draft"]["installed"] is None
            assert not installed.exists()
            assert installed.with_name("SKILL.disabled.md").read_bytes() == original
            api(route + "/skill", method="POST")
            assert installed.read_bytes() == original
            if auto_execute:
                assert api("assistant/notice")["id"] == result["run"]["id"]
            else:
                grant = api("assistant")["grants"][0]
                api("assistant/grants/" + grant["id"], method="DELETE", code=204)
            assert not api("assistant")["grants"]
            print("PASS: approved a real " + ("Edge website" if use_browser else "UIA form")
                  + ", verified one submission, generated, installed and restored Skill, "
                  + ("ran without case approval and notified result" if auto_execute else "revoked standing access"), flush=True)
        finally:
            if browser:
                browser.close()
            if playwright:
                playwright.stop()
            for proc in [fixture, host]:
                if proc and proc.poll() is None:
                    proc.terminate()
                    proc.wait(timeout=10)
            server.shutdown()


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve(), "--browser" in sys.argv[2:], "--force-uia" in sys.argv[2:],
        "--auto" in sys.argv[2:], "--recover" in sys.argv[2:])
