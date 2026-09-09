//! Unix socket IPC for CoreWindows: window listing and actions.
//!
//! Socket: `WINDOWS_SOCKET` or `/run/tontoo-windows.sock`. Framing is one
//! JSON object per line, replies are `{"ok":true,"result":...}` or
//! `{"ok":false,"error":"..."}` (see CoreWindows `wiki/Windows.md`).
//!
//! Ops: `ping`, `list_windows`, `minimize_window`, `set_fullscreen`,
//! `close_window`. Force quit needs no daemon op: CoreWindows sends
//! `SIGKILL` to the reported pid directly.
//!
//! The listener is a calloop [`Generic`] source, so requests run inside the
//! compositor event loop with direct `&mut` access to the space. Client
//! reads are bounded (nonblocking, ~200ms max stall) so a silent client
//! can never freeze the compositor.

use std::{
    collections::HashSet,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
};

use smithay::{
    desktop::Window,
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction},
        wayland_protocols::xdg::shell::server::xdg_toplevel::State as XdgState,
        wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    },
    utils::{Point},
};

use crate::{
    state::{get_app_id, get_window_title, window_app_name, window_wl_surface_any},
    TontooCompositor,
};

/// Default socket path, mirrors CoreWindows `DEFAULT_SOCKET_PATH`.
pub const DEFAULT_SOCKET_PATH: &str = "/run/tontoo-windows.sock";

/// Socket path: `WINDOWS_SOCKET` or the default.
pub fn socket_path() -> PathBuf {
    std::env::var("WINDOWS_SOCKET")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH))
}

/// Bind the socket and insert it into the event loop. Shared by the winit
/// and udev backends (call sites: `main.rs`, after state creation).
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
    tracing::info!("windows-ipc listening on {}", path.display());

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
                            tracing::warn!("windows-ipc accept failed: {:?}", e);
                            break;
                        }
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| anyhow::anyhow!("windows-ipc insert_source failed: {:?}", e))?;
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
    // Push queued wayland messages (configure/close) out immediately.
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
        "list_windows" => Ok(serde_json::json!({"windows": list_windows(state)})),
        "minimize_window" => {
            let window = find_window(state, window_arg(request)?)?;
            let name = window_app_name(&window).unwrap_or_else(|| "TontooOS".to_string());
            crate::shell::ssd::minimize_to_dock(state, &window, name);
            Ok(serde_json::Value::Null)
        }
        "set_fullscreen" => {
            let window = find_window(state, window_arg(request)?)?;
            let fullscreen = request
                .get("fullscreen")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "missing fullscreen".to_string())?;
            set_fullscreen(state, &window, fullscreen)?;
            Ok(serde_json::Value::Null)
        }
        "close_window" => {
            let window = find_window(state, window_arg(request)?)?;
            close_window(&window)?;
            Ok(serde_json::Value::Null)
        }
        other => Err(format!("unknown op: {other}")),
    }
}

fn window_arg(request: &serde_json::Value) -> Result<u64, String> {
    request
        .get("window")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing window".to_string())
}

/// Stable daemon-side id for a window, assigned on first use from
/// `next_window_id` and keyed by surface id.
fn ipc_id(state: &mut TontooCompositor, window: &Window) -> Option<u64> {
    let surface = window_wl_surface_any(window)?;
    let key = surface.id();
    if let Some(id) = state.window_ids.get(&key) {
        return Some(*id);
    }
    let id = state.next_window_id;
    state.next_window_id += 1;
    state.window_ids.insert(key, id);
    Some(id)
}

fn find_window(state: &mut TontooCompositor, id: u64) -> Result<Window, String> {
    // Mapped windows first, then minimized (unmapped) ones.
    let mut candidates: Vec<Window> = state.space.elements().cloned().collect();
    candidates.extend(state.minimized_windows.iter().map(|(_, w)| w.clone()));
    for window in candidates {
        if ipc_id(state, &window) == Some(id) {
            return Ok(window);
        }
    }
    Err(format!("unknown window: {id}"))
}

fn list_windows(state: &mut TontooCompositor) -> Vec<serde_json::Value> {
    // Include minimized windows: they are unmapped but still open.
    let mut ordered: Vec<Window> = state.space.elements().cloned().collect();
    ordered.extend(state.minimized_windows.iter().map(|(_, w)| w.clone()));

    let mut seen_surfaces = HashSet::new();
    let mut rows = Vec::new();
    for window in ordered {
        let Some(surface) = window_wl_surface_any(&window) else {
            continue;
        };
        if !seen_surfaces.insert(surface.id()) {
            continue;
        }
        let id = match ipc_id(state, &window) {
            Some(id) => id,
            None => continue,
        };
        let mut row = serde_json::json!({"id": id});
        if let Some(app_id) = window_app_id(&window) {
            row["app_id"] = serde_json::Value::String(app_id);
        }
        if let Some(title) = window_title(&window) {
            row["title"] = serde_json::Value::String(title);
        }
        if let Some(pid) = window_pid(state, &window, &surface) {
            row["pid"] = serde_json::json!(pid);
        }
        rows.push(row);
    }
    // Drop ids of surfaces that are gone so the table cannot grow forever.
    state.window_ids.retain(|key, _| seen_surfaces.contains(key));
    rows
}

