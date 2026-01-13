use image::{DynamicImage, ImageOutputFormat};
use screenshots::Screen;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;
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
    pub fn image_to_base64(image: &DynamicImage, quality: u8) -> Result<String, String> {
        let mut buffer = Cursor::new(Vec::new());

        // 压缩为 JPEG
        let jpeg = image.to_rgb8();
        let quality = clamp_jpeg_quality(quality);
        jpeg.write_to(&mut buffer, ImageOutputFormat::Jpeg(quality))
            .map_err(|e| format!("图片编码失败: {}", e))?;

        Ok(BASE64.encode(buffer.into_inner()))
    }

    /// 保存截图到文件
    pub fn save_to_file(image: &DynamicImage, path: &str, quality: u8) -> Result<(), String> {
        let ext = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "jpg" || ext == "jpeg" {
            let quality = clamp_jpeg_quality(quality);
            let mut file = File::create(path).map_err(|e| format!("保存截图失败: {}", e))?;
            image
                .write_to(&mut file, ImageOutputFormat::Jpeg(quality))
                .map_err(|e| format!("保存截图失败: {}", e))
        } else {
            image
                .save(path)
                .map_err(|e| format!("保存截图失败: {}", e))
        }
    }
}

fn clamp_jpeg_quality(quality: u8) -> u8 {
    if quality == 0 {
        1
    } else if quality > 100 {
        100
    } else {
        quality
    }
}
