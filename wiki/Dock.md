# Dock

The Dock is a macOS-style application launcher panel anchored to the bottom of
the screen. It renders a frosted glass background, evenly-spaced icons that
magnify under the cursor, a running-app indicator dot, and a bounce animation
when an app is launched.

## Dock

```rust
pub struct Dock {
    pub icons: Vec<DockIcon>,
    pub height: f32,
    pub visible: bool,
    pub hover_index: Option<usize>,
    pub active_app: Option<String>,
    pub animation_state: DockAnimation,
}
```

### Dock::new

```rust
pub fn new() -> Self
```

Creates an empty dock. Height defaults to `BASE_DOCK_HEIGHT` (78.0).

### Dock::add_icon

```rust
pub fn add_icon(&mut self, name: &str)
```

Pins a new application icon to the right end of the dock. Duplicate names are
silently ignored. The animation state is resized to match the new icon count.

### Dock::remove_icon

```rust
pub fn remove_icon(&mut self, name: &str) -> bool
```

Removes an icon by name. Returns `true` if the icon was found and removed.
Adjusts the hover index to remain valid.

### Dock::set_hover

```rust
pub fn set_hover(&mut self, index: Option<usize>)
```

Updates which icon the pointer is hovering over. `None` means no icon is
hovered.

### Dock::set_active_app

```rust
pub fn set_active_app(&mut self, name: &str)
```

Sets the active (focused) application. The matching icon's `is_running` flag
is set to `true`.

### Dock::clear_active_app

```rust
pub fn clear_active_app(&mut self)
```

Clears the active application. Running flags on icons are not affected.

### Dock::bounce_icon

```rust
pub fn bounce_icon(&mut self, name: &str)
```

Triggers a bounce animation (500ms, 16px amplitude, 2 cycles) on the named
icon. No-op if the icon is not found.

### Dock::icon_index

```rust
pub fn icon_index(&self, name: &str) -> Option<usize>
```

Returns the index of the icon with the given name, or `None`.

## DockAnimation

```rust
pub struct DockAnimation {
    pub magnification: Vec<f32>,
    pub target_magnification: Vec<f32>,
    pub bounce_animations: Vec<Option<Animation>>,
}
```

### Dock::tick

```rust
pub fn tick(&mut self, dt: f32)
```

Advances all bounce animations by `dt` seconds. For each icon, relaxes the
current magnification toward the target using an exponential spring decay
(`SPRING_STIFFNESS * dt`). Snaps to zero when the difference is below 0.001.

### Dock::compute_magnification

```rust
pub fn compute_magnification(&mut self, mouse_x: f32, screen_width: f32)
```

Computes the target magnification for each icon. The closest icon reaches
`MAX_MAGNIFICATION` (1.5) and neighbors follow a gaussian falloff
(`MAGNIFICATION_SIGMA = 120.0`).

## Rendering

### Dock::to_draw_commands

```rust
pub fn to_draw_commands(&self, screen_width: f32, y_offset: f32) -> Vec<DrawCommand>
```

Flattens the dock into `DrawCommand`s. An empty or hidden dock produces no
commands. The output includes:

1. Glass panel background (`GlassPanel` command).
2. Per icon: background rect, label or texture, and running-app dot.

### Dock::effective_height

```rust
pub fn effective_height(&self) -> f32
```

Returns the panel height including the tallest magnified icon, the dot gap,
and the dot diameter.

## DockIcon

```rust
pub struct DockIcon {
    pub name: String,
    pub icon_path: Option<String>,
    pub is_running: bool,
}
```

## Bounce

The bounce offset is computed as a damped sinusoidal:

```rust
-A * sin(2 * pi * cycles * t) * (1 - t)
```

where `A = BOUNCE_AMPLITUDE` (16.0), `cycles = BOUNCE_CYCLES` (2.0), and
`t` is the animation progress. Returns 0.0 when no animation is active.

## Constants

| Constant | Value |
|---|---|
| `BASE_DOCK_HEIGHT` | 78.0 |
| `ICON_GAP` | 12.0 |
| `ICON_SIZE` | 48.0 |
| `MAX_MAGNIFICATION` | 1.5 |
| `MAGNIFICATION_SIGMA` | 120.0 |
| `SPRING_STIFFNESS` | 12.0 |
| `BOUNCE_DURATION` | 500ms |
| `BOUNCE_AMPLITUDE` | 16.0 |
| `BOUNCE_CYCLES` | 2.0 |
| `DOT_RADIUS` | 3.0 |

## Cross References

- [Shell.md](Shell.md) -- `ShellState::dock` field
- [Rendering.md](Rendering.md) -- dock is rendered above windows, below the
  cursor
- [Input.md](Input.md) -- dock icon click handling