fn window_app_id(window: &Window) -> Option<String> {
    if let Some(id) = get_app_id(window) {
        return Some(id);
    }
    x11_class(window)
}

fn window_title(window: &Window) -> Option<String> {
    if let Some(title) = get_window_title(window) {
        return Some(title);
    }
    x11_title(window)
}

/// Owning client pid: Wayland credentials, or the X11 `_NET_WM_PID`.
fn window_pid(state: &TontooCompositor, window: &Window, surface: &WlSurface) -> Option<i64> {
    if let Some(pid) = x11_pid(window) {
        return Some(pid);
    }
    let client = state.display_handle.get_client(surface.id()).ok()?;
    client
        .get_credentials(&state.display_handle)
        .ok()
        .map(|c| c.pid as i64)
}

/// Graceful close: ask the client to close (the app may show a save
/// dialog). X11 goes through `WM_DELETE_WINDOW` via `X11Surface::close`.
fn close_window(window: &Window) -> Result<(), String> {
    if let Some(result) = x11_close(window) {
        return result;
    }
    if let Some(toplevel) = window.toplevel() {
        toplevel.send_close();
        return Ok(());
    }
    Err("window has no close target".to_string())
}

/// Fullscreen set/unset for Wayland (`XdgState::Fullscreen` + output-size
/// configure, geometry saved in `fullscreen_restore`) and X11
/// (`X11Surface::set_fullscreen`).
fn set_fullscreen(
    state: &mut TontooCompositor,
    window: &Window,
    fullscreen: bool,
) -> Result<(), String> {
    if let Some(result) = x11_fullscreen(window, fullscreen) {
        return result;
    }
    let toplevel = window
        .toplevel()
        .ok_or_else(|| "window has no toplevel".to_string())?;
    let surface = window_wl_surface_any(window).ok_or_else(|| "window has no surface".to_string())?;
    let key: ObjectId = surface.id();

    if fullscreen {
        if state.fullscreen_restore.contains_key(&key) {
            return Ok(()); // already fullscreen
        }
        let Some(geo) = state.space.element_geometry(window) else {
            return Err("window has no geometry".to_string());
        };
        let (loc, size) = state
            .space
            .outputs()
            .next()
            .and_then(|o| state.space.output_geometry(o))
            .map(|g| (g.loc, g.size))
            .unwrap_or((Point::from((0, 0)), geo.size));
        state.fullscreen_restore.insert(key, geo);
        toplevel.with_pending_state(|s| {
            s.states.set(XdgState::Fullscreen);
            s.size = Some(size);
        });
        toplevel.send_pending_configure();
        state.space.map_element(window.clone(), loc, true);
        state.space.raise_element(window, true);
        return Ok(());
    }

    let Some(saved) = state.fullscreen_restore.remove(&key) else {
        return Ok(()); // was not fullscreen via IPC
    };
    toplevel.with_pending_state(|s| {
        s.states.unset(XdgState::Fullscreen);
        s.size = Some(saved.size);
    });
    toplevel.send_pending_configure();
    state.space.map_element(window.clone(), saved.loc, true);
    state.space.raise_element(window, true);
    Ok(())
}

// --- X11 helpers (`Window::x11_surface` only exists with the udev
// feature; without it there are no X11 windows, so all helpers
// report "not X11") ---

/// X11 window class, if this is an X11 window.
#[cfg(feature = "udev")]
fn x11_class(window: &Window) -> Option<String> {
    let class = window.x11_surface()?.class();
    if class.is_empty() {
        None
    } else {
        Some(class)
    }
}

/// X11 window title, if this is an X11 window.
#[cfg(feature = "udev")]
fn x11_title(window: &Window) -> Option<String> {
    let title = window.x11_surface()?.title();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// X11 `_NET_WM_PID`, if this is an X11 window.
#[cfg(feature = "udev")]
fn x11_pid(window: &Window) -> Option<i64> {
    window.x11_surface()?.pid().map(|pid| pid as i64)
}

/// Graceful X11 close via `WM_DELETE_WINDOW`. `None` = not an X11 window.
#[cfg(feature = "udev")]
fn x11_close(window: &Window) -> Option<Result<(), String>> {
    let x11 = window.x11_surface()?;
    Some(
        x11
            .close()
            .map_err(|e| format!("x11 close failed: {e:?}")),
    )
}

/// X11 fullscreen switch. `None` = not an X11 window.
#[cfg(feature = "udev")]
fn x11_fullscreen(window: &Window, fullscreen: bool) -> Option<Result<(), String>> {
    let x11 = window.x11_surface()?;
    Some(
        x11
            .set_fullscreen(fullscreen)
            .map_err(|e| format!("x11 fullscreen failed: {e:?}")),
    )
}

#[cfg(not(feature = "udev"))]
fn x11_class(_window: &Window) -> Option<String> {
    None
}

#[cfg(not(feature = "udev"))]
fn x11_title(_window: &Window) -> Option<String> {
    None
}

#[cfg(not(feature = "udev"))]
fn x11_pid(_window: &Window) -> Option<i64> {
    None
}

#[cfg(not(feature = "udev"))]
fn x11_close(_window: &Window) -> Option<Result<(), String>> {
    None
}

#[cfg(not(feature = "udev"))]
fn x11_fullscreen(_window: &Window, _fullscreen: bool) -> Option<Result<(), String>> {
    None
}
