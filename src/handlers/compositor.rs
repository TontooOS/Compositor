use crate::{grabs::resize_grab, state::ClientState, TontooCompositor};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client, Resource,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, with_states, CompositorClientState, CompositorHandler,
            CompositorState,
        },
        shm::{ShmHandler, ShmState},
    },
};

use super::xdg_shell;

impl CompositorHandler for TontooCompositor {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(data) = client.get_data::<ClientState>() {
            return &data.compositor_state;
        }
        // The XWayland server's internal client is inserted by smithay with
        // `XWaylandClientData`, not our `ClientState`. Serve its own
        // compositor state so X11 window commits keep working (udev only).
        #[cfg(feature = "udev")]
        if let Some(data) = client.get_data::<smithay::xwayland::XWaylandClientData>() {
            return &data.compositor_state;
        }
        // Unknown client without state data: shared static fallback. A
        // panic here crash-loops the whole compositor via the launchpad
        // supervisor, so never unwrap.
        static FALLBACK: std::sync::OnceLock<CompositorClientState> =
            std::sync::OnceLock::new();
        FALLBACK.get_or_init(CompositorClientState::default)
    }

    fn destroyed(&mut self, _surface: &WlSurface) {
        // Surface destroyed (window closed) - force a redraw so ghost shadows are cleared.
        // Without this, the udev render pump (event-driven) would not notice the dead
        // window until the next animation tick, leaving a 33ms+ ghost.
        self.pending_redraw = true;
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        // TEMP-DEBUG: SHM visibility hunt (revert after diagnosis).
        let buf_info = with_states(surface, |states| {
            let mut guard = states
                .cached_state
                .get::<smithay::wayland::compositor::SurfaceAttributes>();
            let current = guard.current();
            match current.buffer.as_ref() {
            Some(smithay::wayland::compositor::BufferAssignment::NewBuffer(buf)) => {
                smithay::wayland::shm::with_buffer_contents(&buf, |ptr, len, data| {
                    let px0 = if (data.offset as usize) + 4 <= len {
                        // SAFETY: TEMP-DEBUG only; reads 4 bytes inside the
                        // mapped pool while dispatch is blocked in commit.
                        let s = unsafe {
                            std::slice::from_raw_parts(
                                ptr.wrapping_add(data.offset as usize),
                                4,
                            )
                        };
                        [s[0], s[1], s[2], s[3]]
                    } else {
                        [0, 0, 0, 0]
                    };
                    format!(
                        "shm {}x{} stride={} off={} fmt={:?} len={} px0={:?}",
                        data.width,
                        data.height,
                        data.stride,
                        data.offset,
                        data.format,
                        len,
                        px0
                    )
                })
                .ok()
            }
            Some(smithay::wayland::compositor::BufferAssignment::Removed) => {
                Some("buffer removed".to_string())
            }
            None => None,
            }
        });
        if let Some(info) = buf_info {
            tracing::info!("TEMP-DEBUG commit: surf={:?} {}", surface.id(), info);
        }
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| crate::state::window_wl_surface_any(w).as_ref() == Some(&root))
            {
                window.on_commit();
            }
        };

        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
        resize_grab::handle_commit(&mut self.space, surface);

        // Layer-shell surfaces: re-arrange with the newly committed size
        // and drive the configure/ack cycle. Without this the client waits
        // for its initial configure forever and never draws.
        let outputs: Vec<_> = self.space.outputs().cloned().collect();
        for output in &outputs {
            let mut map = smithay::desktop::layer_map_for_output(output);
            if map
                .layer_for_surface(surface, smithay::desktop::WindowSurfaceType::ALL)
                .is_some()
            {
                map.arrange();
                if let Some(layer) = map.layer_for_surface(
                    surface,
                    smithay::desktop::WindowSurfaceType::ALL,
                ) {
                    layer.layer_surface().send_pending_configure();
                }
                self.pending_redraw = true;
            }
        }

        // A client submitted new buffer content: mark the output dirty. The
        // udev render pump picks this up within one frame interval and
        // coalesces bursts of commits into a single render pass.
        self.pending_redraw = true;
    }
}

impl BufferHandler for TontooCompositor {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for TontooCompositor {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}
