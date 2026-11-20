"""Exercise the release installer with isolated shortcuts, registry and app files."""
import configparser
import ctypes
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import winreg

installer = Path(sys.argv[1]).resolve(strict=True)
registration = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenCowork-InstallerTest'
try:
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registration):
        raise SystemExit('An existing installer test registration must be inspected first.')
except FileNotFoundError:
    pass

def special_folder(identifier):
    result = ctypes.create_unicode_buffer(32768)
    assert ctypes.windll.shell32.SHGetFolderPathW(None, identifier, None, 0, result) == 0
    return Path(result.value)

with tempfile.TemporaryDirectory(prefix='opencowork-installer-') as directory:
    root = Path(directory)
    application = root / 'app'
    workspace = application / 'workspace'
    environment = {k: v for k, v in os.environ.items() if not k.startswith('OPENCOWORK_')}
    desktop_link = special_folder(0x10) / 'OpenCowork-InstallerTest.lnk'
    program_group = special_folder(0x02) / 'OpenCowork-InstallerTest'
    assert not desktop_link.exists() and not program_group.exists()

    def install():
        result = subprocess.run([str(installer), '/S', '/TESTMODE', '/D=' + str(application)], env=environment, timeout=120)
        assert result.returncode == 0, result.returncode
        for name in ('opencowork-shell.exe', 'opencowork-desktop.exe', 'Uninstall.exe', 'installation.ini'):
            assert (application / name).is_file(), name
        assert desktop_link.is_file()
        assert (program_group / 'OpenCowork.lnk').is_file()
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registration, access=winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as key:
            assert winreg.QueryValueEx(key, 'InstallLocation')[0] == str(application)
            assert winreg.QueryValueEx(key, 'DisplayVersion')[0] == '0.6.0'

    try:
        install()
        (workspace / 'keep.txt').write_text('User files must survive upgrade and uninstall.', encoding='utf-8')
        configuration = configparser.ConfigParser()
        configuration.read(application / 'installation.ini', encoding='utf-8-sig')
        assert configuration['Installation']['TestMode'] == '1'
        first_hash = hashlib.sha256((application / 'opencowork-shell.exe').read_bytes()).hexdigest()
        install()
        assert hashlib.sha256((application / 'opencowork-shell.exe').read_bytes()).hexdigest() == first_hash
        assert (workspace / 'keep.txt').read_text(encoding='utf-8').startswith('User files')
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        environment.update(OPENCOWORK_CONFIG_HOME=str(root / 'config'), OPENCOWORK_SHELL_PORT=str(port))
        host = subprocess.Popen([str(application / 'opencowork-shell.exe')], cwd=workspace, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=0x08000000)
        try:
            for _ in range(200):
                try:
                    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=1)
                    connection.request('GET', '/api/health')
                    response = connection.getresponse()
                    body=response.read()
                    assert response.status in (200,204),(response.status,body)
                    connection.close()
                    break
                except OSError:
                    time.sleep(.05)
            else:
                raise AssertionError('Installed application did not become healthy')
        finally:
            host.terminate()
            host.wait(10)
    finally:
        uninstaller = application / 'Uninstall.exe'
        if uninstaller.exists():
            result = subprocess.run([str(uninstaller), '/S'], timeout=60)
            assert result.returncode == 0
            for _ in range(200):
                if not (application / 'opencowork-shell.exe').exists():
                    break
                time.sleep(.05)
        assert not (application / 'opencowork-shell.exe').exists()
        assert not (application / 'opencowork-desktop.exe').exists()
        assert not desktop_link.exists() and not program_group.exists()
        assert (workspace / 'keep.txt').exists(), 'Uninstall removed user data'
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registration):
                raise AssertionError('Uninstall registration was retained')
        except FileNotFoundError:
            pass
    print('PASS: silent install, desktop/start-menu shortcuts, uninstall registration, upgrade, installed host health and user-data-preserving uninstall')
