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
        // Default to ClientSide: every app draws its own header from the
        // system theme (MacTahoe for GTK, qt5ct palette for Qt, portal
        // color-scheme for Chrome/Firefox/Electron). A client that
        // explicitly wants server-side decorations requests it below.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
            state.size = Some(Size::from((DEFAULT_WIDTH, DEFAULT_HEIGHT)));
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        // Honor the explicit client request (KWin-style negotiation).
        // ServerSide is strictly opt-in (e.g. Chrome with "Use system
        // title bar"); nothing is ever forced, so CSD apps can not lose
        // their header.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
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
