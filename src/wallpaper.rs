use std::path::Path;

use image::GenericImageView;

pub struct Wallpaper {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}

/// Maximum texture dimension for the wallpaper. virtio-gpu/virgl and GLES2
/// implementations reject or silently misrender very large single textures
/// (a 6016x3384 RGBA wallpaper is ~81 MB), which previously produced a black
/// screen. Downscale to a safe size at load time.
const MAX_DIM: u32 = 4096;

impl Wallpaper {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let img = image::open(path.as_ref())?;
        let (orig_w, orig_h) = img.dimensions();
        let max_side = orig_w.max(orig_h);
        let img = if max_side > MAX_DIM {
            let scale = MAX_DIM as f32 / max_side as f32;
            let nw = (orig_w as f32 * scale).round().max(1.0) as u32;
            let nh = (orig_h as f32 * scale).round().max(1.0) as u32;
            tracing::info!(
                "Downscaling wallpaper {}x{} -> {}x{} (GL texture safety)",
                orig_w,
                orig_h,
                nw,
                nh
            );
            img.resize(nw, nh, image::imageops::FilterType::Triangle)
        } else {
            img
        };
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
