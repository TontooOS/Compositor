//! Unix socket IPC for the Settings daemon: desktop settings owned by the
//! compositor. Deliberately extensible: wallpaper now, display, theme and
//! more ops later without touching the transport.
//!
//! Socket: `COMPOSITOR_SOCKET` or `/run/tontoo-compositor.sock`. Framing is
//! one JSON object per line, replies are `{"ok":true,"result":...}` or
//! `{"ok":false,"error":"..."}` (same shape as the windows IPC).
//!
//! Ops: `ping`, `set_wallpaper` (`{"path": "...", "fill"?}` starts a
//! macOS-like crossfade and switches the fill mode), `get_displays`
//! (outputs with modes plus brightness/night light) and `set_display`
//! (partial brightness/night light/refresh switch).
//!
//! Like the windows IPC, the listener is a calloop [`Generic`] source, so
//! requests run inside the compositor event loop with direct `&mut` access
//! to the state. Client reads are bounded (nonblocking, ~200ms max stall)
//! so a silent client can never freeze the compositor.

use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
};

use smithay::reexports::calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction};

use crate::{display, wallpaper::parse_set_wallpaper_path, TontooCompositor};

/// Default socket path, mirrored by the Settings daemon forwarder.
pub const DEFAULT_SOCKET_PATH: &str = "/run/tontoo-compositor.sock";

/// Socket path: `COMPOSITOR_SOCKET` or the default.
pub fn socket_path() -> PathBuf {
    std::env::var("COMPOSITOR_SOCKET")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH))
}

/// Bind the socket and insert it into the event loop. Shared by the winit
/// and udev backends (call site: `main.rs`, after state creation).
/// A bind failure is **not** fatal: without IPC the desktop still runs.
pub fn init(event_loop: &mut EventLoop<TontooCompositor>) -> anyhow::Result<()> {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Drop a stale socket from an unclean shutdown.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    tracing::info!("settings-ipc listening on {}", path.display());

    event_loop
        .handle()
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            |_, listener, state| {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => handle_connection(stream, state),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => {
                            tracing::warn!("settings-ipc accept failed: {:?}", e);
                            break;
                        }
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| anyhow::anyhow!("settings-ipc insert_source failed: {:?}", e))?;
    Ok(())
}

fn handle_connection(mut stream: UnixStream, state: &mut TontooCompositor) {
    let line = match read_line_bounded(&mut stream) {
        Some(line) => line,
        None => return,
    };
    let request: serde_json::Value = match serde_json::from_str(&line) {
        Ok(request) => request,
        Err(e) => {
            write_reply(&mut stream, &serde_json::json!({"ok": false, "error": format!("bad json: {e}")}));
            return;
        }
    };
    let reply = match dispatch(state, &request) {
        Ok(result) => serde_json::json!({"ok": true, "result": result}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    };
    write_reply(&mut stream, &reply);
    // Push queued wayland messages out immediately.
    let _ = state.display_handle.flush_clients();
    state.request_redraw();
}

/// Read one `\n`-terminated line, nonblocking with a bounded wait so a
/// connected-but-silent client stalls the loop for ~200ms at most.
fn read_line_bounded(stream: &mut UnixStream) -> Option<String> {
    stream.set_nonblocking(true).ok()?;
    let mut buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    for _ in 0..200 {
        match stream.read(&mut byte) {
            Ok(0) => break, // EOF
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                buf.push(byte[0]);
                if buf.len() > 65536 {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(_) => return None,
        }
    }
    if buf.is_empty() {
        return None;
    }
    String::from_utf8(buf).ok()
}

fn write_reply(stream: &mut UnixStream, reply: &serde_json::Value) {
    let line = reply.to_string() + "\n";
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.flush();
}

fn dispatch(state: &mut TontooCompositor, request: &serde_json::Value) -> Result<serde_json::Value, String> {
    let op = request
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing op".to_string())?;
    match op {
        "ping" => Ok(serde_json::json!({"pong": true})),
        "set_wallpaper" => {
            let path = parse_set_wallpaper_path(request)?;
            let fill = request.get("fill").and_then(|v| v.as_str());
            state.set_wallpaper(&path, fill)?;
            Ok(serde_json::json!({
                "path": path.to_string_lossy(),
                "fill": state.wallpaper_fill,
                "fading": true,
            }))
        }
        "get_displays" => {
            let state_obj = display::DisplayState {
                outputs: display::list_displays(state),
                brightness: (state.display_brightness * 100.0).round() as u32,
                night_light: state.display_night_light,
            };
            Ok(serde_json::to_value(state_obj).unwrap_or(serde_json::Value::Null))
        }
        "set_display" => {
            let parsed = display::parse_set_display(request)?;
            let (output, mode, brightness, night_light) =
                display::apply_display(state, &parsed)?;
            Ok(serde_json::json!({
                "output": output,
                "mode": mode,
                "brightness": brightness,
                "night_light": night_light,
            }))
        }
        other => Err(format!("unknown op: {other}")),
    }
}
