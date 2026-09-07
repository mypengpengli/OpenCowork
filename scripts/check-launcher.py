"""Check Windows launcher branches with mock Cargo and no desktop processes."""

import os
from pathlib import Path
import subprocess
import tempfile


def check_launcher():
    repository = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="opencowork launcher & ") as temporary:
        root = Path(temporary)
        # Only replace process launch and pause commands; exercise real build routing.
        launcher = (repository / "Start-OpenClaw.bat").read_text()
        launcher = launcher.replace('start "" "%DESKTOP_EXE%"', 'echo DESKTOP_LAUNCHED')
        launcher = launcher.replace('start "" "%SHELL_EXE%"', 'echo SHELL_LAUNCHED')
        launcher = launcher.replace('start "" "%SHELL_URL%"', 'echo BROWSER_LAUNCHED')
        launcher = launcher.replace('timeout /t 2 /nobreak >nul', 'rem No delay in test')
        launcher = launcher.replace('  pause', '  rem No pause in test')
        (root / "Start-OpenClaw.bat").write_text(launcher)
        (root / "Build-OpenCowork.bat").write_text((repository / "Build-OpenCowork.bat").read_text())
        target = root / "local" / "OpenClaw" / "target"
        (target / "debug").mkdir(parents=True)
        marker = root / "cargo-called.txt"
        (root / "cargo.cmd").write_text(
            '@echo off\n'
            'echo build>>"%OPENCOWORK_TEST_BUILD_MARKER%"\n'
            'if "%OPENCOWORK_TEST_BUILD_FAIL%"=="1" exit /b 1\n'
            'type nul >"%LOCALAPPDATA%\\OpenClaw\\target\\debug\\opencowork-shell.exe"\n'
            'type nul >"%LOCALAPPDATA%\\OpenClaw\\target\\debug\\opencowork-desktop.exe"\n'
            'exit /b 0\n'
        )
        environment = dict(os.environ)
        environment.update(
            LOCALAPPDATA=str(root / "local"),
            PATH=str(root) + ";" + str(Path(os.environ["WINDIR"]) / "System32"),
            OPENCOWORK_TEST_BUILD_MARKER=str(marker),
        )

        def run(argument="", fails=False):
            result = subprocess.run(
                [os.environ["COMSPEC"], "/d", "/c", "Start-OpenClaw.bat", argument],
                cwd=root, env=environment, capture_output=True, text=True, timeout=10,
                creationflags=subprocess.CREATE_NO_WINDOW,
            )
            assert (result.returncode != 0) == fails, result.stdout + result.stderr
            return result.stdout

        assert "DESKTOP_LAUNCHED" in run()
        assert marker.exists()
        assert (target / "build-root.txt").read_text() == str(root) + "\\"
        marker.unlink()
        assert "DESKTOP_LAUNCHED" in run()
        assert not marker.exists(), "Cached launch invoked Cargo"
        assert "DESKTOP_LAUNCHED" in run("--build")
        assert marker.exists()
        marker.unlink()
        (target / "build-root.txt").write_text("D:\\another-checkout\\")
        assert "DESKTOP_LAUNCHED" in run()
        assert marker.exists(), "Different checkout used old binaries"
        marker.unlink()
        (target / "debug" / "opencowork-desktop.exe").unlink()
        assert "SHELL_LAUNCHED" in run()
        assert not marker.exists(), "Web fallback unexpectedly rebuilt"
        environment["OPENCOWORK_TEST_BUILD_FAIL"] = "1"
        failed = run("--build", fails=True)
        assert "DESKTOP_LAUNCHED" not in failed and "SHELL_LAUNCHED" not in failed
        print("PASS: first build, cached launch, explicit build, checkout change, web fallback, build failure")


if __name__ == "__main__":
    check_launcher()
