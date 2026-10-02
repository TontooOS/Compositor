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
| 1 | One buffer pixel per output pixel (full resolution, slowest) |
| 2 | Half resolution (default) |
| 4 | Quarter resolution (cheapest, still smooth after a blur) |

Blur hides the missing resolution, so a divisor of 2 or 4 is visually
indistinguishable from full resolution for frosted surfaces.

## Redraw Policy

Capturing costs one extra draw pass plus one `glReadPixels`, and
`glReadPixels` stalls the pipeline until the copy finished. The compositor
therefore only recaptures when

- the client asked for a new frame (`ack_backdrop` received or the window
  moved or resized), or
- the damage of the last presented frame intersects the window rect,

and never while a previous frame is still unacknowledged. With a static
desktop behind a static window the compositor stops working entirely after
the first frame.

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
| `last_region` | Last captured rect, used to detect moves and resizes |

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

### capture

```rust
pub fn capture(
    space: &Space<Window>,
    renderer: &mut GlesRenderer,
    output: &Output,
    window: &Window,
    wallpaper: Option<(&TextureBuffer<GlesTexture>, &Wallpaper, &str)>,
    buffer: &mut BackdropBuffer,
    scale: u32,
    clear_color: Color32F,
) -> bool
```

Renders everything below `window` inside the window rect into an offscreen
texture at `1 / scale` resolution, reads it back and writes it into
`buffer` at the window rect position.

| Case | Result |
|---|---|
| Window geometry is empty or off-output | `false` |
| The window is on another output | `false` |
| The window has no render elements | `false` |
| The rect does not fit the client buffer | `false` |
| No wallpaper and no other window below | `false` |
| Texture allocation or readback fails | `false` |
| Success | `true` |

Rows are flipped: OpenGL returns the bottom row first, the buffer is
top-down. All values in the returned `BackdropFrame` are physical pixels.

## Cross References

- [TontooUiProtocol.md](TontooUiProtocol.md) -- the protocol messages
- [Rendering.md](Rendering.md) -- winit render pipeline and z-order
- [UdevBackend.md](UdevBackend.md) -- udev render pipeline and z-order
- [Accessibility.md](Accessibility.md) -- reduce transparency kill switch
- [WidgetRenderer.md](WidgetRenderer.md) -- the compositor side glass panel
  that remains for widget-tree clients
