use image::{DynamicImage, ImageFormat};
use screenshots::Screen;
use std::io::Cursor;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub struct ScreenCapture;

impl ScreenCapture {
    /// 截取主屏幕
    pub fn capture_primary() -> Result<DynamicImage, String> {
        let screens = Screen::all().map_err(|e| format!("获取屏幕失败: {}", e))?;

        let primary = screens
            .into_iter()
            .next()
            .ok_or_else(|| "没有找到屏幕".to_string())?;

        let image = primary
            .capture()
            .map_err(|e| format!("截屏失败: {}", e))?;

        let width = image.width();
        let height = image.height();
        let rgba = image.into_raw();

        image::RgbaImage::from_raw(width, height, rgba)
            .map(DynamicImage::ImageRgba8)
            .ok_or_else(|| "图像转换失败".to_string())
    }

    /// 将图片转换为 Base64
    pub fn image_to_base64(image: &DynamicImage, _quality: u8) -> Result<String, String> {
        let mut buffer = Cursor::new(Vec::new());

        // 压缩为 JPEG
        let jpeg = image.to_rgb8();
        jpeg.write_to(&mut buffer, ImageFormat::Jpeg)
            .map_err(|e| format!("图片编码失败: {}", e))?;

        Ok(BASE64.encode(buffer.into_inner()))
    }

    /// 保存截图到文件
    pub fn save_to_file(image: &DynamicImage, path: &str, _quality: u8) -> Result<(), String> {
        image
            .save(path)
            .map_err(|e| format!("保存截图失败: {}", e))
    }
}
