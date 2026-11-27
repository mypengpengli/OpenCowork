use anyhow::{ensure, Context, Result};
use std::ffi::c_void;
use windows::{
    core::{w, HSTRING},
    Globalization::Language,
    Graphics::Imaging::{BitmapEncoder, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
    Win32::{
        Foundation::{
            CloseHandle, HANDLE, HWND, RECT, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Graphics::Gdi::*,
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            StationsAndDesktops::*,
            Threading::{
                CreateMutexW, OpenProcess, QueryFullProcessImageNameW, ReleaseMutex,
                WaitForSingleObject, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            HiDpi::*,
            Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
            WindowsAndMessaging::*,
        },
    },
};

pub struct ObserverLock(HANDLE);

impl ObserverLock {
    pub fn acquire() -> Result<Self> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\OpenCoworkAS.Observation"))?;
            match WaitForSingleObject(handle, 0) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
                _ => {
                    let _ = CloseHandle(handle);
                    anyhow::bail!("Another OpenCowork instance is observing this desktop");
                }
            }
        }
    }
}

impl Drop for ObserverLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct Screen {
    engine: OcrEngine,
    language: String,
    max_size: i32,
    surface: Option<Surface>,
    previous: Vec<u8>,
    previous_window: Vec<u8>,
    source: Option<(isize, String)>,
    retained_crop: bool,
    input_stamp: Option<u32>,
}

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

pub fn parent_is_running(pid: u32) -> bool {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
    unsafe {
        OpenProcess(PROCESS_SYNCHRONIZE, false, pid).is_ok_and(|handle| {
            let result = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
            let _ = CloseHandle(handle);
            result
        })
    }
}

pub fn in_apartment<T>(run: impl FnOnce() -> Result<T>) -> Result<T> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let _apartment = Apartment;
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    run()
}

