# Backdrop

The backdrop stream lets a client render the desktop backdrop blur itself
instead of asking the compositor to blur. The compositor captures the pixels
behind a window once, hands them to the client through a shared memory file,
and then does nothing: blur radius, lens magnification, frost and rim shading
are all computed by the client (TontooUI runs a WGSL compute blur, see
`BackdropBlur` in `TontooLibs/TontooUI/src/renderer/backdrop.rs`).

Wayland never lets a client read the contents of other surfaces, so the pixels
always have to come from the compositor. What this feature removes from the
compositor is the *blur*, not the *capture*.

## Why

Before this feature the only way to get a frosted background was a
`tontoo_ui_surface` widget tree drawn by the compositor. The compositor had a
Gaussian shader for it but never instantiated it (no framebuffer object, no
render target), so `render_glass_cmd` only drew a flat translucent quad.

Splitting the work this way has three effects:

| Concern | Owner |
|---|---|
| Deciding which pixels are behind a window | Compositor |
| Copying those pixels out of the GPU | Compositor |
| Blurring, lensing and frosting them | Client |
| Choosing the blur radius per view | Client |

A client that does not want the stream keeps the old behaviour: nothing is
captured and nothing is sent.

## Protocol

All messages live on the existing `tontoo_ui_surface` interface, version 1.

### Requests

| Request | Description |
|---|---|
| `create_backdrop_buffer(fd, width, height, stride)` | Client hands over a shared memory file descriptor |
| `set_backdrop(enabled, scale, wl_surface_id)` | Enables or disables the stream for one window |
| `ack_backdrop(serial)` | Client confirms it copied a streamed frame |

### Events

| Event | Description |
|---|---|
| `backdrop(serial, x, y, width, height, scale)` | A new frame is available in the shared buffer |

The shared buffer layout is `Xrgb8888`, top-down, `stride` bytes per row,
`width` times `height` pixels. `x`, `y`, `width` and `height` are physical
output pixels and describe the window rect inside that buffer. `scale` is the
downscale divisor: consecutive sampled pixels are `scale` apart in the buffer.

## Pixel Geometry

Everything on the wire and in the shared buffer is in **physical** pixels.
The window rect comes out of the space in logical pixels, so `capture`
converts it once with the output scale and derives everything from there:

| Value | Meaning |
|---|---|
| `x`, `y` | Window rect origin relative to the output origin, physical |
| `width`, `height` | Window rect size in physical pixels |
| `scale` | Downscale divisor |
| columns | `(width - 1) / scale + 1` |
| rows | `(height - 1) / scale + 1` |

The offscreen target is `columns` x `rows` and the samples land in the buffer
at `(x + i * scale, y + j * scale)`, so a client derives the sample grid
with integer arithmetic alone.

OpenGL returns the bottom row of a readback first while the buffer is
top-down, so `capture` walks the readback from its last row.

## Client Protocol

1. Create a shared memory file (`memfd_create` or a file in `/dev/shm`), size
   it to `stride * height`, map it read-write.
2. Send `create_backdrop_buffer` with the file descriptor and the geometry.
   The compositor maps the same file.
3. Bind `tontoo_ui_manager`, create a `tontoo_ui_surface` and send
   `set_backdrop(1, scale, wl_surface_id)` where `wl_surface_id` is the
   protocol id of the client's own `wl_surface`.
4. On every `backdrop` event, copy the sampled pixels out of the mapping and
   send `ack_backdrop(serial)`.
5. Send `set_backdrop(0, ...)` when no glass is on screen.

The protocol id space is per client, so `wl_surface_id` only works when both
objects live on the same connection. A window created by a third-party
toolkit (winit) shares its connection with the client-side protocol objects,
so the id stays valid.

## Scale

`set_backdrop` takes a divisor, clamped to `1..=4`:

| Divisor | Meaning |
|---|---|
| 1 | One buffer pixel per output pixel (full resolution) |
| 2 | Half resolution (the default a client gets when it sends `0`) |
| 4 | Quarter resolution (cheapest, still smooth after a blur) |

With the redraw policy below a static backdrop is captured exactly once, so
the readback is not a per-frame cost and **1 is the right choice for a panel
that only opens over a still desktop**: full resolution means the client blur
runs on real pixels. Divisors above 1 only pay off when the content behind the
window actually animates, where the smaller readback buys back frame rate.

