# State

The `TontooCompositor` struct is the central state object. It is held by the
calloop `EventLoop` and passed mutably to every handler, backend callback, and
render function. It owns the Wayland display, the smithay `Space`, all shell
components, and the GPU render caches.

## TontooCompositor

```rust
pub struct TontooCompositor {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub layer_shell_state: WlrLayerShellState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<TontooCompositor>,
    pub data_device_state: DataDeviceState,
    pub xdg_decoration_state: XdgDecorationState,
    pub popups: PopupManager,
    pub seat: Seat<Self>,
    pub color_scheme: crate::config::ColorScheme,
    pub accessibility: AccessibilitySettings,
    pub cursor: CursorState,
    pub wallpaper: Option<Wallpaper>,
    pub wallpaper_path: PathBuf,
    pub wallpaper_buffer: Option<TextureBuffer<GlesTexture>>,
    pub texture_cache: TextureCache,
    pub widget_renderer: WidgetRenderer,
    pub animation_manager: crate::animation::AnimationManager,
    pub shell: ShellState,
    pub tontoo_ui: TontooUiState,
    pub focused_surface: Option<WlSurface>,
    pub render_cache: RenderCache,
    #[cfg(feature = "udev")]
    pub udev_data: Option<crate::udev::UdevData>,
}
```

### TontooCompositor::new

```rust
pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self
```

Creates the compositor state and all smithay protocol states. The initialization
sequence is:

1. Initialize all wayland protocol states (compositor, xdg-shell, shm,
   layer-shell, output-manager, data-device, xdg-decoration).
2. Create the `tontoo_ui_manager` global (version 1).
3. Create the seat `"seat0"` with keyboard (repeat rate 200/25) and pointer.
4. Load the color scheme from `tontoo/theme.conf`; fall back to `Dark` when the
   file is missing or unreadable.
5. Apply the color scheme to environment variables.
6. Load accessibility settings from `tontoo/accessibility.json`.
7. Load the wallpaper from the `TONTOO_WALLPAPER` env var or the default path
   `/usr/share/tontoo/wallpapers/THAOELAKE/IMAGE.png`.
8. Initialize the Wayland listening socket and register it on the event loop.
9. Create the `RenderCache` and load the system font.

The Wayland socket name is stored in `socket_name` and exported via
`WAYLAND_DISPLAY` after initialization.

> **Note:** `init_wayland_listener` uses `ListeningSocketSource::new_auto()`
> which requires `XDG_RUNTIME_DIR`. If the variable is unset (e.g. when
> started without LaunchPad), the compositor falls back to
> `dirs::runtime_dir()` or `/run/user/<uid>` (creating it with `0700`) and
> finally `/tmp/runtime-<uid>`, sets `XDG_RUNTIME_DIR`, and retries. The
> original `RuntimeDirNotSet` panic is thus avoided; see `src/state.rs:203`.

### TontooCompositor::surface_under

```rust
pub fn surface_under(
    &self,
    pos: Point<f64, Logical>,
) -> Option<(WlSurface, Point<f64, Logical>)>
```

Performs hit testing. Checks layer surfaces (Overlay, Top, Bottom, Background)
before regular windows. Returns the surface and local coordinates.

Returns `None` when the position is not above any surface.

### TontooCompositor::request_redraw

```rust
pub fn request_redraw(&mut self)
```

Triggers a render pass on all udev outputs. This is a no-op on the winit backend
(which redraws on every `WinitEvent::Redraw`).

### TontooCompositor::request_redraw_with_animation

```rust
pub fn request_redraw_with_animation(&mut self, dt: std::time::Duration)
```

Ticks the animation manager by `dt`, then triggers a redraw if animations are
still active. If no animations are running, the render pass is skipped.

### TontooCompositor::set_color_scheme

```rust
pub fn set_color_scheme(&mut self, scheme: crate::config::ColorScheme)
```

Sets the color scheme, updates the cursor theme, applies environment variables,
and saves the setting to `tontoo/theme.conf`. Returns early without error if
saving fails (logs the error).

## ClientState

```rust
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}
```

Per-client data attached to each `wl_client`. The `ClientData` trait impl is
empty -- clients are simply accepted and dropped.

## Free Functions

### get_app_id

```rust
pub fn get_app_id(window: &Window) -> Option<String>
```

Reads the XDG `app_id` from a window's `XdgToplevelSurfaceData`. Returns
`None` if the surface has no toplevel role or the lock fails.

### get_window_title

```rust
pub fn get_window_title(window: &Window) -> Option<String>
```

Reads the XDG `title` from a window's `XdgToplevelSurfaceData`. Returns
`None` on failure.

## Cross References

- [Configuration.md](Configuration.md) -- color scheme loading and saving
- [Input.md](Input.md) -- `request_redraw` is called from input handlers
- [Rendering.md](Rendering.md) -- consumes `TontooCompositor` for GPU output
- [Shell.md](Shell.md) -- `ShellState` is stored as `self.shell`