impl Screen {
    pub fn new() -> Result<Self> {
        let zh = Language::CreateLanguage(&HSTRING::from("zh-Hans"))?;
        let engine = if OcrEngine::IsLanguageSupported(&zh)? {
            OcrEngine::TryCreateFromLanguage(&zh)?
        } else {
            OcrEngine::TryCreateFromUserProfileLanguages()
                .context("No Windows OCR language is available")?
        };
        Ok(Self {
            language: engine.RecognizerLanguage()?.LanguageTag()?.to_string(),
            engine,
            max_size: OcrEngine::MaxImageDimension()? as i32,
            surface: None,
            previous: Vec::new(),
            previous_window: Vec::new(),
            source: None,
            retained_crop: false,
            input_stamp: None,
        })
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn clear(&mut self) {
        self.surface = None;
        self.source = None;
        self.previous.clear();
        self.previous_window.clear();
        self.input_stamp = None;
    }

    pub(super) fn chat_frame(&self, hwnd: u64, pid: u32) -> Result<ChatFrame> {
        let hwnd = HWND(hwnd as usize as *mut c_void);
        let mut bounds = RECT::default();
        let mut owner = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            ensure!(
                owner == pid && process_app(pid).eq_ignore_ascii_case("wxworklocal.exe"),
                "The active window is no longer Sanjutong"
            );
            GetWindowRect(hwnd, &mut bounds)?;
            ensure!(
                GetForegroundWindow() == hwnd && unobstructed(hwnd, bounds),
                "The chat window is obstructed or no longer in front"
            );
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            ensure!(
                width >= 460 && height >= 390 && width <= self.max_size && height <= self.max_size,
                "Open the chat at a size the OCR can read"
            );
            let mut surface = Surface::new(width, height)?;
            let pixels = surface.capture(bounds.left, bounds.top)?;
            let mut current = RECT::default();
            GetWindowRect(hwnd, &mut current)?;
            ensure!(
                current == bounds && GetForegroundWindow() == hwnd && unobstructed(hwnd, bounds),
                "The chat window moved during capture"
            );
            let writer = DataWriter::new()?;
            writer.WriteBytes(pixels)?;
            let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
                &writer.DetachBuffer()?,
                BitmapPixelFormat::Bgra8,
                width,
                height,
            )?;
            let mut lines = Vec::new();
            for line in self.engine.RecognizeAsync(&bitmap)?.join()?.Lines()? {
                let mut rect = RECT {
                    left: width,
                    top: height,
                    right: 0,
                    bottom: 0,
                };
                for word in line.Words()? {
                    let word = word.BoundingRect()?;
                    rect.left = rect.left.min(word.X.floor() as i32);
                    rect.top = rect.top.min(word.Y.floor() as i32);
                    rect.right = rect.right.max((word.X + word.Width).ceil() as i32);
                    rect.bottom = rect.bottom.max((word.Y + word.Height).ceil() as i32);
                }
                if rect.right > rect.left && rect.bottom > rect.top {
                    lines.push((line.Text()?.to_string(), rect));
                }
            }
            Ok(ChatFrame {
                bounds,
                lines,
                pixels: pixels.to_vec(),
            })
        }
    }

    pub fn sample(
        &mut self,
        desktop_pid: u32,
        settings: &super::decision::Settings,
    ) -> Result<Sample> {
        unsafe {
            if !unlocked_desktop() {
                self.clear();
                return Ok(Sample::Locked);
            }
            let input_stamp = foreground_input().map(|(_, _, stamp)| stamp);
            let hwnd = GetForegroundWindow();
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if hwnd.is_invalid() || pid == 0 || pid == desktop_pid || pid == std::process::id() {
                self.clear();
                return Ok(Sample::Skipped);
            }
            let app = process_app(pid);
            let policy = settings.app_policy(&app);
            if policy == super::decision::AppPolicy::Ignore {
                self.clear();
                return Ok(Sample::Excluded);
            }
            let retain_crop = settings.use_images && policy == super::decision::AppPolicy::Allow;
            let mut name = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut name).max(0) as usize;
            let source = (hwnd.0 as isize, String::from_utf16_lossy(&name[..len]));
            let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            ensure!(
                width > 0 && height > 0 && i64::from(width) * i64::from(height) <= 40_000_000,
                "Desktop capture size is out of range"
            );

            if self
                .surface
                .as_ref()
                .is_none_or(|s| s.width != width || s.height != height)
            {
                self.clear();
                self.surface = Some(Surface::new(width, height)?);
            }
            let surface = self.surface.as_mut().unwrap();
            let scoped = settings
                .permits_text(&app)
                .then(|| client_bounds(hwnd))
                .flatten()
                .filter(|bounds| unobstructed(hwnd, *bounds));
            let pixels = surface.capture(left, top)?;
            // Ignore cursor blinking and very small changes on a steady screen.
            let stamp = signature(pixels, width as usize, height as usize);
            if self.source.as_ref() == Some(&source)
                && self.retained_crop == retain_crop
                && !changed(&self.previous, &stamp)
            {
                return Ok(Sample::Unchanged);
            }
            let text = recognize(
                &self.engine,
                pixels,
                width,
                self.max_size,
                0,
                0,
                width,
                height,
            )?;
            if !unlocked_desktop() {
                self.clear();
                return Ok(Sample::Locked);
            }
            let all = Region {
                x: 0,
                y: 0,
                width: width as usize,
                height: height as usize,
            };
            let preview = thumbnail(pixels, width as usize, all, 960)?;
            let region = scoped
                .filter(|bounds| {
                    GetForegroundWindow() == hwnd
                        && client_bounds(hwnd) == Some(*bounds)
                        && unobstructed(hwnd, *bounds)
                })
                .and_then(|bounds| {
                    let x = (bounds.left - left).clamp(0, width);
                    let y = (bounds.top - top).clamp(0, height);
                    let right = (bounds.right - left).clamp(x, width);
                    let bottom = (bounds.bottom - top).clamp(y, height);
                    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
                });
            let window_text = region
                .map(|(x, y, w, h)| {
                    if w == width && h == height {
                        Ok(text.clone())
                    } else {
                        recognize(&self.engine, pixels, width, self.max_size, x, y, w, h)
                    }
                })
                .transpose()?;
            let window_sample = region
                .filter(|_| retain_crop)
                .map(|(x, y, w, h)| {
                    picture_signature(
                        pixels,
                        width as usize,
                        x as usize,
                        y as usize,
                        w as usize,
                        h as usize,
                    )
                })
                .unwrap_or_default();
            let visual_change =
                !window_sample.is_empty() && changed(&self.previous_window, &window_sample);
            let crop = if retain_crop {
                region
                    .map(|(x, y, w, h)| {
                        thumbnail(
                            pixels,
                            width as usize,
                            Region {
                                x: x as usize,
                                y: y as usize,
                                width: w as usize,
                                height: h as usize,
                            },
                            1440,
                        )
                    })
                    .transpose()?
            } else {
                None
            };
            self.previous = stamp;
            self.previous_window = window_sample;
            let event = if self.source.as_ref() != Some(&source) {
                super::ObservationEvent::WindowSwitch
            } else if input_stamp.is_some() && input_stamp != self.input_stamp {
                super::ObservationEvent::AfterInput
            } else {
                super::ObservationEvent::ScreenChange
            };
            self.input_stamp = input_stamp;
            self.source = Some(source.clone());
            self.retained_crop = retain_crop;
            Ok(Sample::Observed {
                window: source.1,
                hwnd: hwnd.0 as u64,
                pid,
                app,
                text,
                window_text,
                visual_change,
                event,
                preview,
                crop,
            })
        }
    }
}

