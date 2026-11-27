"""Verify the native WebView2 IPC updater UI in an isolated app copy and profile."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import tomllib
import urllib.request
from playwright.sync_api import sync_playwright, expect

expected_version = tomllib.loads((Path(__file__).resolve().parents[1]/'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']


def free_port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


with tempfile.TemporaryDirectory(prefix='oc-native-updater-') as directory:
    root = Path(directory)
    app, workspace, local = root/'app', root/'workspace', root/'local'
    for folder in (app, workspace/'.opencowork', local):
        folder.mkdir(parents=True)
    source = Path(sys.argv[1]).resolve()
    for filename in ('opencowork-shell.exe', 'opencowork-desktop.exe'):
        shutil.copy2(source/filename, app/filename)
    # Emulate installed-app metadata only in this private temporary copy.
    (app/'installation.ini').write_text('[Installation]\nTestMode=0\n', encoding='utf-8')
    (app/'Uninstall.exe').write_bytes(b'fixture only, never executed')
    (workspace/'.opencowork/settings.json').write_text(json.dumps({'model':'fixture', 'teamMemorySync':{'enabled':False}}))
    port = free_port()
    environment = {k:v for k,v in os.environ.items() if not k.startswith('OPENCOWORK_')}
    environment.update(LOCALAPPDATA=str(local), OPENCOWORK_CONFIG_HOME=str(root/'config'),
                       WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}')
    process = subprocess.Popen([str(app/'opencowork-desktop.exe')], cwd=workspace, env=environment,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    try:
        with sync_playwright() as playwright:
            browser = None
            for _ in range(150):
                try:
                    targets = json.load(urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list', timeout=1))
                    if not any(t.get('url', '').startswith('http://127.0.0.1:') for t in targets):
                        time.sleep(.1)
                        continue
                    browser = playwright.chromium.connect_over_cdp(f'http://127.0.0.1:{port}', timeout=1000)
                    break
                except Exception:
                    time.sleep(.1)
            assert browser is not None, 'Native WebView2 debugging connection unavailable'
            page = None
            for _ in range(100):
                pages = [p for c in browser.contexts for p in c.pages if p.url.startswith('http://127.0.0.1:')]
                if pages:
                    page = pages[0]
                    break
                time.sleep(.1)
            if page is None:
                targets = urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list').read().decode()
                startup_log = local/'OpenCowork/logs/startup.log'
                raise AssertionError({'targets': targets, 'pages': [p.url for c in browser.contexts for p in c.pages], 'process': process.poll(), 'startup': startup_log.read_text() if startup_log.exists() else None})
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.locator('#nav-settings-button').click()
            page.locator('[data-settings-tab="environment"]').click()
            card = page.locator('#app-updates-card')
            expect(card).to_contain_text(f'当前版本: {expected_version}')
            checkbox = card.locator('#auto-update-enabled')
            expect(checkbox).to_be_checked()
            checkbox.uncheck()
            expect(checkbox).not_to_be_checked()
            preferences = local/'OpenCowork/updates/preferences.json'
            for _ in range(100):
                if preferences.exists() and json.loads(preferences.read_text())['autoEnabled'] is False:
                    break
                time.sleep(.05)
            assert json.loads(preferences.read_text())['autoEnabled'] is False
            # A page cannot install an unverified payload by fabricating ready UI state.
            page.evaluate("window.ipc.postMessage(JSON.stringify({action:'updates:install'}))")
            expect(card).to_contain_text('No verified update is ready')
            assert process.poll() is None
            card.get_by_role('button', name='检查更新', exact=True).click()
            expect(card).to_contain_text('已是最新正式版', timeout=45000)
            page.locator('#locale-toggle-button').click()
            expect(card.get_by_role('button', name='Check for updates', exact=True)).to_be_visible()
            assert page.locator('#nav-history-button').count() == 0
            assert page.locator('#nav-schedules-button').count() == 1
            artifact = Path(__file__).resolve().parents[1]/'.tmp/updater-native.png'
            page.screenshot(path=str(artifact), full_page=True)
            assert not errors, errors
            browser.close()
            # Gracefully close the owned window, allowing its backend to shut down.
            subprocess.run(['powershell.exe','-NoProfile','-Command',f'(Get-Process -Id {process.pid}).CloseMainWindow() | Out-Null'], check=True)
            process.wait(15)
        print('PASS: native WebView2 update status IPC, auto-update preference persistence, unverified-install rejection, localization and main navigation')
    finally:
        if process.poll() is None:
            subprocess.run(['taskkill','/PID',str(process.pid),'/T','/F'], capture_output=True)
            process.wait(10)
        # WebView2 child processes can take a moment to release their profile.
        time.sleep(3)
