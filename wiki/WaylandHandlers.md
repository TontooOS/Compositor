# WaylandHandlers

This module implements all the smithay trait-based Wayland protocol handlers
for the compositor.

## CompositorHandler

### commit

```rust
fn commit(&mut self, surface: &WlSurface)
```

Processes a `wl_surface.commit`. The sequence is:

1. Call `on_commit_buffer_handler` to process any new buffer.
2. Walk up to the root surface (skipping sync subsurfaces).
3. Call `window.on_commit()` on the matching window.
4. Forward to `xdg_shell::handle_commit` for popup/toplevel processing.
5. Forward to `resize_grab::handle_commit` for resize repositioning.

## BufferHandler

```rust
fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
```

No-op buffer destruction handler.

## ShmHandler

```rust
fn shm_state(&self) -> &ShmState
```

Returns a reference to `self.shm_state`.

## SeatHandler

### cursor_image

```rust
fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus)
```

Forwards the cursor image to `CursorState::handle_cursor_image`.

### focus_changed

```rust
fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>)
```

Resets cursor visibility, updates the data device focus, and tracks the
focused surface. Updates the dock `active_app` based on the focused
window's `app_id` or title. When no window is focused, clears the active
app. (The top bar is the external `Menubar.app`; the compositor holds no
menubar state.)

## Data Device

`ClientDndGrabHandler`, `ServerDndGrabHandler`, and `DataDeviceHandler` are
implemented as empty trait impls. Selection handling delegates to
`SelectionHandler`.

## OutputHandler

```rust
impl OutputHandler for TontooCompositor {}
```

Empty implementation.

## XdgShellHandler

### new_toplevel

```rust
fn new_toplevel(&mut self, surface: ToplevelSurface)
```

Configures the new toplevel with a default size of 800x500, sends the
configure, creates a `Window`, maps it at (30, 40) below the reserved
top strut for the external `Menubar.app`, and triggers a redraw.

### new_popup

Tracks the popup via `PopupManager`.

### move_request

Initiates a `MoveSurfaceGrab` when the grab serial is valid.

### resize_request

Initiates a `ResizeSurfaceGrab` with the given edges.

### handle_commit

Handles the initial configure for toplevels that have not yet received one.

### unconstrain_popup

Unconstrains a popup to the output geometry.

## XdgDecorationHandler

The compositor defaults to **Client-Side Decorations (CSD)** so every
app draws its own header from the system theme (MacTahoe for GTK, qt5ct
palette for Qt, portal color-scheme for Chrome/Firefox/Electron).
`request_mode` honors an explicit client wish for `ServerSide`
(KWin-style, strictly opt-in, e.g. Chrome with "Use system title bar").
Nothing is ever forced, so CSD apps can not lose their header.
See [WindowControls.md](WindowControls.md).

### new_decoration

```rust
fn new_decoration(&mut self, toplevel: ToplevelSurface)
```

Sets the decoration mode to `ClientSide` with a default size of 800x500.

### request_mode

```rust
fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode)
```

Honors the requested mode and sends a configure. `ServerSide` activates
the compositor traffic-light titlebar; `ClientSide` keeps app-drawn
decorations.

### unset_mode

Sets the decoration mode to `ClientSide` and sends a configure (previously
cleared the mode).

## WlrLayerShellHandler

### new_layer_surface

```rust
fn new_layer_surface(
    &mut self,
    surface: LayerSurface,
    output: Option<WlOutput>,
    layer: Layer,
    namespace: String,
)
```

Maps the layer surface onto the first available output. Logs a warning when
no output is available.

### layer_destroyed

Unmaps the destroyed layer surface from the layer map.

### Layer configure cycle (`CompositorHandler::commit`)

The smithay delegate never configures layer surfaces on its own, so the
commit handler drives the cycle: when a layer surface commits, the layer
map re-arranges with the new size and `send_pending_configure` proposes
the arranged size back. Without this the client waits for its initial
configure forever (seen with `Menubar.app`: it committed `0x200` and
stayed invisible). Rendering picks the surfaces up via
`render_elements_from_surface_tree` at the arranged
`layer_geometry` (see [Rendering.md](Rendering.md)).

## Delegate Macros

```rust
smithay::delegate_compositor!(TontooCompositor);
smithay::delegate_shm!(TontooCompositor);
smithay::delegate_output!(TontooCompositor);
smithay::delegate_seat!(TontooCompositor);
smithay::delegate_xdg_shell!(TontooCompositor);
smithay::delegate_xdg_decoration!(TontooCompositor);
smithay::delegate_data_device!(TontooCompositor);
smithay::delegate_layer_shell!(TontooCompositor);
```

## Cross References

- [State.md](State.md) -- handler implementations access `TontooCompositor` state
- [Grabs.md](Grabs.md) -- move/resize requests create pointer grabs (clients drive move via CSD)
- [WindowControls.md](WindowControls.md) -- SSD titlebar rendering and traffic-light actions
- [Rendering.md](Rendering.md) -- CSD windows draw their own header; SSD windows get a compositor bar
