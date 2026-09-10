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
output. For disconnected connectors, unmaps the output. The mode for both
the Wayland output and the DRM surface comes from `pick_connector_mode`
so they are always consistent.

### pick_connector_mode

```rust
fn pick_connector_mode(modes: &[drm_crate::control::Mode]) -> Option<drm_crate::control::Mode>
```

Returns the native screen mode:

- first choice is the mode flagged `PREFERRED` by the display (EDID native
  mode), which is what the screen actually has;
- if several modes carry `PREFERRED`, the one with the highest refresh wins,
  ties break toward the smaller area (avoids 4K duplicates on VMs);
- fallback is the first advertised mode;
- returns `None` when the list is empty.

The compositor never picks the largest mode by area. Virtualized drivers
report huge 4K+ modes that do not fit the screen and produced an oversized
framebuffer.

### log_connector_modes

```rust
fn log_connector_modes(connector: &connector::Info)
```

Logs every advertised mode as `WxH @ Hz` with a `(PREFERRED)` marker,
followed by the chosen mode. Use this log to verify the compositor runs
at the native display resolution.

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
`"{interface}-{interface_id}"`. The preferred Wayland mode is the same
native mode returned by `pick_connector_mode`, so `set_preferred` always
matches the mode used for the DRM surface.

## Oversized framebuffer (mode larger than the screen)

Symptom: the desktop renders at 4K or larger on a smaller screen, content
does not fit.

Root cause: `scan_connectors` picked the largest advertised mode by pixel
area while `create_output_for_connector` advertised `PREFERRED`, so the
DRM surface and the Wayland output disagreed. Virtualized GPUs advertise
modes far above the visible screen size.

Fix (`src/udev.rs`): both paths use `pick_connector_mode` (EDID
`PREFERRED` first). Verify via the `Connector mode` / `Chosen mode`
lines in the compositor log.

## Render Pump

The udev backend presents frames with `DrmCompositor::commit_frame`
(synchronous, no VBlank required). A 16 ms calloop timer polls every tick
but only renders when something actually needs a frame:

- `state.pending_redraw` is set by input events, Wayland commits
  (`CompositorHandler::commit`, layer-shell, `tontoo_ui` updates) and
  `set_color_scheme`
- `state.animation_manager.has_active()` is true while a registered
  animation runs

An idle desktop therefore performs zero DRM atomic commits. The previous
unconditional 60 fps pump saturated the VirtualBox vmwgfx SVGA FIFO
(28k+ atomic commits, `Adding connector: Virtual-1` every ~20 ms) and
starved serial, sshd and input.

### try_render_all

```rust
pub fn try_render_all(state: &mut TontooCompositor)
```

Clears `pending_redraw` and renders all surfaces.

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

## Troubleshooting

### Black screen on boot (no output, SSH still works)

Symptom: the VM display stays black, `launchctl list` shows `compositor`
as `stopped`, `/var/log/launchpad/compositor.log` is empty, and the seatd
log contains:

```bash
Could not make device fd drm master: Device or resource busy
```

Root cause: `plymouthd` still holds the DRM master, so the compositor
cannot modeset (`scan_connectors` fails with `Permission denied (os
error 13)` on `/dev/dri/card0` and no output is ever mapped). Two boot
integration defects combined to produce this:

- `BaseOS/archiso/airootfs/usr/local/bin/start-compositor.sh` was not
  executable inside the ISO (`profiledef.sh` `file_permissions` had no
  entry for it, and NTFS/WSL hosts lose the `+x` bit), so LaunchPad
  could only start it via the `/bin/sh` fallback.
- The starter ran `plymouth deactivate/quit` as the unprivileged
  `liveuser` without `sudo`, which fails silently, and only waited ~3s
  with a broken `fuser ... | grep -qE 'plymouth|206'` check.

Fix (boot integration, no compositor code change required):

- Add `0755` `file_permissions` entries in
  `BaseOS/archiso/profiledef.sh` for `start-compositor.sh`,
  `tontoo-sshd.sh` and `tontoo-net-up.sh`.
- `start-compositor.sh` quits plymouth via `sudo` (liveuser has
  `NOPASSWD`), force-kills `plymouthd`/`@lymouthd`, waits up to 10s for
  the process to disappear, and logs progress to stderr (visible in
  `/var/log/launchpad/compositor.log`).

Verify on the running VM via SSH:

```bash
sudo cat /var/log/launchpad/seatd.log
sudo launchctl list
ls -l /usr/local/bin/start-compositor.sh
ps aux | grep -E 'plymouth|compositor' | grep -v grep
```

### Typing in clients stalls for tens of seconds (flush bug)

Symptom: the mouse cursor is reactive, clicks work, but typed text in
Wayland clients (e.g. `foot`) appears only after ~30s, often in bursts.

Root cause: the udev backend never called `DisplayHandle::flush_clients`
after queueing server-to-client events. Upstream `wayland-server` docs
require regular flushing (`display.rs`: "you also need to regularly
invoke `flush_clients()`, which will write the outgoing buffers into
the sockets"). The only flush lived in the display-fd dispatch callback,
which fires solely on client-to-server traffic. Key presses, button and
motion events plus `send_frame` callbacks therefore sat in userspace
buffers while idle clients waited for them — a circular stall broken
only by unrelated client traffic. The winit backend already flushed
after `send_frame` (`render.rs`); the udev backend did not.

Fix (`src/udev.rs`):

- flush in the libinput callback after `process_input_event`, so input
  events reach clients immediately;
- flush in the 16ms render timer after `try_render_all`, so frame
  callbacks reach clients immediately and they commit their next frame.

Verify: inject key events (e.g. via `VBoxManage controlvm <vm>
keyboardputscancode`), then count presented frames — each keypress must
produce a `commit_frame` (`Adding connector: VGA` in
`/var/log/launchpad/compositor.log`) within milliseconds, and typed
text must be visible in the next screenshot.

## Cross References

- [State.md](State.md) -- `TontooCompositor::udev_data` field
- [Rendering.md](Rendering.md) -- shared rendering primitives
- [Input.md](Input.md) -- `Ctrl+Alt+F*` VT switching
