# Rendering

The rendering module implements the GPU compositing pipeline for both the
winit and udev/DRM backends. It composites the wallpaper, window decorations,
client windows, tontoo_ui surfaces, layer-shell system apps, and cursor in
the correct z-order.

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
2. Bind the framebuffer.
3. Build the render element list in z-order:
   - Wallpaper (bottommost)
   - Background/Bottom layer-shell surfaces
   - Window shadows (improved 3-layer shadow)
   - Client windows (`Space`) — CSD: windows include their own header bar
   - Window border + rounded-corner mask
   - TontooUI surfaces
   - Top/Overlay layer-shell surfaces (e.g. `Menubar.app`, `Dock.app`)
   - Cursor (topmost)
4. Submit the frame with damage tracking.
5. Send frame callbacks to windows and layer surfaces.
6. Refresh the space and clean up popups.

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
6. Top/Overlay layer-shell surfaces (e.g. `Menubar.app`, `Dock.app`)
7. `CursorTexture` / `CursorSurface`

> **Note:** Neither the top bar nor the dock is rendered here. They are
> the external `Menubar.app` / `Dock.app` system apps (see
> [Menubar.md](Menubar.md) / [Dock.md](Dock.md)); the compositor only
> reserves the top strut and windows are placed below it.

## Z-Order (Udev)

The DRM compositor uses front-to-back ordering. Elements are pushed in
reverse and the cursor is inserted at index 0:

1. `CursorTexture` / `CursorSurface` (index 0, inserted last)
2. Top/Overlay layer-shell surfaces (e.g. `Menubar.app`, `Dock.app`)
3. `WindowBorder`
4. `Space` (CSD)
5. `TontooUi`
6. `WindowShadow`
7. Background/Bottom layer-shell surfaces
8. `Wallpaper` (pushed last, rendered bottommost)

## Window Decorations

The compositor uses **Client-Side Decorations (CSD)**. Neither a shadow nor a
border is drawn any more: apps and the GTK theme own their own frame, and the
only geometry left in this module is the corner radius kept for the
compositor side decoration helper.

| Constant | Value |
|---|---|
| `WINDOW_CORNER_RADIUS` | 12.0 |
| `WINDOW_SHADOW_OFFSET_Y` | 6.0 |
| `WINDOW_SHADOW_BLUR` | 25.0 |
| `WINDOW_SHADOW_BASE_ALPHA_DARK` | 0.20 |
| `WINDOW_SHADOW_BASE_ALPHA_LIGHT` | 0.12 |
| `WINDOW_BORDER_WIDTH` | 0.5 |

### create_window_shadow_texture

```rust
pub fn create_window_shadow_texture(
    renderer: &mut GlesRenderer,
    win_w: i32, win_h: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

> **Unused.** The function is kept for reference but has no caller: the
> shadow block in the winit pipeline was removed, and the udev backend
> already delegates shadows to the GTK theme.

Generates a single-layer Gaussian shadow around a rounded rectangle. The
texture extends `WINDOW_SHADOW_BLUR + 4` px beyond the window on all sides
and is vertically offset by `WINDOW_SHADOW_OFFSET_Y`.

### create_window_border_mask_texture

```rust
pub fn create_window_border_mask_texture(
    renderer: &mut GlesRenderer,
    win_w: i32, win_h: i32,
    color_scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>>
```

Creates a rounded-corner mask with a 0.5 px border. The background is filled
with the clear color (`#1d1d1d` dark / `#ececec` light); the border is
semi-transparent black.

> **Unused.** The border block in the winit pipeline was removed; the GTK
> theme draws the frame for GTK apps and TontooUI apps draw their own.

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

> **Removed.** `create_glass_texture` and `box_blur_5x5` were deleted
> with the internal dock (their only caller). The external `Dock.app`
> draws its own glass panel.

The compositor does not blur. Widget-tree clients still get the flat
translucent `GlassPanel` quad from [WidgetRenderer.md](WidgetRenderer.md), and
client rendered apps (TontooUI) receive the desktop backdrop through the
stream documented in [Backdrop.md](Backdrop.md) and blur it themselves.

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

> **Removed.** The compositor draws no dock. The bottom dock is the
> external `Dock.app` system app, composited as a layer-shell surface
> like `Menubar.app`. See [Dock.md](Dock.md).

## Cross References

- [State.md](State.md) -- render pipeline reads from `TontooCompositor`
- [Cursor.md](Cursor.md) -- cursor element is inserted at top of z-order
- [UdevBackend.md](UdevBackend.md) -- udev backend uses the same rendering
  primitives
- [RenderCache.md](RenderCache.md) -- cached textures avoid per-frame uploads
- [Backdrop.md](Backdrop.md) -- offscreen capture and readback used for the
  client side blur
