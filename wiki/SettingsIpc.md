# SettingsIpc

Unix socket IPC for the Settings daemon: desktop settings owned by the
compositor. The op table is deliberately extensible (wallpaper now,
display, theme and more later) without touching the transport.

## Protocol

Socket: `COMPOSITOR_SOCKET` or `/run/tontoo-compositor.sock`. One JSON
object per line, one reply line per request (same shape as the windows
IPC: `{"ok": true, "result": ...}` or `{"ok": false, "error": ...}`).

| Op | Params | Result |
|---|---|---|
| `ping` | — | `{"pong": true}` |
| `set_wallpaper` | `{"path": "/abs/image.png"}` | `{"path": "...", "fading": true}` |

Rules:

- The listener is a calloop source, so requests run inside the
  compositor event loop with direct `&mut` state access (shared by the
  winit and udev backends, registered in `main.rs`).
- Client reads are bounded (nonblocking, ~200ms max stall) so a silent
  client can never freeze the compositor.
- A bind failure is not fatal: without IPC the desktop still runs.
- Unknown ops return `ok: false`; new ops extend `dispatch` only.

## API

```rust
pub fn socket_path() -> PathBuf;
pub fn init(event_loop: &mut EventLoop<TontooCompositor>) -> anyhow::Result<()>;
```

- `socket_path` reads `COMPOSITOR_SOCKET` with fallback to the default.
- `init` binds (dropping a stale file), sets nonblocking mode and
  inserts the accept source. Returns `Err` when binding fails.

## Usage / Example

```bash
printf '{"op": "set_wallpaper", "path": "/System/User/Wallpapers/SONOMA/IMAGE.png"}\n' | socat - UNIX-CONNECT:/run/tontoo-compositor.sock
```

## Cross References

- [Wallpaper.md](Wallpaper.md) -- crossfade behind `set_wallpaper`
- [WindowsIpc.md](WindowsIpc.md) -- sibling socket (same transport pattern)
- [Rendering.md](Rendering.md) -- wallpaper render layers
