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

/// Fill modes honored by the render pipeline.
pub const FILL_MODES: &[&str] = &["fill", "fit", "stretch", "center", "tile"];
/// Fill mode used at boot and for unknown values.
pub const DEFAULT_FILL: &str = "fill";

/// True for the five known fill modes.
pub fn valid_fill(fill: &str) -> bool {
    FILL_MODES.contains(&fill)
}

/// One wallpaper quad: destination offset/size in output pixels.
/// The texture source is always the full image (tiles repeat it).
pub struct WallpaperQuad {
    pub offset: (f64, f64),
    pub size: (i32, i32),
}

/// Destination quads for a wallpaper on an output. Unknown or degenerate
/// inputs fall back to a single `fill` quad (or none for empty sizes).
pub fn wallpaper_layout(wp_w: i32, wp_h: i32, out_w: i32, out_h: i32, fill: &str) -> Vec<WallpaperQuad> {
    if wp_w <= 0 || wp_h <= 0 || out_w <= 0 || out_h <= 0 {
        return Vec::new();
    }
    let (ww, wh, ow, oh) = (wp_w as f64, wp_h as f64, out_w as f64, out_h as f64);
    match fill {
        "fit" => {
            let scale = (ow / ww).min(oh / wh);
            let dw = (ww * scale).round() as i32;
            let dh = (wh * scale).round() as i32;
            vec![WallpaperQuad {
                offset: ((ow - dw as f64) / 2.0, (oh - dh as f64) / 2.0),
                size: (dw, dh),
            }]
        }
        "stretch" => vec![WallpaperQuad {
            offset: (0.0, 0.0),
            size: (out_w, out_h),
        }],
        "center" => vec![WallpaperQuad {
            offset: ((ow - ww) / 2.0, (oh - wh) / 2.0),
            size: (wp_w, wp_h),
        }],
        "tile" => {
            let nx = (ow / ww).ceil().max(1.0) as i32;
            let ny = (oh / wh).ceil().max(1.0) as i32;
            let mut quads = Vec::with_capacity((nx * ny) as usize);
            for y in 0..ny {
                for x in 0..nx {
                    quads.push(WallpaperQuad {
                        offset: (x as f64 * ww, y as f64 * wh),
                        size: (wp_w, wp_h),
                    });
                }
            }
            quads
        }
        _ => {
            // "fill" and anything unknown: cover the output, center overflow.
            let scale = (ow / ww).max(oh / wh);
            let dw = (ww * scale).round() as i32;
            let dh = (wh * scale).round() as i32;
            vec![WallpaperQuad {
                offset: ((ow - dw as f64) / 2.0, (oh - dh as f64) / 2.0),
                size: (dw, dh),
            }]
        }
    }
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

    #[test]
    fn fill_modes_validate() {
        assert_eq!(FILL_MODES, &["fill", "fit", "stretch", "center", "tile"]);
        assert!(valid_fill("tile"));
        assert!(!valid_fill("melt"));
        assert!(!valid_fill(""));
    }

    #[test]
    fn layout_covers_and_fits() {
        // 16:9 image on a 16:9 output: cover and fit agree.
        let cover = wallpaper_layout(1920, 1080, 1920, 1080, "fill");
        assert_eq!(cover.len(), 1);
        assert_eq!(cover[0].size, (1920, 1080));
        assert_eq!(cover[0].offset, (0.0, 0.0));
        // Portrait image covered on landscape output: overflow centered.
        let cover = wallpaper_layout(1080, 1920, 1920, 1080, "fill");
        assert_eq!(cover.len(), 1);
        assert!(cover[0].size.0 >= 1920 && cover[0].size.1 >= 1080);
        // Fit letterboxes instead.
        let fit = wallpaper_layout(1080, 1920, 1920, 1080, "fit");
        assert_eq!(fit.len(), 1);
        assert!(fit[0].size.0 <= 1920 && fit[0].size.1 <= 1080);
        assert_eq!(fit[0].offset.1, 0.0);
    }

    #[test]
    fn layout_stretch_center_tile() {
        let stretch = wallpaper_layout(800, 600, 1920, 1080, "stretch");
        assert_eq!(stretch.len(), 1);
        assert_eq!(stretch[0].size, (1920, 1080));
        assert_eq!(stretch[0].offset, (0.0, 0.0));
        let center = wallpaper_layout(800, 600, 1920, 1080, "center");
        assert_eq!(center.len(), 1);
        assert_eq!(center[0].size, (800, 600));
        assert_eq!(center[0].offset, (560.0, 240.0));
        let tiles = wallpaper_layout(800, 600, 1920, 1080, "tile");
        assert_eq!(tiles.len(), 3 * 2);
        assert_eq!(tiles[0].offset, (0.0, 0.0));
        assert_eq!(tiles[5].offset, (1600.0, 600.0));
        // Unknown modes and degenerate inputs stay safe.
        assert_eq!(wallpaper_layout(800, 600, 1920, 1080, "melt").len(), 1);
        assert!(wallpaper_layout(0, 600, 1920, 1080, "fill").is_empty());
        assert!(wallpaper_layout(800, 600, 0, 1080, "fill").is_empty());
    }
}
