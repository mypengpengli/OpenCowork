use super::{
    execution::{Plan, Reply},
    screen::{self, ChatFrame, Screen},
};
use anyhow::{ensure, Context, Result};
use std::{ffi::c_void, mem::size_of, thread, time::Duration};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    UI::{
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::{
            GetAncestor, GetSystemMetrics, GetWindowRect, WindowFromPoint, GA_ROOT,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    },
};

struct Chat<'a> {
    frame: &'a ChatFrame,
    header: RECT,
    composer_top: i32,
    send: RECT,
}

impl<'a> Chat<'a> {
    fn read(frame: &'a ChatFrame, recipient: &str) -> Result<Self> {
        let width = frame.bounds.right - frame.bounds.left;
        let height = frame.bounds.bottom - frame.bounds.top;
        let header = unique_line(frame, |text, r| {
            r.left > width / 6
                && r.left < 3 * width / 4
                && r.top < 110
                && r.top < height / 5
                && compact(text) == compact(recipient)
        })
        .context("Exactly one matching conversation title must be visible near the top")?;
        let send = unique_line(frame, |text, r| {
            r.left > 2 * width / 3
                && r.top > 3 * height / 4
                && height - r.bottom <= 110
                && matches!(compact(text).as_str(), "发送" | "Send")
        })
        .context("Exactly one Send button must be visible at the bottom right")?;
        let composer_top = frame
            .composer_top(header.left, send)
            .context("Could not identify the separator above the message editor")?;
        Ok(Self {
            frame,
            header,
            composer_top,
            send,
        })
    }

    fn draft(&self) -> String {
        self.text_in(|r| {
            r.left + 20 >= self.header.left
                && r.top >= self.composer_top + 4
                && r.bottom < self.send.top - 4
        })
    }

    fn outgoing(&self, message: &str) -> bool {
        let width = self.frame.bounds.right - self.frame.bounds.left;
        let text = self.text_in(|r| {
            r.left >= self.header.left
                && r.right > width * 2 / 3
                && r.top > self.header.bottom + 20
                && r.bottom < self.composer_top - 3
        });
        let expected = compact(message);
        let lines = text.lines().map(compact).collect::<Vec<_>>();
        for start in 0..lines.len() {
            let mut observed = String::new();
            for line in &lines[start..] {
                observed.push_str(line);
                if observed == expected {
                    return true;
                }
                if !expected.starts_with(&observed) {
                    break;
                }
            }
        }
        false
    }

