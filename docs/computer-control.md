# Windows computer control: implementation and Codex comparison

This comparison uses the Windows `@oai/sky` window API documentation installed
with the Codex computer-use plugin (bundle `26.820.60940`), reviewed on
2026-09-09. It describes that interface, not every Codex release or unpublished
implementation. OpenCowork implements equivalent local behavior independently;
it does not bundle or require OpenAI's helper or a Zhipu service.

| Capability | Previous OpenCowork | Current implementation |
| --- | --- | --- |
| Apps and windows | Process list; first matching process window | Installed/running app catalog, launch, explicit HWND + PID + process creation identity, owned-window metadata, ambiguous process selection rejected |
| Targeted input | Global desktop coordinates | Required returned window and fresh state ID; automatic activation, window-relative physical pixels and hit-testing |
| Accessibility actions | Names, roles and coordinates | Indexed controls, runtime identity checks, direct value replacement, Invoke, Toggle, Expand, Collapse, Select and ScrollIntoView when advertised by the control |
| Reading state | Active window, up to 100 elements | Selected window, up to 200 elements, bounded text, focus/value/selection/document fields, optional screenshot/text |
| Stale state | No binding between observation and input | Latest single-use state; 120-second expiry; window identity/geometry, element identity/geometry, screenshot ID and focus checks; desktop mutex across workers |
| Keyboard and pointer | Fixed shortlist, left/right click, vertical wheel | Chords, modifiers, F1-F24, numpad/navigation/punctuation keys; middle/multiple clicks, two-axis scrolling at an explicit point, drag and bounded wait |
| Window screenshots | Whole visible desktop only | Windows Graphics Capture in an owned Rust subprocess, including occluded windows; PrintWindow fallback with reported reason; explicit visible-screen mode |
| Feedback loop | Manual follow-up snapshot | Targeted input returns a fresh observation; failure to observe after input reports that input already happened and requires re-observation |
| Image delivery | Current-turn JPEG for the model | Window/state/coordinate metadata accompanies the latest JPEG; newer observations/errors invalidate older images; chat includes a screenshot preview |

The observe → act → observe loop follows the [official OpenAI computer-use
guide](https://developers.openai.com/api/docs/guides/tools-computer-use).
Windows Graphics Capture uses the MIT-licensed
[windows-capture Rust library](https://github.com/NiiightmareXD/windows-capture).

## Usage

Computer control remains enabled by default, with the existing switch under
Settings > Permissions. Existing tool permission policy continues to apply.

1. Call `Computer` with `action: "list_windows"` (or `list_apps`). Choose the exact
   returned window. For apps not running, use `launch_app`, then list again.
2. Call `get_window_state` with that `window`. Images and text are enabled by
   default. Text-only models should use `includeScreenshot: false`.
3. Call one input action with `window` and the latest `stateId`. Click an
   `elementIndex` or pass `screenshotId` and window-relative `x`/`y`. For typing,
   first click an editable surface and inspect the returned focus.
4. Inspect the fresh result before the next action. On failure, observe again;
   never blindly repeat a possibly partial input.

`scrollX > 0` scrolls right, `scrollY > 0` down. `set_value` uses `text` as the
replacement. `secondary_action` uses `secondaryAction` from the control's
advertised actions. `key` accepts names such as `Control_L+a`,
`Control_L+Shift_L+period`, `F5`, `Home`, and `KP_0`.

Global `snapshot` and `screenshot` remain read-only legacy inspection actions.
Global input without a target window is rejected. Saved old model/tool history
can still be read; the model must obtain a new window state to continue input.
Mouse click/drag press and release events are submitted together in one native
input batch, so cancellation cannot stop the helper between separate down/up
calls. Input already queued to Windows cannot be recalled by stopping the turn.

## Remaining differences and practical limits

- The host still starts an isolated PowerShell input helper per call. Codex's
  persistent helper avoids this startup cost. We have not measured equal latency
  or equal real-model task success rates.
- The OpenAI Responses `computer` protocol is distinct from this project's
  OpenAI-compatible function-tool/image route. This implementation keeps existing
  providers working; it does not reproduce proprietary model training or Codex's
  complete agent orchestration.
- Related popups are discoverable through window listing and owner metadata;
  they are not returned as multiple separately indexed screenshot layers in one
  result as in the Codex plugin interface. Select and observe each modal/window.
- WGC requires Windows support and a working graphics driver. Minimized,
  protected and some accelerated windows can fail capture; the result reports
  the actual backend and fallback reason. Visible capture can include occluders.
- Geometry and identity checks reduce stale-input mistakes but cannot prove
  that every pixel stayed unchanged since observation. Accessibility support
  depends on the app; unsupported patterns return errors with a keyboard path.
- Mouse/keyboard input shares the interactive desktop and can be denied by
  Windows focus rules or privilege boundaries. It is not a sandbox. Screens and
  accessibility contents are untrusted data, not user authorization.
- The exact proprietary Codex confirmation UI and enforcement are not copied.
  OpenCowork retains its own existing tool permissions and stop/cancellation
  flow. It does not claim equivalent safety or permission semantics.

## Checks

`scripts/check-computer-window.ps1` creates disposable WPF windows and checks
window selection, Unicode input, chords, direct value setting, accessibility
actions, screenshot geometry, stale-state rejection, occluded WGC capture,
focus switching, and paired mouse press/release for drag and middle click.
It briefly changes focus only among its own test windows. Pass
`-CaptureExecutable path/to/opencowork-shell.exe` to test a portable build.
`scripts/check-computer-input.ps1` remains an alias for this expanded check.

`scripts/check-features.py` uses an isolated mock provider for screenshot/image
payloads and feature settings, alongside the P1/P2 regression checks.
