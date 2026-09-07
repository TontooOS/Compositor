use crate::{grabs::resize_grab, state::ClientState, TontooCompositor};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler,
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
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn destroyed(&mut self, _surface: &WlSurface) {
        // Surface destroyed (window closed) - force a redraw so ghost shadows are cleared.
        // Without this, the udev render pump (event-driven) would not notice the dead
        // window until the next dock animation / clock tick, leaving a 33ms+ ghost.
        self.pending_redraw = true;
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().unwrap().wl_surface() == &root)
            {
                window.on_commit();
            }
        };

        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
        resize_grab::handle_commit(&mut self.space, surface);

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
