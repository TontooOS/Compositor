# WindowControls

The window controls module implements macOS-style traffic light buttons
(close, minimize, maximize) rendered in the top-left corner of each window.

GTK/Qt apps use client-side decorations and draw their own MacTahoe
header. The compositor forces `ClientSide` for every window, so the
`shell::ssd` titlebar module below is currently dormant (kept as a
fallback); it reuses the geometry and pixel helpers documented here.

## Server-Side Decorations (`shell::ssd`)

| Item | Value |
|---|---|
| `BAR_HEIGHT` | 32 |
| `TOP_STRUT` | 30.0 (reserved for the external Menubar.app) |
| Bar background Dark | `#1d1d1d` |
| Bar background Light | `#ececec` |

The bar is drawn directly above the window content. Maximized and
top-edge windows keep full content (no bar is drawn when there is no
room above the window).

### is_ssd

```rust
pub fn is_ssd(window: &Window) -> bool
```

Returns `true` when the window negotiated `ServerSide` and acked the
configure. CSD windows return `false` and are never touched.

### bar_rect

```rust
pub fn bar_rect(geo: Rectangle<i32, Logical>) -> Option<Rectangle<f32, Logical>>
```

Computes the bar rectangle above the given window geometry. Returns
`None` when the bar would overlap the top strut.

### push_ssd_elements

Pushes the bar background, traffic-light dots, hover symbols and the
centered title into the render list. Shared by the winit and udev
backends.

### Actions

| Action | Behavior |
|---|---|
| Close | `send_close` to the client |
| Maximize | Toggles the maximized state; restores the previous geometry on toggle-off |
| Minimize | Unmaps the window and pins a temporary dock icon (macOS behavior); clicking the icon restores the window |
| Bar background drag | Starts a `MoveSurfaceGrab` |

Default app configuration ships system-following defaults out of the
box: Chromium auto-selects Wayland via the skeleton
`chromium-flags.conf` and reads the GTK theme colors; VSCode follows
the OS color scheme via `window.autoDetectColorScheme` in the skeleton
`settings.json`; Firefox follows the GTK mode plus portal and gets
traffic lights from the skeleton profile at `/etc/skel/.mozilla/firefox`
(native Firefox reads `~/.mozilla`, not `~/.config`).
The system-wide theme push itself is `tontoo-theme-apply`
(`theme.service` at login, also called by Settings after a toggle):
one source (`~/.config/tontoo/theme.conf`) for gsettings, GTK
`settings.ini` files, qt5ct/qt6ct configs and the portal color-scheme.

### Known limitations

- Overlapping SSD windows: bars render above all window content, so a
  lower window bar can overlap an upper window.
- No SSD bar on maximized windows (traffic lights unreachable there).
- Restoring a minimized window whose client already exited drops the
  entry instead.

## Constants

| Constant | Value |
|---|---|
| `DOT_SIZE` | 12.0 |
| `DOT_SPACING` | 8.0 |
| `LEFT_PADDING` | 12.0 |
| `TOP_PADDING` | 14.0 |

### total_width

```rust
pub fn total_width() -> f32
```

Returns the total width of the traffic light area including both side
paddings: `LEFT_PADDING * 2 + DOT_SIZE * 3 + DOT_SPACING * 2` = 72.0.

### total_height

```rust
pub fn total_height() -> f32
```

Returns `TOP_PADDING + DOT_SIZE + 4.0` = 30.0.

## TrafficLightAction

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLightAction {
    Close,
    Minimize,
    Maximize,
}
```

## WindowControls

```rust
pub struct WindowControls {
    pub hovered: bool,
}
```

### WindowControls::new

```rust
pub fn new() -> Self
```

Creates controls with `hovered = false`.

### WindowControls::hit_test

```rust
pub fn hit_test(&self, rel_x: f32, rel_y: f32) -> Option<TrafficLightAction>
```

Tests whether a click position (relative to the window top-left) hits a
traffic light button. Returns `Some(Close)`, `Some(Minimize)`,
`Some(Maximize)`, or `None` if outside all buttons.

### WindowControls::is_in_area

```rust
pub fn is_in_area(rel_x: f32, rel_y: f32) -> bool
```

Returns `true` if the position is within the total traffic light bounding
box. Used for hover detection.

## Colors

Traffic light colors for both dark and light schemes:

| Button | Color (hex) |
|---|---|
| Close | `#FE5B51` |
| Minimize | `#E6C02A` |
| Maximize | `#51C329` |
| Inactive (all) | `#666666` (dark) / `#AAAAAA` (light) |

## Pixel Generation

### create_traffic_light_dot

```rust
pub fn create_traffic_light_dot(size: i32, color: [u8; 4]) -> Vec<u8>
```

Generates a circular dot with anti-aliased edges. Returns RGBA pixel data.
The dot is rendered at the given size and filled with the provided color.

### create_traffic_light_symbol

```rust
pub fn create_traffic_light_symbol(size: i32, symbol: char) -> Vec<u8>
```

Generates a hover symbol overlay for the dots. Supported symbols:
`x` (cross for close), `-` (line for minimize), `+` (plus for maximize).
Returns dark semi-transparent RGBA pixels on a transparent background.

## Cross References

- [Shell.md](Shell.md) -- `ShellState::window_controls` hover map
- [Rendering.md](Rendering.md) -- CSD windows draw their own header; SSD bars render above content
- [Input.md](Input.md) -- SSD bar clicks, drag grabs, dock minimize/restore
- [WaylandHandlers.md](WaylandHandlers.md) -- decoration negotiation (`ClientSide` default, honors `ServerSide`)
