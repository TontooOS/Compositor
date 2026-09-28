# Cursor

The cursor module handles XCursor theme loading, cursor rendering with speed
based magnification, and defines the render element types used by the compositor.

## CursorState

```rust
pub struct CursorState {
    pub surface: Option<WlSurface>,
    pub visible: bool,
    // ... internal fields omitted
}
```

### CursorState::new

```rust
pub fn new(scheme: ColorScheme) -> Self
```

Creates the cursor state with the given color scheme. Loads the XCursor theme
corresponding to the scheme. The initial active cursor is `"left_ptr"`.

### CursorState::set_color_scheme

```rust
pub fn set_color_scheme(&mut self, scheme: ColorScheme)
```

Switches the XCursor theme and clears all cached texture buffers.

### CursorState::reset_visibility

```rust
pub fn reset_visibility(&mut self)
```

Sets `visible` to `true` and restores `active_named` to `"left_ptr"` if it was
`None`.

### CursorState::update_speed

```rust
pub fn update_speed(&mut self, new_pos: Point<f64, Logical>)
```

Computes pointer speed and applies macOS-style magnification. When speed exceeds
`SPEED_THRESHOLD` (800 px/s), the target scale increases linearly up to
`MAX_SCALE` (2.5). After the pointer slows down, the scale restores after
`RESTORE_DELAY_MS` (300 ms) via an exponential lerp (`SCALE_LERP = 0.15`).

### CursorState::handle_cursor_image

```rust
pub fn handle_cursor_image(&mut self, image: CursorImageStatus)
```

Handles the three `CursorImageStatus` variants:

- `Hidden`: hides the cursor, clears surface.
- `Surface(surface)`: stores the surface, clears `active_named`.
- `Named(icon)`: stores the cursor name, clears surface.

### CursorState::get_cursor_element

```rust
pub fn get_cursor_element(
    &mut self,
    renderer: &mut GlesRenderer,
    pointer_pos: Point<f64, Logical>,
) -> Option<CursorRenderElement>
```

Returns the cursor render element. Priority order:

1. Client-provided cursor surface.
2. Named cursor from the XCursor theme (with `TextureBuffer` caching).
3. Built-in 32x32 fallback arrow.

Returns `None` when `visible` is `false`.

The magnification scale is applied to both the cursor size and hotspot.

## CursorRenderElement

```rust
pub enum CursorRenderElement {
    Surface(WaylandSurfaceRenderElement<GlesRenderer>),
    Texture(TextureRenderElement<GlesTexture>),
}
```

## XCursorLoader

Internal loader that resolves XCursor theme names to files on disk.

Searches `XDG_DATA_HOME`, `~/.local/share/icons`, `~/.icons`, `XDG_DATA_DIRS`,
and `/usr/share/icons` for the theme directory. If the primary theme is missing,
it falls back through `FALLBACK_THEMES` in order:

`breeze_cursors`, `Breeze_Snow`, `Adwaita`, `DMZ-White`, `Vanilla-DMZ`,
`default`.

Cursor name aliases are expanded (e.g. `"left_ptr"` also searches `"default"`,
`"arrow"`).

## Constants

| Constant | Value | Description |
|---|---|---|
| `FALLBACK_CURSOR_WIDTH` | 32 | Fallback cursor width (pixels) |
| `FALLBACK_CURSOR_HEIGHT` | 32 | Fallback cursor height (pixels) |
| `SPEED_THRESHOLD` | 800.0 | Speed at which magnification begins |
| `SPEED_MAX` | 2000.0 | Speed at which max magnification is reached |
| `MAX_SCALE` | 2.5 | Maximum cursor magnification |
| `RESTORE_DELAY_MS` | 300 | Delay before restoring scale |
| `SCALE_LERP` | 0.15 | Lerp factor for scale interpolation |

## Render Element Wrappers

The module defines typed wrappers around `TextureRenderElement<GlesTexture>`
for the compositor render element enum `TontooRenderElements`:

`WallpaperElement`, `CursorTextureElement`,
`WindowShadowElement`, `WindowBorderElement`,
`WindowControlsElement`, `WindowTitlebarElement`, `TontooUiTextureElement`.

## Cross References

- [State.md](State.md) -- `TontooCompositor::cursor` field
- [Input.md](Input.md) -- `update_speed` is called on pointer motion
- [Rendering.md](Rendering.md) -- cursor element is inserted at index 0
