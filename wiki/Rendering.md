# Rendering

The rendering module implements the GPU compositing pipeline for both the
winit and udev/DRM backends. It composites the wallpaper, window decorations,
client windows, tontoo_ui surfaces, dock, menubar, and cursor in the correct
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
   - Window shadows
   - Client windows (`Space`)
   - Window border + rounded-corner mask
   - Window titlebar glass + traffic lights + title text
   - TontooUI surfaces
   - Dock glass panel + icons + running-app dots
   - Menubar glass panel + logo + clock
   - Cursor (topmost)
6. Submit the frame with damage tracking.
7. Send frame callbacks to windows and layer surfaces.
8. Refresh the space and clean up popups.

## Z-Order (Winit)

The winit backend pushes elements bottom-to-top (the renderer composites
back-to-front):

1. `Wallpaper`
2. `WindowShadow`
3. `Space` (client windows)
4. `WindowBorder`
5. `WindowTitlebar`, `WindowControls`
6. `TontooUi`
7. `DockBar`
8. `MenuBar`
9. `CursorTexture` / `CursorSurface`

## Z-Order (Udev)

The DRM compositor uses front-to-back ordering. Elements are pushed in
reverse and the cursor is inserted at index 0:

1. `CursorTexture` / `CursorSurface` (index 0, inserted last)
2. `DockBar`
3. `MenuBar`
4. `WindowBorder`
5. `WindowTitlebar`, `WindowControls`
6. `Space`
7. `TontooUi`
8. `WindowShadow`
9. `Wallpaper` (pushed last, rendered bottommost)

## Window Decorations

Constants matching macOS Tahoe:

| Constant | Value |
|---|---|
| `WINDOW_CORNER_RADIUS` | 10.0 |
| `WINDOW_SHADOW_OFFSET_Y` | 4.0 |
| `WINDOW_SHADOW_BLUR` | 20.0 |
| `WINDOW_SHADOW_BASE_ALPHA_DARK` | 0.35 |
| `WINDOW_SHADOW_BASE_ALPHA_LIGHT` | 0.20 |
| `WINDOW_BORDER_WIDTH` | 1.0 |

### create_window_shadow_texture

```rust
fn create_window_shadow_texture(
    renderer: &mut GlesRenderer,
    win_w: i32, win_h: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

Generates a Gaussian shadow around a rounded rectangle. The shadow extends
`WINDOW_SHADOW_BLUR + 4` pixels beyond the window edges.

### create_window_border_mask_texture

```rust
fn create_window_border_mask_texture(
    renderer: &mut GlesRenderer,
    win_w: i32, win_h: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

Creates a rounded-corner mask with a 1px border. The background is filled
with the clear color; the border is semi-transparent black.

### create_window_titlebar_texture

```rust
fn create_window_titlebar_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

Generates a translucent titlebar with rounded top corners and 65% milkiness.

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

### create_menubar_glass

```rust
fn create_menubar_glass(
    renderer: &mut GlesRenderer,
    w: i32, h: i32,
    blur_src: Option<(&[u8], i32, i32, i32, i32, i32, i32)>,
    is_dark: bool,
) -> Option<TextureBuffer<GlesTexture>>
```

Creates a fully transparent menubar with 3-pass box blur. The blurred
wallpaper pixels are shown as-is with no tint.

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
titles, menubar clock, and dock icon labels.

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

The TontooOS logo is loaded from one of:
- `/usr/share/icons/Tontoo_White.png`
- `/usr/share/pixmaps/Tontoo_White.png`
- `/opt/TontooOS/Tontoo_White.png`

Falls back to rendering the letter "T" when no image is found.

## Cross References

- [State.md](State.md) -- render pipeline reads from `TontooCompositor`
- [Cursor.md](Cursor.md) -- cursor element is inserted at top of z-order
- [UdevBackend.md](UdevBackend.md) -- udev backend uses the same rendering
  primitives
- [RenderCache.md](RenderCache.md) -- cached textures avoid per-frame uploads
- [Shaders.md](Shaders.md) -- blur shaders (placeholder, not integrated)
