# UdevBackend

The udev backend provides hardware-accelerated compositing via DRM/KMS,
libseat session management, and libinput. It is used when running the
compositor directly on a TTY.

## UdevData

```rust
pub struct UdevData {
    pub session: LibSeatSession,
    pub devices: HashMap<DrmNode, DeviceData>,
}
```

## DeviceData

```rust
pub struct DeviceData {
    pub drm: DrmDevice,
    pub drm_fd: DrmDeviceFd,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub gles: GlesRenderer,
    pub renderer_formats: Vec<Format>,
    pub surfaces: HashMap<crtc::Handle, SurfaceData>,
    pub known_connectors: HashMap<connector::Handle, crtc::Handle>,
    pub render_node: DrmNode,
    pub registration_token: Option<RegistrationToken>,
}
```

## SurfaceData

```rust
pub struct SurfaceData {
    pub dh: DisplayHandle,
    pub compositor: TontooDrmCompositor,
    pub output: Output,
    pub frame_pending: bool,
}
```

## Initialization

### init_udev

```rust
pub fn init_udev(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>>
```

Initialization sequence:

1. Create a `LibSeatSession` and register the session event source (handles
   `ActivateSession` and `PauseSession` events).
2. Create a `LibinputInputBackend` via `LibinputSessionInterface`.
3. Create a `UdevBackend` and scan existing DRM devices.
4. Register the udev event source (handles `Added`, `Changed`, `Removed`
   device events).
5. Trigger an initial render on all surfaces.

### add_node

```rust
fn add_node(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
    device_id: libc::dev_t,
    path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>>
```

Opens a DRM device, creates GBM/EGL/GLesRenderer, scans connectors, and
registers the DRM event source for VBlank handling.

### scan_connectors

Iterates all DRM connectors on the device. For newly connected connectors,
finds an available CRTC, creates an output and DRM surface, and maps the
output. For disconnected connectors, unmaps the output.

### find_crtc

Finds an available CRTC for a connector by iterating its encoder's possible
CRTCs and excluding those already in use.

### create_output_for_connector

```rust
fn create_output_for_connector(
    display_handle: &DisplayHandle,
    connector: &connector::Info,
) -> Result<Output, Box<dyn std::error::Error>>
```

Creates a Wayland output from a DRM connector. The output name is
`"{interface}-{interface_id}"`.

## Rendering

### try_render_all

```rust
pub fn try_render_all(state: &mut TontooCompositor)
```

Ticks the dock spring physics and triggers a render pass on all udev
surfaces that do not have a pending frame.

### render_surface

```rust
fn render_surface(
    surface: &mut SurfaceData,
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    clear_color: [f32; 4],
    wallpaper: Option<&Wallpaper>,
    cursor: Option<&mut CursorState>,
    pointer_pos: Option<Point<f64, Logical>>,
    wallpaper_buffer: &mut Option<TextureBuffer<GlesTexture>>,
    active_app: &Option<String>,
    render_cache: &mut RenderCache,
    tontoo_ui: &TontooUiState,
    focused_surface: Option<&WlSurface>,
    window_controls: &HashMap<String, WindowControls>,
) -> Result<(), SwapBuffersError>
```

The udev render path. Builds the render element list (same z-order as the
winit backend but in reverse due to DRM front-to-back compositing), renders
the frame, and sends frame callbacks.

## VBlank Handling

The DRM event source handles `VBlank(crtc)` events. On VBlank, it marks the
surface's `frame_pending` as false and triggers a re-render on all surfaces.
The compositor naturally re-renders on every frame request.

## Session Switching

`Ctrl+Alt+F1..F12` switches to the corresponding VT. On session activation
(VT switch back), all `frame_pending` flags are reset and a render is
triggered.

## Cross References

- [State.md](State.md) -- `TontooCompositor::udev_data` field
- [Rendering.md](Rendering.md) -- shared rendering primitives
- [Input.md](Input.md) -- `Ctrl+Alt+F*` VT switching
