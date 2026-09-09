# Rendering

The rendering module implements the GPU compositing pipeline for both the
winit and udev/DRM backends. It composites the wallpaper, window decorations,
client windows, tontoo_ui surfaces, dock, and cursor in the correct
z-order.

## Winit Backend

### init_winit

```rust
pub fn init_winit(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>>
```

Creates a winit window (60Hz), registers an output, and sets up the render
loop. The render loop handles four event types:

1. `Resized` -- updates the output mode.
2. `Input` -- forwards to `process_input_event`.
3. `Redraw` -- performs the full compositing pass.
4. `CloseRequested` -- stops the event loop.

## Render Pipeline (Winit)

On each `Redraw` event:

1. Tick the animation manager.
2. Tick the dock spring physics.
3. Compute dock magnification.
4. Bind the framebuffer.
5. Build the render element list in z-order:
   - Wallpaper (bottommost)
   - Background/Bottom layer-shell surfaces
   - Window shadows (improved 3-layer shadow)
   - Client windows (`Space`) — CSD: windows include their own header bar
   - Window border + rounded-corner mask
   - TontooUI surfaces
   - Dock glass panel + icons + running-app dots
   - Top/Overlay layer-shell surfaces (e.g. `Menubar.app`)
   - Cursor (topmost)
6. Submit the frame with damage tracking.
7. Send frame callbacks to windows and layer surfaces.
8. Refresh the space and clean up popups.

> **Note:** Server-side titlebar / traffic lights have been removed. The
> compositor now uses Client-Side Decorations (CSD): each app draws its own
> decoration bar. See [WaylandHandlers.md](WaylandHandlers.md).

## Z-Order (Winit)

The winit backend pushes elements bottom-to-top (the renderer composites
back-to-front):

1. `Wallpaper`
2. Background/Bottom layer-shell surfaces
3. `WindowShadow` (3-layer shadow with vertical bias)
3. `Space` (client windows, CSD)
4. `WindowBorder`
5. `TontooUi`
6. `DockBar`
7. Top/Overlay layer-shell surfaces (e.g. `Menubar.app`)
8. `CursorTexture` / `CursorSurface`

> **Note:** The top bar is not rendered here. It is the external
> `Menubar.app` system app (see [Menubar.md](Menubar.md)); the compositor
> only reserves the top strut and windows are placed below it.

## Z-Order (Udev)

The DRM compositor uses front-to-back ordering. Elements are pushed in
reverse and the cursor is inserted at index 0:

1. `CursorTexture` / `CursorSurface` (index 0, inserted last)
2. Top/Overlay layer-shell surfaces (e.g. `Menubar.app`)
3. `DockBar`
3. `WindowBorder`
4. `Space` (CSD)
5. `TontooUi`
6. `WindowShadow`
7. Background/Bottom layer-shell surfaces
8. `Wallpaper` (pushed last, rendered bottommost)

## Window Decorations

The compositor now uses **Client-Side Decorations (CSD)**. Only the shadow
and border are drawn by the compositor; the titlebar/traffic lights are
drawn by each client. Constants match the improved multi-layer macOS Tahoe
shadow:

| Constant | Value |
|---|---|
| `WINDOW_CORNER_RADIUS` | 10.0 |
| `WINDOW_SHADOW_OFFSET_Y` | 12.0 |
| `WINDOW_SHADOW_BLUR` | 60.0 (max far layer) |
| `WINDOW_SHADOW_BASE_ALPHA_DARK` | 0.38 |
| `WINDOW_SHADOW_BASE_ALPHA_LIGHT` | 0.22 |
| `WINDOW_BORDER_WIDTH` | 0.7 |
| `TITLEBAR_HEIGHT` | 32 (kept for backward compat, not rendered) |

### create_window_shadow_texture

```rust
pub fn create_window_shadow_texture(
    renderer: &mut GlesRenderer,
    win_w: i32, win_h: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

Generates a high-quality 3-layer Gaussian shadow around a rounded rectangle:

- **tight** (blur 14) — contact/umbra
- **medium** (blur 30) — main penumbra
- **far** (blur 60) — soft ambient diffuse

Layers are weighted `0.50 / 0.32 / 0.18` and multiplied by
`WINDOW_SHADOW_BASE_ALPHA_*`. A vertical bias makes the shadow ~18%
stronger at the bottom than at the top, matching macOS's key-light model.
The texture extends 64 px beyond the window on all sides (`pad = 64`) and
is vertically offset by `WINDOW_SHADOW_OFFSET_Y` (12 px).

### create_window_border_mask_texture

```rust
pub fn create_window_border_mask_texture(
    renderer: &mut GlesRenderer,
    win_w: i32, win_h: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

Creates a rounded-corner mask with a 0.7 px border. The background is filled
with the clear color (`#1d1d1d` dark / `#ececec` light); the border is
semi-transparent black.

### create_window_titlebar_texture

```rust
pub fn create_window_titlebar_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

> **Deprecated / unused.** The function is kept for backward compatibility
> but is no longer called by the render pipeline. Apps must draw their own
> titlebar via CSD. The texture previously generated a solid `#ececec`
> titlebar with rounded top corners.

## Glass Effects

### create_glass_texture

```rust
fn create_glass_texture(
    renderer: &mut GlesRenderer,
    w: i32, h: i32, cr: i32,
    blur_src: Option<(&[u8], i32, i32, i32, i32, i32, i32)>,
    tex_scale: i32,
) -> Option<TextureBuffer<GlesTexture>>
```

Creates a glass panel with 2-pass box blur over the wallpaper, a 30% white
tint overlay, anti-aliased rounded corners, and a 2px white border. The
border alpha is 0.63 (160/255).

When no wallpaper is provided, a solid semi-transparent white is used.

### box_blur_5x5

```rust
fn box_blur_5x5(
    src: &[u8], dest: &mut [u8],
    dw: u32, dh: u32, sw: u32, sh: u32,
    off_x: i32, off_y: i32,
    sx: f64, sy: f64, fill_scale: f64,
)
```

5x5 box blur kernel. Maps destination pixels to source pixels when
source and destination have different sizes. Used for the glass blur.

### signed_dist_rounded

```rust
fn signed_dist_rounded(x: f64, y: f64, w: f64, h: f64, r: f64) -> f64
```

Signed distance to a rounded rectangle. Negative = inside, positive = outside.

## Text Rendering

### render_text_texture

```rust
fn render_text_texture(
    renderer: &mut GlesRenderer,
    text: &str,
    font_size: f32,
    color: [u8; 4],
    font: Option<&fontdue::Font>,
) -> Option<TextureBuffer<GlesTexture>>
```

Rasterizes text using `fontdue`, returns a GPU texture. Used for window
titles on the winit backend.

## Dock Rendering

The dock renders at 2x resolution for HiDPI clarity. Icons use hardcoded
color mappings:

| App | Color (ABGR) |
|---|---|
| Finder | `0xFF2196F3` |
| Terminal | `0xFF2979FF` |
| Settings | `0xFF9E9E9E` |
| Notes | `0xFF4CAF50` |
| Podcasts | `0xFF9C27B0` |
| Unknown | `0xFF607D8B` |

## Cross References

- [State.md](State.md) -- render pipeline reads from `TontooCompositor`
- [Cursor.md](Cursor.md) -- cursor element is inserted at top of z-order
- [UdevBackend.md](UdevBackend.md) -- udev backend uses the same rendering
  primitives
- [RenderCache.md](RenderCache.md) -- cached textures avoid per-frame uploads
- [Shaders.md](Shaders.md) -- blur shaders (placeholder, not integrated)