    fn text_in(&self, contains: impl Fn(RECT) -> bool) -> String {
        let mut lines = self
            .frame
            .lines
            .iter()
            .filter(|(_, r)| contains(*r))
            .collect::<Vec<_>>();
        lines.sort_by_key(|(_, rect)| (rect.top, rect.left));
        lines
            .into_iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn editor_empty(&self) -> bool {
        matches!(
            compact(&self.draft()).as_str(),
            "" | "输入消息" | "Typeamessage"
        )
    }

    fn send_point(&self) -> POINT {
        POINT {
            x: self.frame.bounds.left + (self.send.left + self.send.right) / 2,
            y: self.frame.bounds.top + (self.send.top + self.send.bottom) / 2,
        }
    }

    fn editor_point(&self) -> POINT {
        POINT {
            x: self.frame.bounds.left + self.header.left + 28,
            y: self.frame.bounds.top
                + self.composer_top
                + 2 * (self.send.bottom - self.send.top)
                + 8,
        }
    }
}

fn unique_line(frame: &ChatFrame, matches: impl Fn(&str, RECT) -> bool) -> Option<RECT> {
    let mut candidates = frame.lines.iter().filter(|(text, r)| matches(text, *r));
    let (_, rect) = candidates.next()?;
    candidates.next().is_none().then_some(*rect)
}

fn compact(value: &str) -> String {
    value.split_whitespace().collect()
}

pub(super) fn run(
    plan: &Plan,
    reply: &Reply,
    input_stamp: u32,
    check: impl Fn() -> std::result::Result<(), String>,
    progress: impl Fn(usize) -> std::result::Result<(), String>,
) -> std::result::Result<String, String> {
    screen::in_apartment(|| {
        let scanner = Screen::new()?;
        let current = |stamp| -> Result<()> {
            check().map_err(anyhow::Error::msg)?;
            ensure!(
                screen::foreground_input() == Some((plan.hwnd, plan.pid, stamp)),
                "The user changed focus or used the desktop"
            );
            Ok(())
        };
        let snapshot = || scanner.chat_frame(plan.hwnd, plan.pid);
        current(input_stamp)?;
        let frame = snapshot()?;
        let chat = Chat::read(&frame, &reply.recipient)?;
        ensure!(
            chat.editor_empty() && !chat.outgoing(&reply.message),
            "An existing draft or an identical visible message needs your review"
        );
        let point = chat.editor_point();
        ensure!(
            point.y < chat.send_point().y - 12,
            "The message editor is too small"
        );
        progress(1).map_err(anyhow::Error::msg)?;
        current(input_stamp)?;
        click(plan.hwnd, frame.bounds, point)?;
        let after_focus = stamp(plan.hwnd, plan.pid)?;
        thread::sleep(Duration::from_millis(160));
        let frame = snapshot()?;
        let chat = Chat::read(&frame, &reply.recipient)?;
        ensure!(
            chat.editor_point() == point && chat.editor_empty(),
            "The editor changed before typing"
        );
        current(after_focus)?;
        type_text(&reply.message)?;
        let after_typing = stamp(plan.hwnd, plan.pid)?;
        thread::sleep(Duration::from_millis(200));
        let frame = snapshot()?;
        let chat = Chat::read(&frame, &reply.recipient)?;
        ensure!(
            compact(&chat.draft()) == compact(&reply.message) && !chat.outgoing(&reply.message),
            "The editor text did not exactly match the approved message"
        );
        progress(2).map_err(anyhow::Error::msg)?;
        current(after_typing)?;
        click(plan.hwnd, frame.bounds, chat.send_point())?;
        let after_send = stamp(plan.hwnd, plan.pid)?;
        for _ in 0..6 {
            thread::sleep(Duration::from_millis(350));
            current(after_send)?;
            let frame = snapshot()?;
            let chat = Chat::read(&frame, &reply.recipient)?;
            current(after_send)?;
            if chat.editor_empty() && chat.outgoing(&reply.message) {
                return Ok(receipt(reply));
            }
        }
        anyhow::bail!("The send button was clicked, but the expected message was not confirmed");
    })
    .map_err(|error| error.to_string())
}

pub(super) fn verify(hwnd: u64, pid: u32, reply: &Reply) -> std::result::Result<String, String> {
    screen::in_apartment(|| {
        let before = stamp(hwnd, pid)?;
        let frame = Screen::new()?.chat_frame(hwnd, pid)?;
        let chat = Chat::read(&frame, &reply.recipient)?;
        ensure!(
            stamp(hwnd, pid)? == before && chat.editor_empty() && chat.outgoing(&reply.message),
            "The exact message is not visible in the approved conversation"
        );
        Ok(receipt(reply))
    })
    .map_err(|error| error.to_string())
}

fn receipt(reply: &Reply) -> String {
    format!("三局通已发送给 {}：{}", reply.recipient, reply.message)
}

fn stamp(hwnd: u64, pid: u32) -> Result<u32> {
    screen::foreground_input()
        .filter(|(window, owner, _)| *window == hwnd && *owner == pid)
        .map(|(_, _, stamp)| stamp)
        .context("The approved conversation is no longer in front")
}

fn click(hwnd: u64, expected: RECT, point: POINT) -> Result<()> {
    let hwnd = HWND(hwnd as usize as *mut c_void);
    let mut actual = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut actual)?;
        ensure!(
            actual == expected && GetAncestor(WindowFromPoint(point), GA_ROOT) == hwnd,
            "The approved chat point moved or is obstructed"
        );
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let x = point.x - GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = point.y - GetSystemMetrics(SM_YVIRTUALSCREEN);
        ensure!(
            (0..width).contains(&x) && (0..height).contains(&y),
            "Chat point is off-screen"
        );
        let mouse = |flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: ((i64::from(x) * 65_536 + 32_768) / i64::from(width)) as i32,
                    dy: ((i64::from(y) * 65_536 + 32_768) / i64::from(height)) as i32,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        send(&[
            mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
            mouse(MOUSEEVENTF_LEFTDOWN),
            mouse(MOUSEEVENTF_LEFTUP),
        ])
    }
}

fn type_text(text: &str) -> Result<()> {
    let input = text
        .encode_utf16()
        .flat_map(|w_scan| {
            [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP].map(|flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: w_scan,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            })
        })
        .collect::<Vec<_>>();
    unsafe { send(&input) }
}

unsafe fn send(input: &[INPUT]) -> Result<()> {
    ensure!(
        unsafe { SendInput(input, size_of::<INPUT>() as i32) } as usize == input.len(),
        "Windows did not accept the complete chat input"
    );
    Ok(())
}
