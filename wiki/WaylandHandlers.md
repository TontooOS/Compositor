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
focused surface. Updates the dock `active_app` and menubar `app_name`
based on the focused window's `app_id` or title. When no window is focused,
clears the active app and resets the menubar to `"TontooOS"`.

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
configure, creates a `Window`, maps it at (30, 40) below the menubar, and
triggers a redraw.

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

### new_decoration

```rust
fn new_decoration(&mut self, toplevel: ToplevelSurface)
```

Sets the decoration mode to `ServerSide` with a default size of 800x500.

### request_mode

Honors `ClientSide` requests. All other modes resolve to `ServerSide`.

### unset_mode

Clears the decoration mode and sends a configure.

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
- [Grabs.md](Grabs.md) -- move/resize requests create pointer grabs
- [WindowControls.md](WindowControls.md) -- traffic light clicks send
  `send_close`, `send_pending_configure`
