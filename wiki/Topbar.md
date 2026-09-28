# Topbar

A simple glass top bar widget. It is not rendered by the compositor
render pipeline; the system top bar is the external `Menubar.app`
(see [Menubar.md](Menubar.md)).

## Topbar

```rust
pub struct Topbar {
    pub visible: bool,
    pub height: f32,
}
```

Default height is 28.0 logical pixels.

### Topbar::new

```rust
pub fn new() -> Self
```

Creates a visible topbar with the default height.

## Rendering

### Topbar::to_draw_commands

```rust
pub fn to_draw_commands(
    &self,
    screen_width: f32,
    color_scheme: ColorScheme,
) -> Vec<DrawCommand>
```

Returns an empty vec when `visible` is `false`.

Produces a single `GlassPanel` draw command with zero corner radius and
0.8 alpha. Dark theme uses zero milkiness, light theme uses 0.3 milkiness.

## Cross References

- [Shell.md](Shell.md) -- `ShellState::topbar` field
- [Menubar.md](Menubar.md) -- external `Menubar.app` system app
- [Rendering.md](Rendering.md) -- topbar is rendered as a glass panel
