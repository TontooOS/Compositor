use std::path::Path;

use image::GenericImageView;

pub struct Wallpaper {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}

impl Wallpaper {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let img = image::open(path.as_ref())?;
        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8().into_raw();
        tracing::info!(
            "Loaded wallpaper {:?}: {}x{} ({} bytes)",
            path.as_ref(),
            width,
            height,
            rgba.len()
        );
        Ok(Self {
            pixels: rgba,
            width: width as i32,
            height: height as i32,
        })
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }
}
