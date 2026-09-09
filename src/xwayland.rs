//! XWayland support: run X11 apps (xterm, …) on the TontooOS compositor.
//!
//! smithay boots an `Xwayland` server process (`/usr/bin/Xwayland`, shipped
//! in the `xorg-xwayland` package) and hands us a privileged X11 connection.
//! [`X11Wm`] makes us its window manager; every mapped X11 top-level becomes
//! a regular [`Window`](smithay::desktop::Window) in our
//! [`Space`](smithay::desktop::Space), so rendering, input focus and
//! stacking work exactly like Wayland windows.
//!
//! Only wired up for the udev/DRM backend (the live system). The winit
//! development backend stays Wayland-only for now.

use smithay::{
    desktop::Window,
    reexports::{
        calloop::EventLoop,
        wayland_server::{protocol::wl_surface::WlSurface, Client},
    },
    utils::{Logical, Point, Rectangle},
    wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    xwayland::{
        xwm::{Reorder, ResizeEdge, XwmId},
        X11Surface, X11Wm, XwmHandler, XWayland, XWaylandEvent,
    },
};

use crate::TontooCompositor;

impl XWaylandShellHandler for TontooCompositor {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }
}

/// Runtime state for the XWayland server and its window manager.
#[derive(Default)]
pub struct XWaylandState {
    /// Our window-manager session, created once the server is ready.
    pub xwm: Option<X11Wm>,
    /// Wayland client of the XWayland server, parked here until `Ready`.
    pending_client: Option<Client>,
    /// Cascade offset for initial placement of X11 windows.
    cascade: i32,
}

/// Start the XWayland server. On `Ready` we become its window manager and
/// publish `DISPLAY` so spawned children (and later terminals) find it.
///
/// The `XWayland` handle itself is owned by the event loop from here on;
/// dropping it (loop shutdown) stops the server.
pub fn start_xwayland(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>> {
    // X11 sockets live in /tmp/.X11-unix. smithay binds them directly and
    // fails with ENOENT if the directory is missing (minimal live systems
    // often lack it, and nothing else creates it for us).
    let x11_dir = std::path::Path::new("/tmp/.X11-unix");
    if let Err(err) = std::fs::create_dir_all(x11_dir) {
        tracing::warn!("Could not create /tmp/.X11-unix: {}", err);
    } else if let Err(err) = std::fs::set_permissions(x11_dir, std::os::unix::fs::PermissionsExt::from_mode(0o1777)) {
        tracing::warn!("Could not chmod /tmp/.X11-unix: {}", err);
    }

    let (xwayland, client) = XWayland::spawn(
        &state.display_handle,
        None,
        // No glamor on llvmpipe VMs: force software presentation, otherwise
        // Xwayland fails to initialize its EGL backend on render-node-less GPUs.
        [("XWAYLAND_NO_GLAMOR", "1")],
        Vec::<String>::new(),
        true,
        std::process::Stdio::null(),
        std::process::Stdio::null(),
        |_| {},
    )?;

    state.xwayland_state.pending_client = Some(client);
    // X11Wm::start_wm demands a LoopHandle<'static>. The event loop outlives
    // the whole compositor run (main returns only after it finishes), and the
    // extended handle is used solely inside loop callbacks, so extending the
    // borrow is sound. This is the standard workaround for smithay 0.7 XWM
    // startup from inside an event-source callback.
    // SAFETY: see above; never used after the event loop is dropped.
    let loop_handle: smithay::reexports::calloop::LoopHandle<'static, TontooCompositor> =
        unsafe { std::mem::transmute(event_loop.handle()) };

    event_loop.handle().insert_source(
        xwayland,
        move |event, _, state: &mut TontooCompositor| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => {
                let dpy = format!(":{display_number}");
                unsafe { std::env::set_var("DISPLAY", &dpy) };
                tracing::info!("XWayland ready on DISPLAY={}", dpy);

                let Some(client) = state.xwayland_state.pending_client.take() else {
                    tracing::error!("XWayland signalled ready twice, ignoring");
                    return;
                };
                match X11Wm::start_wm(
                    loop_handle.clone(),
                    &state.display_handle,
                    x11_socket,
                    client,
                ) {
                    Ok(xwm) => {
                        tracing::info!("X11 window manager session started");
                        state.xwayland_state.xwm = Some(xwm);
                        state.request_redraw();
                    }
                    Err(err) => {
                        tracing::error!("Failed to start X11 WM: {:?}", err);
                    }
                }
            }
            XWaylandEvent::Error => {
                tracing::error!("XWayland failed to start (is /usr/bin/Xwayland installed?)");
            }
        },
    )?;

    Ok(())
}

