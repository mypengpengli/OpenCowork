"""Exercise real Windows launcher routing with mock Cargo and locked old EXEs."""
import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import subprocess
import tempfile


def check_launcher():
    repository = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="opencowork launcher & ") as temporary:
        root = Path(temporary)
        first, second = root / "main project", root / "another checkout"
        shared = root / "local"
        marker = root / "cargo-called.txt"
        for checkout in (first, second):
            (checkout / "scripts").mkdir(parents=True)
            for name in ("Start-OpenClaw.bat", "Build-OpenCowork.bat"):
                content = (repository / name).read_text().replace("  pause", "  rem no pause in test")
                (checkout / name).write_text(content)
            script = (repository / "scripts/launch-windows.ps1").read_text()
            # Mock the GUI launch only. Build, staging and state updates stay real.
            anchor = "$ErrorActionPreference = 'Stop'"
            script = script.replace(anchor, anchor + "\nfunction Start-Process { param([string]$FilePath, [string]$WorkingDirectory) Write-Output \"LAUNCH|$FilePath|$WorkingDirectory\" }", 1)
            (checkout / "scripts/launch-windows.ps1").write_text(script)
        (root / "cargo.cmd").write_text(
            '@echo off\n'
            'echo build>>"%OPENCOWORK_TEST_BUILD_MARKER%"\n'
            'if "%OPENCOWORK_TEST_BUILD_FAIL%"=="1" exit /b 1\n'
            'if not exist "%LOCALAPPDATA%\\OpenClaw\\target\\launcher" mkdir "%LOCALAPPDATA%\\OpenClaw\\target\\launcher"\n'
            'echo "%CD%">"%LOCALAPPDATA%\\OpenClaw\\target\\launcher\\opencowork-shell.exe"\n'
            'echo "%CD%">"%LOCALAPPDATA%\\OpenClaw\\target\\launcher\\opencowork-desktop.exe"\n'
            'exit /b 0\n')
        environment = dict(os.environ)
        environment.update(LOCALAPPDATA=str(shared), PATH=str(root) + ";" + os.environ["PATH"],
                           OPENCOWORK_TEST_BUILD_MARKER=str(marker))

        def run(checkout=first, argument="", fails=False, build_only=False):
            command = "Build-OpenCowork.bat" if build_only else "Start-OpenClaw.bat"
            result = subprocess.run([os.environ["COMSPEC"], "/d", "/c", command, argument],
                                    cwd=checkout, env=environment, capture_output=True, text=True,
                                    timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
            assert (result.returncode != 0) == fails, result.stdout + result.stderr
            return result.stdout

        def launched(output):
            return Path(next(line.split("|")[1] for line in output.splitlines() if line.startswith("LAUNCH|")))

        old = launched(run())
        assert old.is_file() and str(first) in old.read_text()
        marker.unlink()
        assert launched(run()) == old and not marker.exists()
        # Lock BOTH old binaries just like Windows does while an app is running.
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateFileW.restype = wintypes.HANDLE
        kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        handles = []
        try:
            for path in (old, old.with_name("opencowork-shell.exe")):
                handle = kernel.CreateFileW(str(path), 0x80000000, 1, None, 3, 0x80, None)
                assert handle not in (None, wintypes.HANDLE(-1).value), ctypes.get_last_error()
                handles.append(handle)
            new = launched(run(argument="--build"))
            assert new != old and old.is_file() and new.is_file()
            assert new.read_text() == old.read_text()
        finally:
            for handle in handles:
                kernel.CloseHandle(handle)
        marker.unlink()
        other = launched(run(second))
        assert other.parent.parent.parent != new.parent.parent.parent
        assert str(second) in other.read_text() and str(first) in new.read_text()
        marker.unlink()
        assert launched(run()) == new and not marker.exists(), "Another checkout changed main's runtime"
        # Standalone build stages a new runtime but never starts it.
        assert "LAUNCH|" not in run(build_only=True)
        staged = launched(run())
        assert staged != new
        environment["OPENCOWORK_TEST_BUILD_FAIL"] = "1"
        assert "LAUNCH|" not in run(argument="--build", fails=True)
        environment.pop("OPENCOWORK_TEST_BUILD_FAIL")
        assert launched(run()) == staged, "Failed build replaced the known-good runtime"
        # Missing shell triggers rebuild rather than launching an incomplete pair.
        staged.with_name("opencowork-shell.exe").unlink()
        assert launched(run()) != staged
        print("PASS: cached launch, explicit build with locked EXEs, per-checkout isolation, build-only staging, failed-build preservation and missing-payload rebuild")


if __name__ == "__main__":
    check_launcher()
