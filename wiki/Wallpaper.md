# Wallpaper

The wallpaper module loads a PNG or JPEG image at startup and stores the raw
RGBA pixel data for use by the render pipeline.

## Wallpaper

```rust
pub struct Wallpaper {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}
```

### Wallpaper::load

```rust
pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>>
```

Opens the image at `path` using the `image` crate, converts it to RGBA, and
stores the pixel data. Logs the path, dimensions, and byte count on success.
Returns `Err` when the file cannot be opened or decoded.

### Wallpaper::pixels

```rust
pub fn pixels(&self) -> &[u8]
```

Returns a reference to the raw RGBA pixel buffer.

### Wallpaper::size

```rust
pub fn size(&self) -> (i32, i32)
```

Returns `(width, height)` in pixels.

## Loading

The wallpaper path is determined at startup:

1. If the `TONTOO_WALLPAPER` environment variable is set, that path is used.
2. Otherwise the default path is
   `/System/User/Wallpapers/THAOELAKE/IMAGE.png`.

> **Note:** the legacy path `/usr/share/tontoo/wallpapers/THAOELAKE/IMAGE.png`
> resolves to the same file through a compatibility symlink kept by the ISO
> build (`BaseOS/scripts/stage-wallpapers.sh`).

If loading fails, the compositor logs a warning and uses the clear color as the
background.

## Runtime Switching

`TontooCompositor::set_wallpaper` starts a macOS-like 450ms crossfade to
a new image file: the old wallpaper renders underneath at full alpha,
the incoming one on top with the eased (smoothstep) progress alpha,
then it takes over (GPU buffers swapped, no re-upload). Failures leave
the current wallpaper untouched. Both backends drive frames until the
fade finishes (udev render timer, winit redraw requests).

```rust
pub fn set_wallpaper(&mut self, path: &Path) -> Result<(), String>
pub fn wallpaper_fade_alpha(&self, now: Instant) -> Option<f32>
pub fn finish_wallpaper_fade_if_done(&mut self, now: Instant)
```

Set remotely via [SettingsIpc.md](SettingsIpc.md) (`set_wallpaper`).

## Usage

The render pipeline uploads the wallpaper to a GPU `TextureBuffer` once, then
reuses it on every frame. The wallpaper is scaled to fill the output using a
"cover" strategy (scale up to fill, center the overflow).

## Cross References

- [State.md](State.md) -- `TontooCompositor::wallpaper` and `wallpaper_path` fields
- [Rendering.md](Rendering.md) -- wallpaper is the bottommost render layer
- [SettingsIpc.md](SettingsIpc.md) -- `set_wallpaper` remote op