pub(super) struct ChatFrame {
    pub bounds: RECT,
    pub lines: Vec<(String, RECT)>,
    pixels: Vec<u8>,
}

impl ChatFrame {
    /// Locate a long horizontal separator between the conversation and editor.
    pub(super) fn composer_top(&self, left: i32, send: RECT) -> Option<i32> {
        let width = self.bounds.right - self.bounds.left;
        let button_height = send.bottom - send.top;
        let from = (send.top - 18 * button_height).max((self.bounds.bottom - self.bounds.top) / 2);
        let to = send.top - 3 * button_height;
        let xs = (left + 24..send.left - 16).step_by(8).collect::<Vec<_>>();
        if xs.len() < 15 {
            return None;
        }
        let color = |x: i32, y: i32| {
            let i = ((y * width + x) * 4) as usize;
            &self.pixels[i..i + 3]
        };
        let changed = |a: &[u8], b: &[u8]| a.iter().zip(b).any(|(a, b)| a.abs_diff(*b) >= 8);
        (from..=to).rev().find(|&y| {
            let base = color(xs[0], y);
            xs.iter()
                .filter(|&&x| {
                    let at = color(x, y);
                    !changed(at, base)
                        && (changed(at, color(x, y - 3)) || changed(at, color(x, y + 3)))
                })
                .count()
                * 10
                >= xs.len() * 9
        })
    }
}

pub enum Sample {
    Locked,
    Skipped,
    Excluded,
    Unchanged,
    Observed {
        window: String,
        hwnd: u64,
        pid: u32,
        app: String,
        text: String,
        window_text: Option<String>,
        visual_change: bool,
        event: super::ObservationEvent,
        preview: Vec<u8>,
        crop: Option<Vec<u8>>,
    },
}

unsafe fn client_bounds(hwnd: HWND) -> Option<RECT> {
    use windows::Win32::Foundation::POINT;
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect).ok()? };
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(hwnd, &mut origin).as_bool() } {
        return None;
    }
    Some(RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + rect.right,
        bottom: origin.y + rect.bottom,
    })
}

unsafe fn unobstructed(hwnd: HWND, bounds: RECT) -> bool {
    let mut above = hwnd;
    while let Ok(next) = unsafe { GetWindow(above, GW_HWNDPREV) } {
        above = next;
        if !unsafe { IsWindowVisible(above).as_bool() } {
            continue;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(above, &mut rect) }.is_err() {
            return false;
        }
        if rect.left < bounds.right
            && rect.right > bounds.left
            && rect.top < bounds.bottom
            && rect.bottom > bounds.top
        {
            return false;
        }
    }
    true
}

pub(super) fn foreground_input() -> Option<(u64, u32, u32)> {
    unsafe {
        if !unlocked_desktop() {
            return None;
        }
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut input = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        (!hwnd.is_invalid() && pid != 0 && GetLastInputInfo(&mut input).as_bool()).then_some((
            hwnd.0 as u64,
            pid,
            input.dwTime,
        ))
    }
}

fn process_app(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buffer = [0u16; 2048];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(h);
        if result.is_err() {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..size as usize])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
}

unsafe fn unlocked_desktop() -> bool {
    let Ok(desk) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) else {
        return false;
    };
    let mut name = [0u16; 32];
    let result = GetUserObjectInformationW(
        HANDLE(desk.0),
        UOI_NAME,
        Some(name.as_mut_ptr().cast()),
        (name.len() * 2) as u32,
        None,
    );
    let _ = CloseDesktop(desk);
    result.is_ok() && name.starts_with(&[68, 101, 102, 97, 117, 108, 116, 0])
}

struct Surface {
    screen: HDC,
    buffer: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    pixels: *mut c_void,
    width: i32,
    height: i32,
}

