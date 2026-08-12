# Menubar

A macOS-style top menu bar rendered as a translucent glass panel spanning the
full screen width. The left side shows the TontooOS logo icon and the active
application name. The right side shows system tray icons (Wi-Fi, battery,
clock). A TontooOS dropdown menu is available via the OS menu button.

## Menubar

```rust
pub struct Menubar {
    pub height: f32,
    pub visible: bool,
    pub os_menu_active: bool,
    pub app_name: String,
    pub show_clock: bool,
    pub show_wifi: bool,
    pub show_battery: bool,
}
```

Default height is 28.0 logical pixels. `show_clock`, `show_wifi`, and
`show_battery` are all `true` by default.

### Menubar::new

```rust
pub fn new() -> Self
```

Creates the menubar with sensible defaults. `app_name` starts as
`"TontooOS"`.

### Menubar::set_app_name

```rust
pub fn set_app_name(&mut self, name: &str)
```

Updates the application name shown next to the OS logo.

### Menubar::toggle_os_menu

```rust
pub fn toggle_os_menu(&mut self)
```

Toggles the TontooOS dropdown menu open/closed.

## Rendering

### Menubar::to_draw_commands

```rust
pub fn to_draw_commands(
    &self,
    screen_width: f32,
    color_scheme: ColorScheme,
) -> Vec<DrawCommand>
```

Returns an empty vec when `visible` is `false`.

The dark theme uses zero milkiness (near-transparent glass) with white text.
The light theme uses 0.3 milkiness (milky glass) with dark text.

The draw commands include:

1. Glass panel background.
2. Left side: TontooOS logo rect + "T" label, app name, "TontooOS" menu
   button (highlighted when `os_menu_active`).
3. Right side: clock (HH:MM), battery icon, Wi-Fi icon.
4. When the OS menu is active: a dropdown menu with items including About,
   System Preferences, App Store, Force Quit, Sleep, Restart, Shut Down,
   and Lock Screen. Destructive items are rendered in red.

## MenubarItem

```rust
pub struct MenubarItem {
    pub label: String,
    pub icon: Option<String>,
}
```

A helper type for defining individual menu bar entries. Currently used
structurally but not directly consumed by the render loop.

## Time

The clock displays UTC time in HH:MM format. This is derived from
`SystemTime::now() - UNIX_EPOCH` using modular arithmetic. The time is not
converted to the local timezone.

## Text Width Estimation

Text width is estimated as `text.len() * font_size * 0.55`. This is a rough
approximation sufficient for positioning.

## Cross References

- [Shell.md](Shell.md) -- `ShellState::menubar` field
- [Rendering.md](Rendering.md) -- menubar is rendered above windows, below
  the cursor
- [Input.md](Input.md) -- `update_traffic_light_hover` and
  `update_tontoo_ui_hover` are called on pointer motion
