use crate::DesktopEvent;
use tao::{event_loop::EventLoopProxy, platform::windows::WindowExtWindows, window::Window};
use windows::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};

#[derive(serde::Deserialize)]
pub struct Notice {
    id: String,
    title: String,
    text: String,
}

pub fn watch_notices(url: &str, proxy: EventLoopProxy<DesktopEvent>) {
    let url = format!("{url}api/assistant/notice");
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .build()
        else {
            return;
        };
        let mut displayed = std::collections::VecDeque::<String>::new();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let notice = client
                .get(&url)
                .send()
                .and_then(|response| response.error_for_status())
                .and_then(|response| response.json::<Option<Notice>>());
            if let Ok(Some(notice)) = notice {
                if displayed.contains(&notice.id) {
                    continue;
                }
                displayed.push_back(notice.id.clone());
                if displayed.len() > 48 {
                    displayed.pop_front();
                }
                if proxy.send_event(DesktopEvent::Notice(notice)).is_err() {
                    break;
                }
            }
        }
    });
}

const TRAY_MESSAGE: u32 = WM_APP + 17;
const TRAY_ID: usize = 17;

pub struct Tray {
    data: NOTIFYICONDATAW,
    callbacks: Box<Callbacks>,
}

struct Callbacks {
    proxy: EventLoopProxy<DesktopEvent>,
    taskbar_created: u32,
}

impl Tray {
    pub fn new(window: &Window, proxy: EventLoopProxy<DesktopEvent>) -> anyhow::Result<Self> {
        let hwnd = HWND(window.hwnd() as _);
        let mut tray = Self {
            data: unsafe { icon_data(hwnd)? },
            callbacks: Box::new(Callbacks {
                proxy,
                taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
            }),
        };
        unsafe {
            anyhow::ensure!(
                SetWindowSubclass(
                    hwnd,
                    Some(on_message),
                    TRAY_ID,
                    (&mut *tray.callbacks as *mut Callbacks) as usize,
                )
                .as_bool(),
                "could not register tray callback"
            );
            anyhow::ensure!(
                Shell_NotifyIconW(NIM_ADD, &tray.data).as_bool(),
                "could not add tray icon"
            );
        }
        Ok(tray)
    }

    pub fn notify(&self, notice: Notice) {
        let mut data = NOTIFYICONDATAW {
            cbSize: self.data.cbSize,
            hWnd: self.data.hWnd,
            uID: self.data.uID,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        put_text(&mut data.szInfoTitle, &notice.title);
        put_text(&mut data.szInfo, &notice.text);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}

fn put_text(buffer: &mut [u16], text: &str) {
    let limit = buffer.len().saturating_sub(1);
    for (slot, unit) in buffer
        .iter_mut()
        .zip(text.replace('\0', " ").encode_utf16().take(limit))
    {
        *slot = unit;
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
            let _ = RemoveWindowSubclass(self.data.hWnd, Some(on_message), TRAY_ID);
        }
    }
}

unsafe fn icon_data(hwnd: HWND) -> windows::core::Result<NOTIFYICONDATAW> {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID as u32,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: LoadIconW(None, IDI_APPLICATION)?,
        ..Default::default()
    };
    let title: Vec<_> = "OpenCowork AS".encode_utf16().collect();
    data.szTip[..title.len()].copy_from_slice(&title);
    Ok(data)
}

unsafe extern "system" fn on_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    let callbacks = &*(data as *const Callbacks);
    if message == TRAY_MESSAGE && wparam.0 == TRAY_ID {
        let action = match lparam.0 as u32 {
            WM_LBUTTONUP => Some(DesktopEvent::Show),
            NIN_BALLOONUSERCLICK => Some(DesktopEvent::ShowAssistant),
            WM_RBUTTONUP => tray_menu(hwnd).ok().flatten(),
            _ => None,
        };
        if let Some(action) = action {
            let _ = callbacks.proxy.send_event(action);
        }
        return LRESULT(0);
    }
    if callbacks.taskbar_created != 0 && message == callbacks.taskbar_created {
        if let Ok(data) = icon_data(hwnd) {
            let _ = Shell_NotifyIconW(NIM_ADD, &data);
        }
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

unsafe fn tray_menu(hwnd: HWND) -> windows::core::Result<Option<DesktopEvent>> {
    let menu = CreatePopupMenu()?;
    let result = (|| {
        AppendMenuW(menu, MF_STRING, 1, w!("打开 OpenCowork AS"))?;
        AppendMenuW(menu, MF_STRING, 2, w!("暂停屏幕观察"))?;
        AppendMenuW(menu, MF_STRING, 3, w!("退出"))?;
        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor)?;
        let _ = SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY,
            cursor.x,
            cursor.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        Ok(match selected.0 {
            1 => Some(DesktopEvent::Show),
            2 => Some(DesktopEvent::Pause),
            3 => Some(DesktopEvent::Quit),
            _ => None,
        })
    })();
    let _ = DestroyMenu(menu);
    result
}
