"""Check observation pages and live updates using isolated data and a headless browser."""

from contextlib import closing
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request
import uuid

from playwright.sync_api import sync_playwright, expect


def run(executable):
    with tempfile.TemporaryDirectory(prefix="oc-as-history-") as folder:
        root = Path(folder)
        config = root / "config"
        store = config / "as-desktop"
        store.mkdir(parents=True)
        (root / ".opencowork").mkdir()
        (root / ".opencowork/settings.json").write_text(json.dumps({
            "model": "fixture",
            "provider": {"name": "Fixture", "baseUrl": "http://127.0.0.1:9/v1", "apiKeyEnv": "AS_HISTORY_KEY"},
        }), encoding="utf-8")
        now = int(time.time() * 1000)
        records = [{
            "id": str(uuid.uuid4()), "timeMs": now - 50_000 + index * 1000,
            "window": f"Observation {index}", "text": f"Visible evidence {index}", "app": "fixture.exe",
        } for index in range(41)]
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        env = {key: value for key, value in os.environ.items() if not key.startswith("OPENCOWORK_")}
        env.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port), AS_HISTORY_KEY="fixture-key")
        process = None
        with (root / "shell.log").open("w", encoding="utf-8") as log:
            def api(path, method="GET"):
                with http.open(urllib.request.Request(base + path, method=method), timeout=5) as response:
                    raw = response.read()
                    return json.loads(raw) if raw else None

            def start():
                nonlocal process
                if process:
                    process.terminate()
                    process.wait(timeout=10)
                database = store / "assistant.sqlite3"
                if not database.exists():
                    (store / "observations.json").write_text(json.dumps(records), encoding="utf-8")
                else:
                    with closing(sqlite3.connect(database)) as db, db:
                        item = records[-1]
                        db.execute("INSERT INTO observations (id, time_ms, body) VALUES (?, ?, ?)",
                                   (item["id"], item["timeMs"], json.dumps(item)))
                process = subprocess.Popen([executable], cwd=root, env=env, stdout=log, stderr=log,
                                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                for _ in range(100):
                    try:
                        api("/api/health")
                        return
                    except OSError:
                        time.sleep(0.1)
                raise AssertionError("The isolated shell did not start")

            try:
                start()
                first = api("/api/assistant/observations")
                assert first["page"] == 1 and first["pages"] == 5 and first["total"] == 41
                assert len(first["observations"]) == 10
                anchor = first["anchor"]
                second = api(f"/api/assistant/observations?page=2&anchor={anchor}")
                expected = [item["id"] for item in second["observations"]]
                assert not set(expected).intersection(item["id"] for item in first["observations"])
                assert api("/api/assistant/observations?page=9999")["page"] == 5
                assert api("/api/assistant/observations?pageSize=999")["pageSize"] == 50
                assert api("/api/assistant/observations?anchor=" + str(uuid.uuid4()))["anchorExpired"]
                with sync_playwright() as p:
                    browser = p.chromium.launch(channel="msedge", headless=True)
                    page = browser.new_page(viewport={"width": 1440, "height": 1050})
                    errors = []
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.goto(base, wait_until="networkidle")
                    page.locator("#nav-assistant-button").click()
                    rows = page.locator("#assistant-observations > details")
                    expect(rows).to_have_count(10)
                    assert page.locator('#assistant-decision-form [name="endpoint"]').count() == 0
                    page.locator("#assistant-history-next").click()
                    expect(page.locator("#assistant-history-page")).to_contain_text("2 / 5")
                    assert rows.evaluate_all("(rows) => rows.map(row => row.dataset.id)") == expected
                    rows.first.locator("summary").click()
                    draft_id = expected[0]
                    editor = rows.first.locator(".assistant-outcome textarea").first
                    editor.fill("Unsaved feedback survives live updates and paging")
                    new = {
                        "id": str(uuid.uuid4()), "timeMs": int(time.time() * 1000),
                        "window": "New foreground observation", "text": "New evidence", "app": "fixture.exe",
                    }
                    records.append(new)
                    start()
                    expect(page.locator("#assistant-history-latest")).to_contain_text("有新记录", timeout=10000)
                    assert rows.evaluate_all("(rows) => rows.map(row => row.dataset.id)") == expected
                    expect(editor).to_have_value("Unsaved feedback survives live updates and paging")
                    page.locator("#assistant-history-latest").click()
                    expect(rows.first).to_have_attribute("data-id", new["id"])
                    page.locator("#assistant-history-next").click()
                    restored = page.locator(f'#assistant-observations > details[data-id="{draft_id}"]')
                    expect(restored.locator(".assistant-outcome textarea").first).to_have_value(
                        "Unsaved feedback survives live updates and paging")
                    for number in range(3, 6):
                        page.locator("#assistant-history-next").click()
                        expect(page.locator("#assistant-history-page")).to_contain_text(f"{number} / 5")
                    expect(rows).to_have_count(2)
                    expect(page.locator("#assistant-history-next")).to_be_disabled()
                    api("/api/assistant/observations/" + new["id"], method="DELETE")
                    expect(page.locator("#assistant-history-page")).to_contain_text("1 / 5", timeout=10000)
                    expect(rows).to_have_count(10)
                    assert not errors, errors
                    browser.close()
                print("PASS: bounded pages, stable history under new observations, preserved drafts, latest navigation and deleted-anchor recovery")
            finally:
                if process and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)


if __name__ == "__main__":
    run(str(Path(sys.argv[1]).resolve()))
