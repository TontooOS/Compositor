# Input

The input module processes keyboard, pointer motion, pointer button, and
pointer axis events. It also manages dock hover detection, tontoo_ui hover,
and application launching.

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

Updates the cursor speed magnification, dock hover state, and tontoo_ui hover
state. Then forwards the event to the focused client. Traffic light hover was
removed with CSD.

#### Pointer Absolute Motion

Same as pointer motion but with absolute coordinates, transformed relative
to the output geometry.

#### Pointer Button

On button press, the following elements are checked in order:

1. **Dock icon clicks**: iterates over dock icon hit rectangles. On match,
   triggers the bounce animation, sets the active app, launches the app, and
   marks the icon as running.

2. **TontooUI surface clicks**: checks if the click falls within a
   `tontoo_ui` surface. Performs hit testing on the widget tree and sends
   `widget_clicked` events.

3. **Window focus**: on first click on a window, raises it, sets keyboard
   focus, and updates the dock active app. On second click on the
   same window, passes the click through to the application. On click on
   empty space, deactivates all windows.

> **Removed:** Window traffic light clicks and titlebar drag are no longer
> handled here — windows use Client-Side Decorations. Move is via the
> client's `xdg_toplevel.move_request`.

#### Pointer Axis

Forwards horizontal and vertical scroll events with v120 discrete values.

## App Launching

### launch_app_static

```rust
fn launch_app_static(name: &str)
```

Launches an application by name. Each app tries a list of fallback binaries
in order:

| App | Fallbacks |
|---|---|
| `"Finder"` | `nautilus`, `dolphin`, `thunar`, `pcmanfm`, `nemo` |
| `"Terminal"` | `foot`, `alacritty`, `kitty`, `weston-terminal` |
| `"Settings"` | `gnome-control-center`, `systemsettings`, `xfce4-settings-editor` |
| `"Notes"` | Placeholder (logs "coming soon") |
| `"Podcasts"` | Placeholder (logs "coming soon") |
| Other | Direct `std::process::Command::new(name)` |

All spawned processes receive `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`,
`XDG_SESSION_TYPE=wayland`, and `MOZ_ENABLE_WAYLAND=1` in their environment.

## Hit Testing

### dock_icon_rects

```rust
fn dock_icon_rects(screen_w: f64, screen_h: f64, icon_count: usize) -> Vec<Rectangle<i32, Logical>>
```

Computes the hit rectangles for all dock icons given the screen dimensions.
Uses the same constants as the dock renderer: 78px dock height, 15px bottom
margin, 48px icon size, 12px icon gap.

### update_dock_hover

Updates the dock hover state based on the pointer position.

> `update_traffic_light_hover` was removed with CSD — clients handle their
> own hover. `WindowControls` remains as an app helper.

### update_tontoo_ui_hover

Performs hit testing on `tontoo_ui` widget trees and sends
`widget_hovered` events when the hovered node changes.

## Cross References

- [State.md](State.md) -- `process_input_event` is defined on `TontooCompositor`
- [Cursor.md](Cursor.md) -- `update_speed` is called on pointer motion
- [Dock.md](Dock.md) -- `bounce_icon`, `set_hover`, `set_active_app`
- [Menubar.md](Menubar.md) -- external `Menubar.app` (no compositor state)
- [WindowControls.md](WindowControls.md) -- helper for apps (no longer used by compositor)
- [Grabs.md](Grabs.md) -- `MoveSurfaceGrab` now via client `move_request` (CSD)