impl XwmHandler for TontooCompositor {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwayland_state
            .xwm
            .as_mut()
            .expect("XWM event before session started")
    }

    fn new_window(&mut self, _xwm: XwmId, window: X11Surface) {
        tracing::debug!("X11 new window {:?}", window.window_id());
    }

    fn new_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        tracing::debug!("X11 new override-redirect window {:?}", window.window_id());
    }

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(err) = window.set_mapped(true) {
            tracing::warn!("X11 set_mapped failed: {:?}", err);
            return;
        }
        // Cascade initial placement like a classic WM.
        let n = self.xwayland_state.cascade;
        self.xwayland_state.cascade = (n + 1) % 8;
        let geo = window.geometry();
        let pos = Point::<i32, Logical>::from((100 + n * 40, 80 + n * 30));
        if let Err(err) = window.configure(Rectangle::new(pos, geo.size)) {
            tracing::debug!("X11 initial configure failed: {:?}", err);
        }
        let mapped = Window::new_x11_window(window);
        self.space.map_element(mapped, pos, true);
        self.request_redraw();
        tracing::info!("X11 window mapped at {:?}", pos);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        // Menus, tooltips, dropdowns: show where the client put them.
        let pos = window.geometry().loc;
        let mapped = Window::new_x11_window(window);
        self.space.map_element(mapped, pos, false);
        self.request_redraw();
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        remove_x11_window(self, &window);
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        remove_x11_window(self, &window);
    }

    #[allow(clippy::too_many_arguments)]
    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let mut geo = window.geometry();
        if let Some(x) = x {
            geo.loc.x = x;
        }
        if let Some(y) = y {
            geo.loc.y = y;
        }
        if let Some(w) = w {
            geo.size.w = w as i32;
        }
        if let Some(h) = h {
            geo.size.h = h as i32;
        }
        if let Err(err) = window.configure(geo) {
            tracing::debug!("X11 configure failed: {:?}", err);
            return;
        }
        // Move the space element if the client asked for a new position.
        if x.is_some() || y.is_some() {
            if let Some(existing) = find_x11_window(&self.space, &window) {
                self.space.map_element(existing, geo.loc, false);
            }
        }
        self.request_redraw();
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<smithay::reexports::x11rb::protocol::xproto::Window>,
    ) {
        self.request_redraw();
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _resize_edge: ResizeEdge,
    ) {
        // v1: server-side resizing for X11 windows is not implemented.
        tracing::debug!("X11 resize request ignored (v1)");
    }

    fn move_request(&mut self, _xwm: XwmId, _window: X11Surface, _button: u32) {
        // v1: server-side moving for X11 windows is not implemented.
        tracing::debug!("X11 move request ignored (v1)");
    }
}

fn find_x11_window(
    space: &smithay::desktop::Space<Window>,
    window: &X11Surface,
) -> Option<Window> {
    let id = window.window_id();
    space.elements().find_map(|w| {
        let is_match = w
            .x11_surface()
            .map(|x| x.window_id() == id)
            .unwrap_or(false);
        is_match.then(|| w.clone())
    })
}

fn remove_x11_window(state: &mut TontooCompositor, window: &X11Surface) {
    if let Some(existing) = find_x11_window(&state.space, window) {
        state.space.unmap_elem(&existing);
        state.request_redraw();
        tracing::info!("X11 window unmapped");
    }
}

/// App name for the menubar / dock, mirroring the Wayland lookup order.
pub fn x11_app_name(window: &Window) -> Option<String> {
    let x11 = window.x11_surface()?;
    let title = x11.title();
    if !title.is_empty() {
        return Some(title);
    }
    let class = x11.class();
    if !class.is_empty() {
        return Some(class);
    }
    None
}

/// Focused [`WlSurface`] of a mapped window, Wayland or X11.
pub fn window_wl_surface(window: &Window) -> Option<WlSurface> {
    if let Some(toplevel) = window.toplevel() {
        return Some(toplevel.wl_surface().clone());
    }
    window.x11_surface().and_then(|x| x.wl_surface())
}
