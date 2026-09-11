use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use image::GenericImageView;

pub struct Wallpaper {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}

/// Crossfade duration for runtime wallpaper switches (macOS-like).
pub const FADE_DURATION: Duration = Duration::from_millis(450);

/// Incoming wallpaper during a crossfade: rendered on top of the current
/// wallpaper with the eased alpha until it takes over.
pub struct WallpaperFade {
    pub next: Wallpaper,
    pub path: PathBuf,
    pub start: Instant,
}

/// Eased crossfade alpha in `[0.0, 1.0]` (smoothstep).
pub fn fade_eased(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Raw crossfade progress in `[0.0, 1.0]` from elapsed time.
pub fn fade_progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / FADE_DURATION.as_secs_f32()).min(1.0)
}

/// Parse a `set_wallpaper` request: `{"op": "set_wallpaper", "path": "..."}`.
pub fn parse_set_wallpaper_path(request: &serde_json::Value) -> Result<PathBuf, String> {
    request
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "missing path".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_eased_clamps_and_eases() {
        assert_eq!(fade_eased(-1.0), 0.0);
        assert_eq!(fade_eased(0.0), 0.0);
        assert_eq!(fade_eased(1.0), 1.0);
        assert_eq!(fade_eased(2.0), 1.0);
        assert_eq!(fade_eased(0.5), 0.5);
        // Smoothstep: slow start, fast middle.
        assert!(fade_eased(0.25) < 0.25);
        assert!(fade_eased(0.75) > 0.75);
    }

    #[test]
    fn fade_progress_clamps_at_one() {
        assert_eq!(fade_progress(Duration::ZERO), 0.0);
        assert_eq!(fade_progress(FADE_DURATION), 1.0);
        assert_eq!(fade_progress(FADE_DURATION * 3), 1.0);
    }

    #[test]
    fn parse_set_wallpaper_path_validates() {
        let ok = serde_json::json!({"op": "set_wallpaper", "path": "/a/b.png"});
        assert_eq!(
            parse_set_wallpaper_path(&ok).unwrap(),
            PathBuf::from("/a/b.png")
        );
        assert!(parse_set_wallpaper_path(&serde_json::json!({"op": "set_wallpaper"})).is_err());
        assert!(parse_set_wallpaper_path(&serde_json::json!({"path": ""})).is_err());
        assert!(parse_set_wallpaper_path(&serde_json::json!({})).is_err());
    }
}
