# Input

The input module processes keyboard, pointer motion, pointer button, and
pointer axis events. It also manages tontoo_ui hover and SSD titlebar
hover.

> **CSD:** The compositor no longer handles window traffic light
> clicks or titlebar drag. Those are drawn and handled by clients.
> Window movement is via `xdg_toplevel::move_request` from the client.

## Input Processing

### process_input_event

```rust
pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>)
```

Dispatches input events by type.

#### Keyboard

Forwards keyboard events to the focused client. Two special shortcuts are
handled before forwarding:

| Shortcut | Action |
|---|---|
| `Super+Enter` | Launches the first available terminal |
| `Ctrl+Alt+F1..F12` | Switches to VT 1..12 (udev backend only) |

#### Pointer Motion

Updates the cursor speed magnification and tontoo_ui hover state. Then
forwards the event to the focused client. Traffic light hover was
removed with CSD.

#### Pointer Absolute Motion

Same as pointer motion but with absolute coordinates, transformed relative
to the output geometry.

#### Pointer Button

On button press, the following elements are checked in order:

1. **TontooUI surface clicks**: checks if the click falls within a
   `tontoo_ui` surface. Performs hit testing on the widget tree and sends
   `widget_clicked` events.

2. **SSD titlebar clicks**: traffic-light actions and move drags for
   server-side-decorated windows (opt-in only, see CSD note above).

3. **Window focus**: a click on a window raises it, sets keyboard
   focus, and passes the click through to the application in the same
   press (single-click select + activate, macOS behavior). On click on
   empty space, deactivates all windows.

> **Removed:** Window traffic light clicks and titlebar drag are no longer
> handled here — windows use Client-Side Decorations. Move is via the
> client's `xdg_toplevel.move_request`.

#### Pointer Axis

Forwards horizontal and vertical scroll events with v120 discrete values.

## App Launching

### launch_terminal

```rust
fn launch_terminal()
```

Launches a terminal for the `Super+Enter` shortcut, trying `foot`,
`alacritty`, `kitty`, `weston-terminal` in order. All other apps are
launched from the external `Dock.app` / LaunchPad; the compositor
itself launches nothing else.

All spawned processes receive `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`,
`XDG_SESSION_TYPE=wayland`, and `MOZ_ENABLE_WAYLAND=1` in their environment.

## Hit Testing

### update_ssd_hover

Tracks pointer hover over SSD titlebars and sets the per-window
`WindowControls.hovered` flag so hover symbols render. Requests a redraw
when any flag changes.

### update_tontoo_ui_hover

Performs hit testing on `tontoo_ui` widget trees and sends
`widget_hovered` events when the hovered node changes.

## Cross References

- [State.md](State.md) -- `process_input_event` is defined on `TontooCompositor`
- [Cursor.md](Cursor.md) -- `update_speed` is called on pointer motion
- [Dock.md](Dock.md) -- external `Dock.app` (no compositor state)
- [Menubar.md](Menubar.md) -- external `Menubar.app` (no compositor state)
- [WindowControls.md](WindowControls.md) -- helper for apps (no longer used by compositor)
- [Grabs.md](Grabs.md) -- `MoveSurfaceGrab` now via client `move_request` (CSD)
