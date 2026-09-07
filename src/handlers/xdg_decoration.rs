use smithay::{
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    utils::Size,
    wayland::shell::xdg::{
        decoration::XdgDecorationHandler,
        ToplevelSurface,
    },
};

use crate::TontooCompositor;

const DEFAULT_WIDTH: i32 = 800;
const DEFAULT_HEIGHT: i32 = 500;

impl XdgDecorationHandler for TontooCompositor {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // CSD: compositor no longer draws a topbar/titlebar — each app draws its own.
        // Advertise ClientSide so GTK/Qt will use client-side decorations.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
            state.size = Some(Size::from((DEFAULT_WIDTH, DEFAULT_HEIGHT)));
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        // Always prefer ClientSide — compositor does not provide server decorations.
        // If the client explicitly requests ServerSide we still give ClientSide
        // (apps must draw their own header bar).
        let _ = mode; // ignored — force CSD
        let effective = Mode::ClientSide;
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(effective);
            // No extra server-side size reservation needed; client includes its
            // own header in its buffer.
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
        toplevel.send_configure();
    }
}

smithay::delegate_xdg_decoration!(TontooCompositor);
