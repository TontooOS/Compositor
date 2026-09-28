# SettingsIpc

Unix socket IPC for the Settings daemon: desktop settings owned by the
compositor. The op table is deliberately extensible (wallpaper and
display now, theme and more later) without touching the transport.

## Protocol

Socket: `COMPOSITOR_SOCKET` or `/run/tontoo-compositor.sock`. One JSON
object per line, one reply line per request (same shape as the windows
IPC: `{"ok": true, "result": ...}` or `{"ok": false, "error": ...}`).

| Op | Params | Result |
|---|---|---|
| `ping` | — | `{"pong": true}` |
| `set_wallpaper` | `{"path": "/abs/image.png", "fill"?}` | `{"path": "...", "fill": "...", "fading": true}` |
| `get_displays` | — | `{"outputs": [...], "brightness": 0-100, "night_light": bool}` |
| `set_display` | `{"output"?, "width"?, "height"?, "refresh"?, "brightness"?, "night_light"?}` | `{"output", "mode", "brightness", "night_light"}` |

Rules:

- The listener is a calloop source, so requests run inside the
  compositor event loop with direct `&mut` state access (shared by the
  winit and udev backends, registered in `main.rs`).
- Client reads are bounded (nonblocking, ~200ms max stall) so a silent
  client can never freeze the compositor.
- A bind failure is not fatal: without IPC the desktop still runs.
- Unknown ops return `ok: false`; new ops extend `dispatch` only.
- `fill` is optional (`fill`, `fit`, `stretch`, `center`, `tile`;
  see [Wallpaper.md](Wallpaper.md)): a valid mode switches the render
  mode immediately, omitting it keeps the current mode, unknown modes
  are rejected without touching anything.

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
printf '{"op": "get_displays"}\n' | socat - UNIX-CONNECT:/run/tontoo-compositor.sock
printf '{"op": "set_display", "brightness": 80.0}\n' | socat - UNIX-CONNECT:/run/tontoo-compositor.sock
```

## Cross References

- [Wallpaper.md](Wallpaper.md) -- crossfade behind `set_wallpaper`
- [Display.md](Display.md) -- outputs, refresh switching and overlays
- [WindowsIpc.md](WindowsIpc.md) -- sibling socket (same transport pattern)
- [Rendering.md](Rendering.md) -- wallpaper render layers
