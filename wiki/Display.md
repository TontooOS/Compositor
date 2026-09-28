# Display

Display settings owned by the compositor: output modes, brightness and
night light. Served over the settings socket (`get_displays`,
`set_display`); the Settings daemon persists the values.

## Outputs

`list_displays` returns every mapped output with its connector modes
(resolution plus refresh rate in Hz) and its current mode. On the udev
backend the modes come from the DRM connector; on winit the mode is
fixed by the host window.

```rust
pub struct DisplayMode {
  pub width: i32,
  pub height: i32,
  pub refresh: u32,
}
```

```rust
pub struct DisplayInfo {
  pub name: String,
  pub modes: Vec<DisplayMode>,
  pub current: Option<DisplayMode>,
}
```

## Refresh Switching

`apply_display` switches the refresh rate live when a mode with the
current output size exists: the DRM surface is recreated with the new
mode while the space geometry and all windows stay valid (resolution
never changes here). Resolution changes are rejected (they need a
reboot); on winit only the running mode applies.

```rust
pub fn apply_display(
  state: &mut TontooCompositor,
  request: &SetDisplay,
) -> Result<(String, Option<DisplayMode>, u32, bool), String>;
```

- Resolves the output by name (first output by default, error when
  unknown or absent).
- Applies brightness (0-100, clamped to a factor) and night light
  immediately, then the optional mode switch.
- Returns the effective output name, mode, brightness percent and night
  light flag, and requests a redraw.

## Brightness and Night Light

Software overlays rendered above all content just below the cursor
(`overlay_elements`, both backends): a black fullscreen quad dimming by
`1 - brightness`, plus a warm quad (`1.0, 0.55, 0.25` at `0.30` alpha)
for night light. They work everywhere, including VMs without a
backlight device. Inactive overlays render nothing.

```rust
pub fn overlay_elements(
  renderer: &mut GlesRenderer,
  width: f32,
  height: f32,
  brightness: f32,
  night_light: bool,
) -> Vec<TextureRenderElement<GlesTexture>>;
```

## Cross References

- [SettingsIpc.md](SettingsIpc.md) -- `get_displays`/`set_display` ops
- [Rendering.md](Rendering.md) -- overlay layer order
- [State.md](State.md) -- `display_brightness`/`display_night_light`