## Redraw Policy

Capturing costs one extra draw pass plus one `glReadPixels`, and
`glReadPixels` stalls the pipeline until the copy finished. The compositor
therefore captures **only when the backdrop actually changed**, and never
while a previous frame is still unacknowledged.

The trigger is an explicit `dirty` flag, not frame damage. Frame damage is
the union of every element, including the window itself, so a window that
repaints its own glass would force a capture on every frame even when the
desktop behind it never moves. Instead:

| Source | Effect |
|---|---|
| `set_backdrop` enabling the stream | `dirty` |
| `CompositorHandler::commit` for a surface whose rect intersects the stream rect | `dirty` |
| Any change in the per-frame window geometry snapshot | `dirty` on every stream |
| `set_wallpaper`, wallpaper crossfade finished | `dirty` on every stream |
| `apply_display` changed brightness or night light | `dirty` on every stream |
| Window geometry differs from the last captured rect | captures directly |

Two details matter:

- **Commit filtering.** A stream is only dirtied when the committed surface
  actually overlaps its rect, and never for the surface it watches. The
  menubar clock repaints once a second; without the overlap test it would
  keep invalidating a panel in the middle of the screen.
- **Geometry snapshot.** Moving, resizing, maximizing or fullscreening a
  window produces no buffer commit at all, so `commit` cannot see it.
  `BackdropGeometry::refresh` compares one `Vec` of `(protocol id, rect)` per
  frame and dirties everything on a change. That single comparison covers
  drag, resize, shortcuts, the windows IPC, mapping, unmapping and
  layer-surface repositioning.

With a static desktop behind a static window the compositor captures once
and then stops working entirely.

### Self-heal

X11 surfaces repaint through `XWayland` and never reach
`CompositorHandler::commit`, so a video playing behind a TontooOS window
would freeze the backdrop for good. A `1 s` cadence covers that.

The cadence is **only active while an X11 window overlaps the stream rect**,
detected by `untracked_overlap` (`Window::toplevel` is `None` exactly for X11
windows). On a pure Wayland desktop it never fires, which is what makes
"captured exactly once over a still desktop" true. A stream over an X11
window keeps refreshing once a second, which is the only case where that
matters.

A failed capture sets `retry_after` one second out, so a rect that
permanently does not fit the client buffer cannot spin the offscreen pass
every frame.

## Tracing

`capture` logs at debug level:

| Field | Meaning |
|---|---|
| `x`, `y`, `w`, `h` | Captured rect in physical pixels |
| `scale` | Downscale divisor |
| `reason` | `dirty`, `moved` or `self-heal` |
| `captures` | Running counter for this stream |

A stream over a static desktop should stay at `captures: 1` until something
behind it moves.

## Reduce Transparency

`AccessibilitySettings::reduce_transparency` disables the stream entirely:
the compositor stops capturing and the client sees no `backdrop` events, so
glass falls back to its plain translucent tint.

## Types

### BackdropStream

```rust
pub struct BackdropStream {
    pub enabled: bool,
    pub scale: u32,
    pub wl_surface_id: u32,
    pub buffer: Option<BackdropBuffer>,
    pub serial: u32,
    pub pending_ack: Option<u32>,
    pub last_region: Option<Rectangle<i32, Logical>>,
}
```

Per `tontoo_ui_surface` stream state, stored in `TontooUiSurfaceState`.

| Field | Description |
|---|---|
| `enabled` | Last value of `set_backdrop` |
| `scale` | Clamped downscale divisor |
| `wl_surface_id` | Protocol id of the watched window, `0` when unset |
| `buffer` | Client shared mapping, `None` before `create_backdrop_buffer` |
| `serial` | Counter, incremented on every sent frame |
| `pending_ack` | Serial waiting for `ack_backdrop`, blocks the next capture |
| `last_region` | Last captured rect in physical pixels, detects moves and resizes |
| `dirty` | Set when something behind the window changed, cleared on capture |
| `retry_after` | Blocks a retry after a failed capture |
| `last_capture` | Time of the last capture, drives the self-heal cadence |
| `captures` | Running capture counter, reported in the debug log |

### BackdropBuffer

```rust
pub struct BackdropBuffer {
    map: *mut u8,
    len: usize,
    width: i32,
    height: i32,
    stride: i32,
}
```

