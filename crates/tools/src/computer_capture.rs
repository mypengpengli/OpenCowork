//! One-frame Windows Graphics Capture worker. The caller owns this process and
//! bounds its lifetime, including a driver that never produces a frame.
use std::path::PathBuf;
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    encoder::ImageFormat,
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

struct OneFrame(PathBuf);
impl GraphicsCaptureApiHandler for OneFrame {
    type Flags = PathBuf;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self(context.flags))
    }
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if frame.width() == 0
            || frame.height() == 0
            || u64::from(frame.width()) * u64::from(frame.height()) > 40_000_000
        {
            return Err("Invalid capture dimensions".into());
        }
        frame.save_as_image(&self.0, ImageFormat::Jpeg)?;
        println!(
            "{}",
            serde_json::json!({"width":frame.width(),"height":frame.height(),"method":"Windows.Graphics.Capture"})
        );
        control.stop();
        Ok(())
    }
}

pub fn worker() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    let handle = args
        .get(2)
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n != 0)
        .ok_or("Missing window handle")?;
    let path = PathBuf::from(args.get(3).ok_or("Missing output path")?);
    let window = Window::from_raw_hwnd(handle as *mut std::ffi::c_void);
    let settings = Settings::new(
        window,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Rgba8,
        path,
    );
    OneFrame::start(settings).map_err(|e| format!("Windows Graphics Capture: {e}"))
}
