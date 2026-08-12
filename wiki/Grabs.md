# Grabs

Pointer grabs are used to implement interactive window movement and resizing.
They are installed on the seat pointer via `pointer.set_grab`.

## MoveSurfaceGrab

```rust
pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<TontooCompositor>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}
```

### motion

```rust
fn motion(&mut self, data, handle, _focus, event)
```

Moves the window by the pointer delta from the grab start location. Calls
`space.map_element` with the new location on each motion event.

### button

```rust
fn button(&mut self, data, handle, event)
```

Releases the grab when the left mouse button (0x110) is no longer pressed.

All other grab callbacks forward to the inner handle.

## ResizeSurfaceGrab

```rust
pub struct ResizeSurfaceGrab {
    start_data: PointerGrabStartData<TontooCompositor>,
    window: Window,
    edges: ResizeEdge,
    initial_rect: Rectangle<i32, Logical>,
    last_window_size: Size<i32, Logical>,
}
```

### ResizeSurfaceGrab::start

```rust
pub fn start(
    start_data: PointerGrabStartData<TontooCompositor>,
    window: Window,
    edges: ResizeEdge,
    initial_window_rect: Rectangle<i32, Logical>,
) -> Self
```

Sets the surface's `ResizeSurfaceState` to `Resizing` and returns a grab with
the initial rectangle.

### motion

```rust
fn motion(&mut self, data, handle, _focus, event)
```

Computes the new window size based on the pointer delta and the resize edges.
The size is clamped to the client's min/max size (min 1x1). Sets the
`xdg_toplevel` pending state to `Resizing` with the new size and sends a
configure.

### button

```rust
fn button(&mut self, data, handle, event)
```

On release, clears the `Resizing` state, sends the final configure, and
transitions to `WaitingForLastCommit`.

## ResizeEdge

```rust
bitflags::bitflags! {
    pub struct ResizeEdge: u32 {
        const TOP          = 0b0001;
        const BOTTOM       = 0b0010;
        const LEFT         = 0b0100;
        const RIGHT        = 0b1000;
        const TOP_LEFT     = Self::TOP.bits() | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();
        const TOP_RIGHT    = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}
```

## ResizeSurfaceState

```rust
enum ResizeSurfaceState {
    Idle,
    Resizing { edges, initial_rect },
    WaitingForLastCommit { edges, initial_rect },
}
```

Tracks the resize lifecycle per surface. Stored in the surface data map.

### handle_commit

```rust
pub fn handle_commit(space: &mut Space<Window>, surface: &WlSurface) -> Option<()>
```

Called on surface commit. If the resize is waiting for the final commit,
repositions the window to account for the new size when resizing from the
top or left edges.

## Cross References

- [WaylandHandlers.md](WaylandHandlers.md) -- move/resize requests initiate grabs
- [Input.md](Input.md) -- titlebar drag starts a `MoveSurfaceGrab`
