# XWayland

X11 applications (e.g. `xterm`) run on TontooOS through an embedded
XWayland server plus a built-in X11 window manager. Both live in
`src/xwayland.rs` and are only compiled into the udev/DRM backend (the
live system); the winit development backend stays Wayland-only.

## Startup

```rust
pub fn start_xwayland(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>>
```

Called at the end of `udev::init_udev`. The sequence:

1. `start_xwayland` creates `/tmp/.X11-unix` (mode `1777`) first.
   smithay binds X11 sockets there directly and fails with `ENOENT`
   when the directory is missing, which minimal live systems often lack.
2. `XWayland::spawn` launches `/usr/bin/Xwayland` (package
   `xorg-xwayland`) with `-rootless -terminate`, display auto-selected,
   `XWAYLAND_NO_GLAMOR=1` (software presentation: llvmpipe VMs expose no
   render node, so glamor init would fail).
3. The `XWayland` handle is owned by the event loop; dropping it (loop
   shutdown) stops the server.
4. On `XWaylandEvent::Ready`, `X11Wm::start_wm` registers us as the X11
   window manager, and `DISPLAY=:<n>` is exported process-wide so spawned
   children inherit it. On `XWaylandEvent::Error` (binary missing) only a
   warning is logged; the Wayland desktop keeps working.

`X11Wm::start_wm` requires a `LoopHandle<'static>`. The event loop
outlives the whole compositor run and the extended handle is used solely
inside loop callbacks, so the borrow is extended with an `unsafe`
transmute (documented at the call site).

## Window Management

`impl XwmHandler for TontooCompositor` maps every X11 top-level to a
regular `Window::new_x11_window`:

| Handler | Behavior |
|---|---|
| `map_window_request` | `set_mapped(true)`, cascade placement, `map_element(..., activate=true)` |
| `mapped_override_redirect_window` | Mapped at client position (menus, tooltips) |
| `unmapped_window`, `destroyed_window` | `unmap_elem` + redraw |
| `configure_request` | Grants geometry, repositions the space element |
| `configure_notify` | Redraw |
| `resize_request`, `move_request` | Ignored in v1 (logged at debug) |

Rendering, stacking and pointer hit-testing reuse the shared `Space`
machinery, so X11 windows behave like Wayland windows.

## Input Focus

`src/input.rs` resolves the focused surface via
`xwayland::window_wl_surface` (xdg toplevel first, X11 `wl_surface`
otherwise) instead of unwrapping `toplevel()`, which would panic on X11
windows. `xwayland::x11_app_name` (X11 title, then class) extends the shell
(`Dock.app` / `Menubar.app`) app-name chain, and `send_pending_configure`
loops skip windows without an xdg toplevel.

## Panic Safety

Two traps crash-loop the compositor as soon as the first X11 window maps,
so both are handled explicitly:

1. Client data: smithay inserts the XWayland server's internal client with
   `XWaylandClientData`, not our `ClientState`. `client_compositor_state`
   (`src/handlers/compositor.rs`) serves the XWayland client's own
   `compositor_state` first and falls back to a shared static instead of
   unwrapping.
2. Surface lookup: `Window::toplevel()` returns `None` for X11 windows.
   `state::window_wl_surface_any` (xdg toplevel or X11 `wl_surface`)
   replaces every `.toplevel().unwrap()` in space-element scans
   (`compositor.rs`, `xdg_shell.rs`, `handlers/mod.rs`,
   `resize_grab.rs`). XDG-only follow-ups (initial configure) are guarded
   by `window.toplevel()`.

## Protocol State

`XWaylandShellState::new` is registered in `TontooCompositor::new` and
`XWaylandShellHandler` is implemented (both udev-gated), plus the
`delegate_xwayland_shell!` macro in `src/handlers/mod.rs`.

## Cross References

- [UdevBackend.md](UdevBackend.md) - backend that hosts the XWayland server
- [State.md](State.md) - `xwayland_state`, `xwayland_shell_state` fields
- [Input.md](Input.md) - click-to-focus for X11 windows
