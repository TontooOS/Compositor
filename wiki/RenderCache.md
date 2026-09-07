# RenderCache

The render cache stores pre-generated GPU textures for compositor elements
(dock, window decorations) so they do not need to be recomputed
every frame.

## TexBuf

```rust
pub type TexBuf = smithay::backend::renderer::element::texture::TextureBuffer<GlesTexture>;
```

Type alias for the GPU texture buffer.

## RenderCache

```rust
pub struct RenderCache {
    pub dock_panel: Option<(i32, i32, ColorScheme, TexBuf)>,
    pub dock_icons: HashMap<(String, ColorScheme, i32), TexBuf>,
    pub window_shadows: HashMap<(i32, i32, ColorScheme), TexBuf>, // improved 3-layer shadow
    pub window_borders: HashMap<(i32, i32, ColorScheme), TexBuf>,
    pub window_titlebars: HashMap<(i32, i32, ColorScheme), TexBuf>, // legacy, unused with CSD
    pub traffic_light_dots: HashMap<(String, i32, i32, ColorScheme), TexBuf>, // legacy, unused
    pub traffic_light_symbols: HashMap<(char, i32, ColorScheme), TexBuf>, // legacy, unused
    pub font: Option<fontdue::Font>,
}
```

> **Note:** `window_titlebars`, `traffic_light_dots`, and
> `traffic_light_symbols` are kept for backward compatibility but are no
> longer populated by the render pipeline (CSD).

### RenderCache::new

```rust
pub fn new() -> Self
```

Creates an empty cache with all fields set to `None` or empty.

### RenderCache::load_font

```rust
pub fn load_font(&mut self) -> Option<&fontdue::Font>
```

Loads a system font from well-known paths. Search order:

1. `/usr/share/fonts/OTF/SF-Pro-Display-Regular.otf`
2. `/usr/share/fonts/OTF/SF-Pro-Text-Regular.otf`
3. `/usr/share/fonts/TTF/SF-Pro.ttf`
4. `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`
5. `/usr/share/fonts/TTF/DejaVuSans.ttf`

Returns `None` when no font can be loaded. Logs a warning.

### RenderCache::invalidate

```rust
pub fn invalidate(&mut self)
```

Clears all cached textures and the font. Used when the color scheme changes
or the window size changes.

## Cache Key Conventions

| Cache | Key | Notes |
|---|---|---|
| `dock_panel` | `(width, height, ColorScheme)` | |
| `dock_icons` | `(name, ColorScheme, size)` | |
| `window_shadows` | `(win_w, win_h, ColorScheme)` | 3-layer shadow |
| `window_borders` | `(win_w, win_h, ColorScheme)` | |
| `window_titlebars` | `(width, height, ColorScheme)` | legacy, unused (CSD) |
| `traffic_light_dots` | `(color_name, size, scale, ColorScheme)` | legacy, unused (CSD) |
| `traffic_light_symbols` | `(symbol_char, size, ColorScheme)` | legacy, unused (CSD) |

## Cross References

- [State.md](State.md) -- `TontooCompositor::render_cache` field
- [Rendering.md](Rendering.md) -- the render pipeline reads and populates
  this cache
- [UdevBackend.md](UdevBackend.md) -- the udev render path uses the same
  cache
