# WindowsIpc

Unix socket IPC serving CoreWindows: window listing and actions
(minimize, restore, fullscreen, graceful close). Implemented in
`src/windows_ipc.rs`, initialized once in `main.rs` after state
creation, shared by the winit and udev backends.

## Socket

| Item | Value |
|---|---|
| Default path | `/run/tontoo-windows.sock` |
| Override | `WINDOWS_SOCKET` environment variable |
| Framing | One JSON object per line, one JSON reply line |

A stale socket file from an unclean shutdown is removed before bind.
A bind failure is **not** fatal: it logs a warning and the desktop
runs without IPC (useful for dev sessions without `/run` write
access, where `WINDOWS_SOCKET` points elsewhere).

## Event loop integration

The listener is a calloop `Generic` source, so requests run inside
the compositor event loop with direct `&mut TontooCompositor` access
to the space. Client reads are nonblocking with a bounded ~200ms
wait, so a connected-but-silent client can never freeze the
compositor. After each request, queued Wayland messages
(configure/close) are flushed and a redraw is requested.

## Window ids

Daemon-side window ids (`u64`, starting at 1) are assigned on first
use from `TontooCompositor::next_window_id`, keyed by surface id in
`TontooCompositor::window_ids`. Ids stay stable while the surface
lives; the table is pruned on every `list_windows` so it cannot grow
forever.

## Ops

| Op | Request | Effect |
|---|---|---|
| `ping` | `{"id":1,"op":"ping"}` | Answers `{"pong":true}` |
| `list_windows` | `{"id":1,"op":"list_windows"}` | All mapped windows plus minimized ones: `{"windows":[{"id":1,"app_id":"...","title":"...","pid":1234}]}` (all fields but `id` optional; minimized rows carry `"minimized":true`) |
| `minimize_window` | `{"id":1,"op":"minimize_window","window":5}` | Reuses `shell::ssd::minimize_to_dock` (unmap + dock icon, restore via dock click) |
| `restore_window` | `{"id":1,"op":"restore_window","window":5}` | Restores a minimized window (re-map centered + focus, drop temp dock icon) via `shell::ssd::restore_minimized`; errors when the id is not minimized or its client is gone |
| `set_fullscreen` | `{"id":1,"op":"set_fullscreen","window":5,"fullscreen":true}` | Wayland: `XdgState::Fullscreen` + output-size configure, geometry saved in `fullscreen_restore` and restored on exit. X11: `X11Surface::set_fullscreen` |
| `close_window` | `{"id":1,"op":"close_window","window":5}` | Graceful close: `xdg_toplevel.send_close` on Wayland, `WM_DELETE_WINDOW` (`X11Surface::close`) on X11. The app may show a save dialog |

Replies are `{"ok":true,"result":...}` or
`{"ok":false,"error":"..."}`. Unknown ops, missing fields and
unknown window ids answer `ok: false`. Force quit needs no daemon
op: CoreWindows sends `SIGKILL` to the reported pid directly.

## Pid source

Wayland pids come from client credentials
(`get_client` + `get_credentials`); X11 pids come from
`_NET_WM_PID` (`X11Surface::pid`). X11 handling is compiled only
with the `udev` feature; without it the helpers report "not X11".

## Usage / Example

```bash
WINDOWS_SOCKET=/tmp/tontoo-windows.sock cargo run -- --winit
printf '{"id":1,"op":"list_windows"}\n' | socat - UNIX-CONNECT:/tmp/tontoo-windows.sock
```

## Cross References

- [State.md](State.md) – `window_ids`, `next_window_id`, `fullscreen_restore` fields
- [Shell.md](Shell.md) – shell state touched by minimize
- [WindowControls.md](WindowControls.md) – SSD minimize/maximize this reuses
- [XWayland.md](XWayland.md) – X11 window sources