A writable mapping of the client file. `Drop` calls `munmap`.

### BackdropBuffer::new

```rust
pub unsafe fn new(fd: RawFd, width: i32, height: i32, stride: i32) -> Option<Self>
```

Maps `stride * height` bytes read-write.

| Case | Result |
|---|---|
| `width` or `height` below 1 | `None` |
| `stride` below `width * 4` | `None` |
| `mmap` fails | `None` |

### BackdropGeometry

```rust
pub struct BackdropGeometry {
    last: Vec<(u32, Rectangle<i32, Physical>)>,
}

pub fn refresh(&mut self, space: &Space<Window>, out_scale: f64) -> bool
```

Snapshot of every space element's protocol id and rect. Returns `true` when
anything moved, was added or disappeared. Costs one small vector compare per
frame and is what catches window moves, which produce no buffer commit.

### dirty_all

```rust
pub fn dirty_all(tontoo_ui: &mut TontooUiState)
```

Marks every stream for recapture. Used for changes that affect the whole
output: wallpaper, crossfade, brightness and night light.

### dirty_intersecting

```rust
pub fn dirty_intersecting<'a>(
    streams: &mut impl Iterator<Item = &'a mut BackdropStream>,
    rect: Rectangle<i32, Physical>,
    skip_surface_id: u32,
)
```

Marks every stream whose `last_region` overlaps `rect`, except the stream
watching `skip_surface_id`.

Streams that never captured have no `last_region` and are skipped; they
capture on their first frame anyway.

### dirty_from_commit

```rust
pub fn dirty_from_commit(
    tontoo_ui: &mut TontooUiState,
    space: &Space<Window>,
    surface: &WlSurface,
)
```

Called from `CompositorHandler::commit` for every frame a client paints.
Resolves the surface to a layer surface (dock, menubar) first, then to a
space window, converts to output-local physical pixels and calls
`dirty_intersecting`.

| Case | Effect |
|---|---|
| No stream is enabled | Returns immediately |
| Surface is a layer surface | Dirts overlapping streams with the layer geometry |
| Surface is a space window | Same, scaled to physical pixels |
| Surface is a popup or unclaimed | Returns, it sits above the desktop |

### CaptureReason

```rust
pub enum CaptureReason { Dirty, Moved, SelfHeal }
```

Reported in the debug log so the capture pattern of a stream can be read off
the compositor log. `as_str()` yields `dirty`, `moved` or `self-heal`.

### needs_capture

```rust
pub fn needs_capture(
    &self,
    region: Rectangle<i32, Physical>,
    now: Instant,
    untracked: bool,
) -> Option<CaptureReason>
```

Checked in order: backoff, `dirty`, region changed, self-heal cadence. The
`untracked` flag gates the cadence and must only be set when an X11 window
overlaps the rect.

### capture

```rust
pub fn capture(
    scratch: &mut BackdropCapture,
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    output: &Output,
    window: &Window,
    wallpaper: Option<(&TextureBuffer<GlesTexture>, &Wallpaper, &str, Option<f32>)>,
    buffer: &mut BackdropBuffer,
    scale: u32,
    clear_color: [f32; 4],
) -> Option<BackdropFrame>
```

Renders everything below `window` inside the window rect into an offscreen
texture at `1 / scale` resolution, reads it back and writes it into
`buffer` at the window rect position.

| Case | Result |
|---|---|
| Window geometry is empty or off-output | `None` |
| The window is on another output | `None` |
| The window has no render elements | `None` |
| The rect does not fit the client buffer | `None` |
| Texture allocation or readback fails | `None` |
| Success | `Some(BackdropFrame)` |

Rows are flipped: OpenGL returns the bottom row first, the buffer is
top-down. All values in the returned `BackdropFrame` are physical pixels.

The readback buffer lives in `BackdropCapture` and is reused across
captures, so a full-resolution capture does not allocate on every call.

## Cross References

- [TontooUiProtocol.md](TontooUiProtocol.md) -- the protocol messages
- [Rendering.md](Rendering.md) -- winit render pipeline and z-order
- [UdevBackend.md](UdevBackend.md) -- udev render pipeline and z-order
- [Accessibility.md](Accessibility.md) -- reduce transparency kill switch
- [WidgetRenderer.md](WidgetRenderer.md) -- the compositor side glass panel
  that remains for widget-tree clients
