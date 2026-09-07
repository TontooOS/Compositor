# WindowControls

The window controls module implements macOS-style traffic light buttons
(close, minimize, maximize) rendered in the top-left corner of each window.

> **Note (CSD):** Since the compositor now uses Client-Side Decorations,
> the compositor no longer renders traffic lights or a titlebar itself.
> `WindowControls` is kept as a **helper library for apps** — apps that want
> a native macOS look can reuse `DOT_SIZE`, `close_color`, and the pixel
> generation helpers to draw their own header. The compositor's `Input`
> and `Rendering` pipelines no longer reference it directly.

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

- [Shell.md](Shell.md) -- `ShellState::window_controls` map (kept, unused by compositor)
- [Rendering.md](Rendering.md) -- previously rendered in the titlebar area; now CSD (apps render)
- [Input.md](Input.md) -- previous traffic light click/hover; now handled by clients
- [WaylandHandlers.md](WaylandHandlers.md) -- decoration now `ClientSide`