impl Surface {
    unsafe fn new(width: i32, height: i32) -> Result<Self> {
        let mut surface = Self {
            screen: GetDC(None),
            buffer: HDC::default(),
            bitmap: HBITMAP::default(),
            previous_bitmap: HGDIOBJ::default(),
            pixels: std::ptr::null_mut(),
            width,
            height,
        };
        ensure!(!surface.screen.is_invalid(), "Could not access desktop");
        surface.buffer = CreateCompatibleDC(Some(surface.screen));
        ensure!(
            !surface.buffer.is_invalid(),
            "Could not allocate capture buffer"
        );
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        surface.bitmap = CreateDIBSection(
            Some(surface.screen),
            &info,
            DIB_RGB_COLORS,
            &mut surface.pixels,
            None,
            0,
        )?;
        surface.previous_bitmap = SelectObject(surface.buffer, surface.bitmap.into());
        ensure!(
            !surface.previous_bitmap.is_invalid() && !surface.pixels.is_null(),
            "Invalid capture buffer"
        );
        Ok(surface)
    }

    unsafe fn capture(&mut self, x: i32, y: i32) -> Result<&[u8]> {
        BitBlt(
            self.buffer,
            0,
            0,
            self.width,
            self.height,
            Some(self.screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        )?;
        ensure!(GdiFlush().as_bool(), "Could not finish desktop capture");
        Ok(std::slice::from_raw_parts(
            self.pixels.cast(),
            self.width as usize * self.height as usize * 4,
        ))
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous_bitmap.is_invalid() {
                SelectObject(self.buffer, self.previous_bitmap);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.buffer.is_invalid() {
                let _ = DeleteDC(self.buffer);
            }
            if !self.screen.is_invalid() {
                ReleaseDC(None, self.screen);
            }
        }
    }
}

fn recognize(
    engine: &OcrEngine,
    pixels: &[u8],
    width: i32,
    max: i32,
    left: i32,
    top: i32,
    region_width: i32,
    region_height: i32,
) -> Result<String> {
    let mut results = Vec::new();
    for y in (top..top + region_height).step_by(max as usize) {
        for x in (left..left + region_width).step_by(max as usize) {
            let rows = (top + region_height - y).min(max);
            let cols = (left + region_width - x).min(max);
            let writer = DataWriter::new()?;
            for row in y..y + rows {
                let start = ((row * width + x) * 4) as usize;
                writer.WriteBytes(&pixels[start..start + cols as usize * 4])?;
            }
            let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
                &writer.DetachBuffer()?,
                BitmapPixelFormat::Bgra8,
                cols,
                rows,
            )?;
            let text = engine.RecognizeAsync(&bitmap)?.join()?.Text()?.to_string();
            results.push(text);
        }
    }
    Ok(results.join("\n"))
}

fn signature(pixels: &[u8], width: usize, height: usize) -> Vec<u8> {
    picture_signature(pixels, width, 0, 0, width, height)
}

fn picture_signature(
    pixels: &[u8],
    stride: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
) -> Vec<u8> {
    (top..top + height)
        .step_by(5)
        .flat_map(|y| {
            (left..left + width).step_by(5).map(move |x| {
                let i = (y * stride + x) * 4;
                ((u16::from(pixels[i]) + 2 * u16::from(pixels[i + 1]) + u16::from(pixels[i + 2]))
                    / 4) as u8
            })
        })
        .collect()
}

fn changed(previous: &[u8], next: &[u8]) -> bool {
    previous.len() != next.len()
        || previous
            .iter()
            .zip(next)
            .filter(|(a, b)| a.abs_diff(**b) > 22)
            .take(32)
            .count()
            == 32
}

struct Region {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

fn thumbnail(pixels: &[u8], stride: usize, area: Region, limit: usize) -> Result<Vec<u8>> {
    let divisor = area.width.max(area.height).max(limit);
    let columns = (area.width * limit / divisor).max(1);
    let rows = (area.height * limit / divisor).max(1);
    let mut resized = Vec::with_capacity(columns * rows * 4);
    for row in 0..rows {
        for col in 0..columns {
            let x = area.x + col * area.width / columns;
            let y = area.y + row * area.height / rows;
            let i = (y * stride + x) * 4;
            resized.extend_from_slice(&[pixels[i], pixels[i + 1], pixels[i + 2], 255]);
        }
    }
    let writer = DataWriter::new()?;
    writer.WriteBytes(&resized)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &writer.DetachBuffer()?,
        BitmapPixelFormat::Bgra8,
        columns as i32,
        rows as i32,
    )?;
    let stream = InMemoryRandomAccessStream::new()?;
    let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, &stream)?.join()?;
    encoder.SetSoftwareBitmap(&bitmap)?;
    encoder.FlushAsync()?.join()?;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    let count = reader.LoadAsync(u32::try_from(stream.Size()?)?)?.join()?;
    let mut png = vec![0; count as usize];
    reader.ReadBytes(&mut png)?;
    Ok(png)
}
